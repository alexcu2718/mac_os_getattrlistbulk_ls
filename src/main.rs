use std::{env, ffi::OsStr};
use fdf::FileType;
use core::ffi::CStr;
use std::os::unix::ffi::OsStrExt;
use std::io;
use core::cell::Cell;
// macOS-specific constants not in libc crate
const ATTR_CMN_ERROR: u32 = 0x20000000;
const VREG: u8 = 1; //DT_REG !=THIS (weird convention)
const VDIR: u8 = 2;
const VLNK: u8 = 5;
const VBLK: u8 = 3;
const VCHR: u8 = 4;
const VFIFO: u8 = 6;
const VSOCK: u8 = 7;

mod test;

// File entry information (a beta version to match API in my own crate)
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct DirEntryBeta {
    path: Box<CStr>,
    file_type: FileType,
    file_name_index: usize,
    depth: u32,
    inode: u64,
    is_traversible_cache: Cell<Option<bool>>,
}


// hacky way to get filetype yay
fn get_filetype(obj_type: u8) -> FileType {
    match obj_type {
        VREG => FileType::RegularFile,
        VDIR => FileType::Directory,
        VLNK => FileType::Symlink,
        VBLK => FileType::BlockDevice,
        VCHR => FileType::CharDevice,
        VFIFO | VSOCK => FileType::Socket,
        _ => FileType::Unknown
    }
}

fn main() {
    let args: Vec<String> = env::args().collect();

    if args.len() < 2 {
        eprintln!("usage: {} directory", args[0]);
        std::process::exit(1);
    }

    let root_dir: &OsStr = OsStrExt::from_bytes(args[1].as_bytes());
    let direntry = fdf::DirEntry::new(root_dir).expect("i am not fixing this yet");
    let result =  DirIterator::new(&direntry);

    match result {
        Ok(entries) => {
            for entry in entries {
                
                let entry_formatted = entry.path.to_bytes();
                let file_name=String::from_utf8_lossy(&entry_formatted[entry.file_name_index..]);
                let entry_string=String::from_utf8_lossy(entry_formatted);
                println!("{:<60} {:<15} {:<12} {}", 
                    entry_string, 
                    entry.file_type, 
                    entry.inode,
                    file_name
                 
                );
            }
            }
        
        Err(_) => {
            eprintln!("{}", args[0]);
            std::process::exit(1);
        }
    }
}

#[inline]
fn init_from_direntry(dir_path: &fdf::DirEntry) -> (Vec<u8>, usize) {
    let dir_path_in_bytes = dir_path.as_bytes();
    let mut base_len = dir_path_in_bytes.len();
    let is_root = dir_path_in_bytes == b"/";
    let needs_slash: usize = usize::from(!is_root);

    const MAX_SIZED_DIRENT_LENGTH: usize = 2 * 256; // 2* `NAME_MAX`

    let mut path_buffer = vec![0u8; base_len + needs_slash + MAX_SIZED_DIRENT_LENGTH];
    let buffer_ptr = path_buffer.as_mut_ptr();
    
    unsafe { 
        core::ptr::copy_nonoverlapping(dir_path_in_bytes.as_ptr(), buffer_ptr, base_len);
        *buffer_ptr.add(base_len) = b'/' * (!is_root as u8) 
        

    };

    base_len += needs_slash;
    (path_buffer, base_len)
}

#[inline]
fn append_filename_and_get_index<'a>(buffer: &'a mut [u8], base_len: usize, filename: &'a [u8]) -> (&'a CStr, usize) {
    let filename_len = filename.len();
    
    unsafe {
        core::ptr::copy_nonoverlapping(
            filename.as_ptr(),
            buffer.as_mut_ptr().add(base_len),
            filename_len
        );
        *buffer.as_mut_ptr().add(base_len + filename_len) = 0;
        
        let full_path = CStr::from_bytes_with_nul_unchecked(
            &buffer.get_unchecked(..base_len + filename_len + 1)
        );
        
        (full_path, base_len)
    }
}
pub struct DirIterator {
    dirfd: i32,
    attrlist: libc::attrlist,
    attrbuf: [u8; 128 * 1024],
    current_offset: usize,
    remaining_entries: i32,
    path_buffer: Vec<u8>,
    base_len: usize,
    depth: u32,
    is_finished: bool,
}

impl DirIterator {
    pub fn new(dir_path: &fdf::DirEntry) -> Result<Self, io::Error> {
        let c_path = dir_path.as_ptr();
        const FLAGS: i32 = libc::O_CLOEXEC | libc::O_DIRECTORY | libc::O_NONBLOCK;
        let dirfd = unsafe { libc::open(c_path.cast(), FLAGS) };
        
        if dirfd == -1 {
            return Err(io::Error::last_os_error());
        }

        let attrlist = libc::attrlist {
            bitmapcount: libc::ATTR_BIT_MAP_COUNT as u16,
            reserved: 0,
            commonattr: libc::ATTR_CMN_RETURNED_ATTRS
                | libc::ATTR_CMN_NAME
                | ATTR_CMN_ERROR
                | libc::ATTR_CMN_OBJTYPE
                | libc::ATTR_CMN_FILEID,
            volattr: 0,
            dirattr: 0,
            fileattr: 0, 
            forkattr: 0,
        };

        let attrbuf = [0u8; 128 * 1024];
        let (path_buffer, base_len) = init_from_direntry(dir_path);
        let depth = (dir_path.depth() + 1) as u32;

        Ok(Self {
            dirfd,
            attrlist,
            attrbuf,
            current_offset: 0,
            remaining_entries: 0,
            path_buffer,
            base_len,
            depth,
            is_finished: false,
        })
    }

    fn get_next_entry(&mut self) -> Option<DirEntryBeta> {
        // If buffer is empty, read next batch
        if self.remaining_entries <= 0 && !self.is_finished {
            match self.read_next_batch() {
                Ok(0) => {
                    self.is_finished = true;
                    return None;
                }
                Ok(_) => {} // We have new entries
                Err(_) => {
                    self.is_finished = true;
                    return None;
                }
            }
        }

        if self.remaining_entries <= 0 {
            return None;
        }

        unsafe {
            let entry_ptr = self.attrbuf.as_ptr().add(self.current_offset);
            let entry_length = std::ptr::read(entry_ptr as *const u32);
            
            // Check bounds
            if self.current_offset + entry_length as usize > self.attrbuf.len() {
                self.remaining_entries = 0;
                return None;
            }

            let mut field_ptr = entry_ptr.add(std::mem::size_of::<u32>());
            let returned_attrs = std::ptr::read(field_ptr as *const libc::attribute_set_t);
            field_ptr = field_ptr.add(std::mem::size_of::<libc::attribute_set_t>());

            // Extract filename
            let mut filename: Option<&[u8]> = None;
            if returned_attrs.commonattr & libc::ATTR_CMN_NAME != 0 {
                let name_start = field_ptr;
                let name_info = std::ptr::read(field_ptr as *const libc::attrreference_t);
                field_ptr = field_ptr.add(std::mem::size_of::<libc::attrreference_t>());
                let name_ptr = name_start.add(name_info.attr_dataoffset as usize);

                if name_info.attr_length > 0 {
                    let name_length = (name_info.attr_length - 1) as usize;
                    let name_slice = std::slice::from_raw_parts(name_ptr, name_length);
                    
                    // Skip . and ..
                    if name_slice != b"." && name_slice != b".." {
                        filename = Some(name_slice);
                    }
                }
            }

            // Skip entries without filenames or with errors
            if filename.is_none() || (returned_attrs.commonattr & ATTR_CMN_ERROR != 0) {
                if returned_attrs.commonattr & ATTR_CMN_ERROR != 0 {
                    std::ptr::read(field_ptr as *const u32)
                } else {
                    0
                };
                
                // Skip this entry
                self.current_offset += entry_length as usize;
                self.remaining_entries -= 1;
                return self.get_next_entry();
            }

            // Get object type
            let obj_type = if returned_attrs.commonattr & libc::ATTR_CMN_OBJTYPE != 0 {
                let obj_type = std::ptr::read(field_ptr);
                field_ptr = field_ptr.add(std::mem::size_of::<u32>());
                obj_type
            } else {
                libc::DT_UNKNOWN
            };

            // Get inode
            let inode = if returned_attrs.commonattr & libc::ATTR_CMN_FILEID != 0 {
                std::ptr::read(field_ptr as *const u64)
            } else {
                0
            };

            // Move to next entry
            self.current_offset += entry_length as usize;
            self.remaining_entries -= 1;

       
            if let Some(name) = filename {
                let (full_path, file_name_index) = append_filename_and_get_index(
                    &mut self.path_buffer, 
                    self.base_len, 
                    name
                );
                let file_type = get_filetype(obj_type);

                Some(DirEntryBeta {
                    path: full_path.into(),
                    file_type,
                    file_name_index,
                    depth: self.depth,
                    inode,
                    is_traversible_cache: Cell::new(None)
                })
            } else {
                self.get_next_entry()
            }
        }
    }

    fn read_next_batch(&mut self) -> Result<i32, io::Error> {
        let retcount = unsafe {
            libc::getattrlistbulk(
                self.dirfd,
                &mut self.attrlist as *mut libc::attrlist as *mut libc::c_void,
                self.attrbuf.as_mut_ptr() as *mut libc::c_void,
                self.attrbuf.len(),
                0,
            )
        };

        if retcount < 0 {
            return Err(io::Error::last_os_error());
        }

        if retcount == 0 {
            self.is_finished = true;
        }

        self.remaining_entries = retcount;
        self.current_offset = 0;
        Ok(retcount)
    }
}

impl Iterator for DirIterator {
    type Item = DirEntryBeta;

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        self.get_next_entry()
    }
}

impl Drop for DirIterator {
    fn drop(&mut self) {
        unsafe {
            libc::close(self.dirfd);
        }
    }
}
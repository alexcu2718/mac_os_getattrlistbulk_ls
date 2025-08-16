use std::env;
use fdf::FileType;
use fdf::cstr;
pub type SlimmerBytes = Box<[u8]>;
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
struct DirEntryBeta {
    path: SlimmerBytes,
    file_type: FileType,
    //file_name_index: u16,
    // depth:u8 
    inode: u64,
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

    let root_dir = &args[1];

    let result = get_dir_info(&root_dir);

    match result {
        Ok(entries) => {
            for entry in entries {
                let entry_formatted=String::from_utf8_lossy(&entry.path);
                println!("{:<60} {:<15} {:<12}", 
                    entry_formatted, 
                    entry.file_type, 
                    entry.inode,
                );
            }
        }
        Err(e) => {
            eprintln!("{}: {}", args[0], e);
            std::process::exit(1);
        }
    }
}


fn get_dir_info(path: &str) -> Result<Vec<DirEntryBeta>, String> {
    // Open directory
    let c_path:*const u8 = unsafe{cstr!(path)};
    const FLAGS: i32 = libc::O_CLOEXEC | libc::O_DIRECTORY | libc::O_NONBLOCK;
    let dirfd = unsafe { libc::open(c_path.cast(), FLAGS) };
    if dirfd == -1 {
        let errno = unsafe { *libc::__error() };
        let error_msg = match errno {
            libc::ENOENT => "No such file or directory",
            libc::EACCES => "Permission denied",
            libc::ENOTDIR => "Not a directory",
            _ => "Cannot access directory",
        };
        return Err(format!("{}: {}", path, error_msg));
    }

    // Set up attribute list for getattrlistbulk
    let mut attrlist = libc::attrlist {
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

    let mut attrbuf = [0u8; 128 * 1024]; //THIS BUFFER IS PROBABLY *WAY TOO BIG*, i need to read more about this to address it.
    let mut entries = Vec::new();

    loop {
        let retcount = unsafe {
            libc::getattrlistbulk(
                dirfd,
                &mut attrlist as *mut libc::attrlist as *mut libc::c_void,
                attrbuf.as_mut_ptr() as *mut libc::c_void,
                attrbuf.len(),
                0,
            )
        };

        if retcount <= 0 {
            if retcount < 0 {
                let errno = unsafe { *libc::__error() };
                let error_msg = match errno {
                    libc::EACCES => "Permission denied",
                    libc::ENOENT => "No such file or directory",
                    _ => "Cannot read directory contents",
                };
                return Err(format!("{}: {}", path, error_msg));
            }
            break;
        }

        // Parse attribute buffer
        let mut entry_ptr = attrbuf.as_ptr();
        for _ in 0..retcount {
            unsafe {
                 let entry_length = std::ptr::read(entry_ptr as *const u32);
                let mut field_ptr = entry_ptr.add(std::mem::size_of::<u32>());

                // Read returned attributes bitmask
                let returned_attrs =
                  std::ptr::read(field_ptr as *const libc::attribute_set_t);


                field_ptr = field_ptr.add(std::mem::size_of::<libc::attribute_set_t>());

                // Extract filename
                //this needs to be all erased and rewritten soon, sigh.
                let mut filename: Option<String> = None;
                if returned_attrs.commonattr & libc::ATTR_CMN_NAME != 0 {
                    let name_start = field_ptr; // Save start of attrreference_t
                    let name_info =
                        std::ptr::read(field_ptr as *const libc::attrreference_t);
                    field_ptr = field_ptr.add(std::mem::size_of::<libc::attrreference_t>());
                    let name_ptr = name_start.add(name_info.attr_dataoffset as usize);

                    if name_info.attr_length > 0 {
                        let name_slice = &*std::ptr::slice_from_raw_parts(
                            name_ptr,
                            (name_info.attr_length - 1) as usize,
                        );
                        
                            if name_slice==b"." || name_slice==b".." {
                                entry_ptr = entry_ptr.add(entry_length as usize);
                                continue;
                            }
                            filename = Some(String::from_utf8_lossy(name_slice).to_string());
                            //this is hacky too.
                        
                    }
                }

                // Check for errors
                if returned_attrs.commonattr & ATTR_CMN_ERROR != 0 {
                    let error_code = std::ptr::read(field_ptr as *const u32);
                    field_ptr = field_ptr.add(std::mem::size_of::<u32>());
                    if error_code != 0 {
                        if let Some(name) = &filename {
                            eprintln!("cannot access '{}/{}': error {}", path, name, error_code);
                        }
                        entry_ptr = entry_ptr.add(entry_length as usize);
                        continue;
                    }
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
                    let inode = std::ptr::read(field_ptr as *const u64);
                    inode
                } else {
                    0
                };

                // Create entry for all file types
                if let Some(name) = filename { //deleting this soon.
                    let full_path = if path == "/" {
                        format!("/{}", name)
                    } else {
                        format!("{}/{}", path, name)
                    };

                    let file_type = get_filetype(obj_type);

                    let entry = DirEntryBeta {
                        path: full_path.as_bytes().into(), //im slowly patching the API to meet my own, this is SO stupid.
                        file_type,
                        //file_name_index TODO!
                        //depth TODO!
                        inode,
                    };

                    entries.push(entry);

                  
                }

                // Move to next entry
                entry_ptr = entry_ptr.add(entry_length as usize);
            }
        }
    }

    // Close directory
    unsafe {
        libc::close(dirfd);
    }

    Ok(entries )
}

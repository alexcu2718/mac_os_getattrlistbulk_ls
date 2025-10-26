use std::{env, ffi::OsStr};
use std::os::unix::prelude::OsStrExt;


use redir::*;

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

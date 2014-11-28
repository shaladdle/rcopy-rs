#![feature(slicing_syntax)]
#![feature(if_let)]

extern crate rcopy;

use std::io::fs;
use std::os;
use std::io::FileStat;
use std::io::{TypeFile, TypeDirectory};

fn main() {
    let (src_dir, dst_dir) = match os::args()[] {
        [_, ref s, ref d] => (Path::new(s), Path::new(d)),
        _ => {
            println!("usage: rcopy src_dir dst_dir");
            return;
        },
    };

    let mut elems = match fs::walk_dir(&src_dir) {
        Ok(x) => x,
        Err(e) => {
            println!("Could not walk directory \"{}\": {}", src_dir.display(), e);
            std::os::set_exit_status(-1);
            return;
        }
    };
    for src_file in elems {
        let file_size = match fs::stat(&src_file) {
            Ok(info) =>  {
                if info.kind == TypeDirectory {
                    continue;
                }
                info.size
            },
            Err(e) => {
                println!("Couldn't stat file \"{}\" while walking: {}", src_file.display(), e);
                continue;
            },
        };
        let rel_file = match src_file.path_relative_from(&src_dir) {
            Some(p) => p,
            None => {
                println!("Couldn't get relative path for \"{}\" relative to \"{}\"", src_file.display(), src_dir.display());
                std::os::set_exit_status(-1);
                return;
            },
        };
        let dst_file = dst_dir.join(&rel_file);

        // If the file already exists and it has the right file size, assume it was copied
        // properly.
        //
        // TODO: what if there is a progress file there? Do we remove it? That's kind of an
        // implementation detail :(.
        match fs::stat(&dst_file) {
            Ok(FileStat{size: existing_file_size, ..}) => {
                if existing_file_size == file_size {
                    println!("[ {}/{} ] {} (skipped)", file_size, file_size, rel_file.display());
                    continue;
                }
            },
            _ => (),
        }

        // Create the containing directory for the destination file if it doesn't exist.
        let dst_file_dir = dst_file.dir_path();
        match fs::stat(&dst_file_dir) {
            Ok(FileStat{kind: TypeFile, ..}) => {
                println!("Want \"{}\" to be a directory, but it already exists as a file", dst_file_dir.display());
                std::os::set_exit_status(-1);
                return;
            }
            Err(_) => {
                if let Err(e) = fs::mkdir(&dst_file_dir, std::io::USER_DIR) {
                    println!("Couldn't create destination file's directory \"{}\": {}", dst_file_dir.display(), e);
                    std::os::set_exit_status(-1);
                    return
                }
            }
            _ => (),
        }

        // Start the async copy
        let progress_rx = rcopy::resumable_file_copy(&dst_file, &src_file);

        // Wait for the copy to be complete, printing progress as it goes
        for progress in progress_rx.iter() {
            print!("\r[ {}/{} ] {}", progress.current, progress.total, rel_file.display());
        }
        print!("\n");
    }
}

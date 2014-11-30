#![feature(slicing_syntax)]
#![feature(if_let)]

extern crate rcopy;

use std::io;
use std::io::fs;
use std::os;

fn calc_percent(current: f64, total: f64) -> Option<f64> {
    if total == 0f64 {
        return None;
    }
    Some(100f64 * current / total)
}

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
                if info.kind == io::TypeDirectory {
                    continue;
                }
                info.size
            },
            Err(e) => {
                println!("Couldn't stat file \"{}\" while walking: {}", src_file.display(), e);
                continue;
            },
        };
        let rel_file = src_file.path_relative_from(&src_dir).expect("Couldn't do path_relative_from, something terrible has happened");
        let dst_file = dst_dir.join(&rel_file);

        // If the file already exists and it has the right file size, assume it was copied
        // properly.
        //
        // TODO: what if there is a progress file there? Do we remove it? That's kind of an
        // implementation detail :(.
        match fs::stat(&dst_file) {
            Ok(io::FileStat{size: existing_file_size, ..}) => {
                if existing_file_size == file_size {
                    println!("[ {:3.2}% ] {} (skipped)", 100f64, rel_file.display());
                    continue;
                }
            },
            _ => (),
        }

        // Create the containing directory for the destination file if it doesn't exist.
        let dst_file_dir = dst_file.dir_path();
        match fs::stat(&dst_file_dir) {
            Ok(io::FileStat{kind: io::TypeFile, ..}) => {
                println!("Want \"{}\" to be a directory, but it already exists as a file", dst_file_dir.display());
                std::os::set_exit_status(-1);
                return;
            },
            Err(io::IoError{kind: io::FileNotFound, ..}) => {
                if let Err(e) = fs::mkdir_recursive(&dst_file_dir, std::io::USER_DIR) {
                    println!("Couldn't create destination file's directory \"{}\": {}", dst_file_dir.display(), e);
                    std::os::set_exit_status(-1);
                    return;
                }
            },
            Err(e) => {
                println!("Couldn't stat destination file directory \"{}\": {}", dst_file_dir.display(), e);
                std::os::set_exit_status(-1);
                return;
            },
            _ => (),
        }

        // Start the async copy
        let status_rx = rcopy::resumable_file_copy(&dst_file, &src_file);
        // Wait for the copy to be complete, printing progress as it goes
        for status in status_rx.iter() {
            let progress = match status {
                Err(e) => {
                    println!("Non-retryable error encountered while copying: {}", e);
                    std::os::set_exit_status(-1);
                    return;
                },
                Ok(p) => p,
            };
            let percent = calc_percent(progress.current as f64, progress.total as f64).unwrap_or(0f64);
            print!("[ {:3.2}% ] {}\r", percent, rel_file.display());
        }
        print!("\n");
    }
}

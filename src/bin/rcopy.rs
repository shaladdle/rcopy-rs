#![feature(slicing_syntax)]
#![feature(if_let)]
#![feature(globs)]

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

enum StringError {
    Message(String),
    IoError(io::IoError),
}

impl std::fmt::Show for StringError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter) -> Result<(), std::fmt::Error> {
        use StringError::*;
        match *self {
            Message(ref s) => s.fmt(formatter),
            IoError(ref e) => e.fmt(formatter),
        }
    }
}

enum MkDstDirError {
    FoundRegularFileNotDir(Path),
    MkDirFailed(io::IoError),
    IoError(io::IoError),
}

impl std::fmt::Show for MkDstDirError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter) -> Result<(), std::fmt::Error> {
        use MkDstDirError::*;
        match *self {
            FoundRegularFileNotDir(ref dir_path) =>
                format!("destination's file directory \"{}\" is a regular file, move this file somewhere else to continue", dir_path.display()).fmt(formatter),
            MkDirFailed(ref io_error) => format!("failed to create directory: {}", io_error).fmt(formatter),
            IoError(ref io_error) => io_error.fmt(formatter),
        }
    }
}

// TODO: Have a better check than "the destination file is the same size as the source file" for
// determining that the file was already copied.
fn file_already_copied(fpath: &Path, file_size: u64) -> Result<bool, StringError> {
    match fs::stat(fpath) {
        Ok(io::FileStat{kind: io::TypeDirectory, ..}) =>
            Err(StringError::Message(format!("destination file \"{}\" exists and is a directory", fpath.display()))),
        Ok(io::FileStat{size: existing_file_size, ..}) => Ok(existing_file_size == file_size),
        Err(io::IoError{kind: io::FileNotFound, ..}) => Ok(false),
        Err(e) => Err(StringError::IoError(e)),
    }
}

fn try_mkdir(fpath: &Path) -> Result<(), MkDstDirError> {
    let fdir = fpath.dir_path();
    match fs::stat(&fdir) {
        Ok(io::FileStat{kind: io::TypeFile, ..}) => Err(MkDstDirError::FoundRegularFileNotDir(fdir.clone())),
        Err(io::IoError{kind: io::FileNotFound, ..}) => {
            if let Err(e) = fs::mkdir_recursive(&fdir, std::io::USER_DIR) {
                return Err(MkDstDirError::MkDirFailed(e));
            }
            Ok(())
        },
        Err(e) => Err(MkDstDirError::IoError(e)),
        _ => Ok(()),
    }
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
        match file_already_copied(&dst_file, file_size) {
            Ok(true) => {
                println!("[ {:3.2}% ] {} (skipped)", 100f64, rel_file.display());
                continue;
            }
            Ok(false) => (),
            Err(e) => {
                println!("Error: {}", e);
                std::os::set_exit_status(-1);
                return;
            }
        }

        if let Err(e) = try_mkdir(&dst_file) {
            println!("Error: {}", e);
            std::os::set_exit_status(-1);
            return;
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
            std::io::stdio::flush();
        }
        print!("\n");
    }
}

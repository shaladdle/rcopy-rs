#![feature(slicing_syntax)]
#![feature(if_let)]
#![feature(globs)]

extern crate rcopy;

use std::io;
use std::error;
use std::io::fs;
use std::os;
use std::time;
use std::comm;

struct TimeMeasure {
    tx_done: Sender<()>,
    rx_time: Receiver<time::Duration>,
}

impl TimeMeasure {
    fn start() -> TimeMeasure {
        let (tx_done, rx_done) = comm::channel();
        let (tx_time, rx_time) = comm::channel();
        spawn(proc() {
            tx_time.send(time::Duration::span(|| {
                rx_done.recv();
            }));
        });
        TimeMeasure{
            tx_done: tx_done,
            rx_time: rx_time,
        }
    }

    fn done(&self) -> time::Duration {
        self.tx_done.send(());
        self.rx_time.recv()
    }
}

fn calc_percent(current: f64, total: f64) -> Option<f64> {
    if total == 0f64 {
        return None;
    }
    Some(100f64 * current / total)
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

impl error::FromError<io::IoError> for StringError {
    fn from_error(io_error: io::IoError) -> StringError {
        StringError::IoError(io_error)
    }
}

impl error::FromError<MkDstDirError> for StringError {
    fn from_error(err: MkDstDirError) -> StringError {
        StringError::Message(format!("{}", err))
    }
}

impl error::FromError<rcopy::RCopyError> for StringError {
    fn from_error(err: rcopy::RCopyError) -> StringError {
        StringError::Message(format!("{}", err))
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

fn copy_file(dst_file: &Path, src_file: &Path, rel_file: &Path) -> Result<(), StringError> {
    let mut bytes_to_copy : i64 = 0;
    let measure = TimeMeasure::start();
    // Start the async copy
    let status_rx = rcopy::resumable_file_copy(dst_file, src_file);
    // Wait for the copy to be complete, printing progress as it goes
    for status in status_rx.iter() {
        let progress = try!(status);
        // Set this to the total number of bytes we will copy in this call to
        // resumable_file_copy. This is used to measure transfer speed.
        if bytes_to_copy == 0 {
            bytes_to_copy = progress.total - progress.current;
        }
        let percent = calc_percent(progress.current as f64, progress.total as f64).unwrap_or(0f64);
        print!("\r[ {:6.2}% ] {}", percent, rel_file.display());
        std::io::stdio::flush();
    }
    // Compute the average transfer speed for this invocation of resumable_file_copy.
    let ms = measure.done().num_milliseconds();
    let mb_per_ms = (bytes_to_copy / (1 << 20)) as f64 / ms as f64;
    let mb_per_second = mb_per_ms * 1000f64;
    print!(" ({:.2}MB/s)", mb_per_second);
    print!("\n");
    Ok(())
}

fn copy_directory(dst_dir: &Path, src_dir: &Path) -> Result<(), StringError> {
    let mut elems = try!(fs::walk_dir(src_dir));
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
        let rel_file = src_file.path_relative_from(src_dir).expect("Couldn't do path_relative_from, something terrible has happened");
        let dst_file = dst_dir.join(&rel_file);

        // If the file already exists and it has the right file size, assume it was copied
        // properly.
        if try!(file_already_copied(&dst_file, file_size)) {
            println!("[ {:3.2}% ] {} (skipped)", 100f64, rel_file.display());
            continue;
        }

        try!(try_mkdir(&dst_file));
        try!(copy_file(&dst_file, &src_file, &rel_file));
    }
    Ok(())
}

fn main() {
    let (src_dir, dst_dir) = match os::args()[] {
        [_, ref s, ref d] => (Path::new(s), Path::new(d)),
        _ => {
            println!("usage: rcopy src_dir dst_dir");
            return;
        },
    };

    if let Err(e) = copy_directory(&dst_dir, &src_dir) {
        println!("Error: {}", e);
        std::os::set_exit_status(-1);
    }
}

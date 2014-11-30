#![feature(unboxed_closures)]
#![feature(slicing_syntax)]
#![feature(if_let)]
#![feature(globs)]

use std::error;
use std::io::net::ip::ToSocketAddr;
use std::io::net::ip::SocketAddr;
use std::io::{Reader,Writer};
use std::io::fs;
use std::io;
use std::comm::channel;
use std::time::duration::Duration;
use std::vec::Vec;

const CHUNK_SIZE : uint = 8 << 20; // 8MiB

pub enum ProgFileInvalidCause {
    WrongEncodedSize(u64),
    PosOutOfRange{position: i64, file_size: i64},
}

impl std::fmt::Show for ProgFileInvalidCause {
    fn fmt(&self, formatter: &mut std::fmt::Formatter) -> Result<(), std::fmt::Error> {
        use ProgFileInvalidCause::*;
        let message = match *self {
            WrongEncodedSize(actual_size) =>
                format!("Due to how position is encoded, the progress file should be 8 bytes.
                         Its actual size is {}, so its format must be wrong", actual_size),
            PosOutOfRange{position, file_size} =>
                format!("The position read from the progress file is out of range. The position
                         in the progress file was {}. The actual file size is {}.", position, file_size),
        };
        message.fmt(formatter)
    }
}

pub enum RCopyError{
    NotImplemented,
    ProgFileInvalid{fpath: Path, cause: ProgFileInvalidCause},
    IoError(io::IoError),
}

impl RCopyError {
    fn is_retryable(&self) -> bool {
        match *self {
            RCopyError::ProgFileInvalid{..} => false,
            _ => true,
        }
    }
}

impl error::FromError<io::IoError> for RCopyError {
    fn from_error(io_error: io::IoError) -> RCopyError {
        RCopyError::IoError(io_error)
    }
}

impl error::Error for RCopyError {
    fn description(&self) -> &str {
        use RCopyError::*;
        match *self {
            NotImplemented => "not implemented",
            ProgFileInvalid{..} => "invalid progress file",
            IoError(ref e) => e.description(),
        }
    }

    fn detail(&self) -> Option<String> {
        use RCopyError::*;
        match *self {
            ProgFileInvalid{ref fpath, ref cause} => {
                Some(format!("progress file: \"{}\", cause: {}", fpath.display(), cause))
            }
            IoError(ref e) => e.detail(),
            _ => None,
        }
    }

    fn cause(&self) -> Option<&error::Error> {
        if let RCopyError::IoError(ref e) = *self {
            return e.cause();
        }
        None
    }
}

pub type RCopyResult<T> = Result<T, RCopyError>;

// TODO: Move this daemon stuff to another file.
#[allow(dead_code)]
pub struct RCopyDaemon {
    hostport: SocketAddr,
}

impl RCopyDaemon {
    pub fn new<A: ToSocketAddr>(hostport: A) -> RCopyResult<RCopyDaemon> {
        Ok(RCopyDaemon{hostport: try!(hostport.to_socket_addr())})
    }
    pub fn serve(&mut self) -> RCopyError {
        RCopyError::NotImplemented
    }
}

pub struct ProgressInfo {
    pub current: i64,
    pub total: i64,
}

// Retries f as long as it returns a 'retryable' error, as determined by the
// RCopyError.is_retryable method.
//
// Time waited between retries increases exponentially, but will never exceed max_wait.
fn retry_exp<F: FnMut() -> RCopyResult<()>>(max_wait: Duration, mut f: F) -> RCopyResult<()> {
    let mut n = 0;
    loop {
        match f() {
            Ok(_) => break,
            Err(e) => if e.is_retryable() { break; },
        }
        let mut ms = Duration::milliseconds(1 << n);
        if ms > max_wait {
            ms = max_wait;
        }
        n += 1;
        std::io::timer::sleep(ms);
    }
    Ok(())
}

fn copy_chunk<R: Reader, W: Writer>(w: &mut W, r: &mut R, buf: &mut [u8]) -> RCopyResult<uint> {
    let mut pos = 0;
    while pos < buf.len() {
        match r.read(buf[mut]) {
            Ok(n) => {
                pos += n;
                if pos == buf.len() {
                    break
                }
            },
            Err(io::IoError{kind: std::io::EndOfFile, ..}) => break,
            Err(e) => return Err(error::FromError::from_error(e)),
        }
    }
    try!(w.write(buf[..pos]));
    Ok(pos)
}

// Reads the position from the file at fpath.
//
// If the file at fpath is of the wrong format, it is assumed that the file that's there is a
// progress file, and we would be clobbering somebody's data by continuing the operation.
fn read_position(fpath: &Path, file_size: i64) -> RCopyResult<i64> {
    use RCopyError::ProgFileInvalid;
    use ProgFileInvalidCause::*;
    let prog_file_size = try!(fs::stat(fpath)).size;
    if prog_file_size != 8u64 {
        let cause = WrongEncodedSize(prog_file_size);
        return Err(ProgFileInvalid{fpath: fpath.clone(), cause: cause});
    }
    let mut f = try!(fs::File::open(fpath));
    let position = try!(f.read_be_i64());
    if position < 0 || position > file_size {
        let cause = PosOutOfRange{
            position: position,
            file_size: file_size,
        };
        return Err(ProgFileInvalid{fpath: fpath.clone(), cause: cause});
    }
    Ok(position)
}

// TODO: Should we call read position here as a way to verify that the file we are about to
// overwrite is valid? This would add a cost to writing the position, which is done frequently, but
// might avoid overwriting somebody's file that happened to be named dst_file.progress. On one hand
// I doubt this will ever come up, and on the other hand, it would be tragic to have rcopy have the
// possibility of destroying someone's files.
fn write_position(fpath: &Path, position: i64) -> RCopyResult<()> {
    let mut f = try!(fs::File::create(fpath));
    Ok(try!(f.write_be_i64(position)))
}

// Given a destination file, returns the path for the associated progress file.
fn progress_file_path(dst_file: &Path) -> Path {
    let ext = dst_file.extension_str().unwrap_or("");
    dst_file.with_extension(format!("{}{}", ext, ".progress"))
}

fn try_copy(dst_path: &Path, src_path: &Path, tx: &Sender<ProgressInfo>) -> RCopyResult<()> {
    let mut src_file = try!(fs::File::open(src_path));
    let file_size = try!(fs::stat(src_path)).size as i64;
    let mut dst_file = try!(fs::File::open_mode(dst_path, std::io::Open, std::io::Write));
    let prog_path = progress_file_path(dst_path);

    let mut position = match read_position(&prog_path, file_size) {
        Ok(p) => p,
        Err(RCopyError::IoError(io::IoError{kind: io::FileNotFound, ..})) => 0,
        Err(e) => return Err(error::FromError::from_error(e)),
    };
    try!(src_file.seek(position, std::io::SeekSet));
    try!(dst_file.seek(position, std::io::SeekSet));

    // Now that we are at the right position in the file, wrap the reader in a buffered reader.
    let mut src_file = io::BufferedReader::with_capacity(2 * CHUNK_SIZE, src_file);

    let mut buf = Vec::new();
    buf.grow(CHUNK_SIZE, 0);
    tx.send(ProgressInfo{current: 0, total: file_size});
    while position < file_size {
        let ncopied = try!(copy_chunk(&mut dst_file, &mut src_file, buf[mut])) as i64;
        position += ncopied as i64;
        try!(write_position(&prog_path, position));
        tx.send(ProgressInfo{current: position, total: file_size});
    }
    if let Err(e) = fs::unlink(&prog_path) {
        println!("Error removing progress file: {}", e);
    };

    Ok(())
}

// TODO: Figure out how to pass back non retryable errors and remove this allow.
#[allow(unused_must_use)]
pub fn resumable_file_copy(dst_path: &Path, src_path: &Path) -> Receiver<ProgressInfo> {
    // Copy these so they can be captured by the retry
    let (src_path, dst_path) = (src_path.clone(), dst_path.clone());
    let (tx, rx) = channel();
    spawn(proc() {
        retry_exp(Duration::seconds(4), || {
            try_copy(&dst_path, &src_path, &tx)
        });
    });
    rx
}

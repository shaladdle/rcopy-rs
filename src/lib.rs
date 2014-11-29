#![feature(unboxed_closures)]
#![feature(slicing_syntax)]
#![feature(if_let)]
#![feature(globs)]

use std::error::Error;
use std::error::FromError;
use std::io::net::ip::ToSocketAddr;
use std::io::net::ip::SocketAddr;
use std::io::{Reader,Writer};
use std::io::fs;
use std::io;
use std::comm::channel;
use std::time::duration::Duration;
use std::vec::Vec;

const CHUNK_SIZE : uint = 8 << 20; // 8MiB

pub enum RCopyError{
    NotImplemented,
    ProgFileNotFound,
    IoError(io::IoError),
}

impl FromError<io::IoError> for RCopyError {
    fn from_error(io_error: io::IoError) -> RCopyError {
        RCopyError::IoError(io_error)
    }
}

impl Error for RCopyError {
    fn description(&self) -> &str {
        use RCopyError::*;
        match *self {
            NotImplemented => "not implemented",
            ProgFileNotFound => "progress file not found",
            IoError(ref e) => e.description(),
        }
    }

    fn detail(&self) -> Option<String> {
        if let RCopyError::IoError(ref e) = *self {
            return e.detail();
        }
        None
    }

    fn cause(&self) -> Option<&Error> {
        if let RCopyError::IoError(ref e) = *self {
            return e.cause();
        }
        None
    }
}

pub type RCopyResult<T> = Result<T, RCopyError>;

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

// Retries f until it returns false, backing off exponentially each time. The total
// time waited after a retry will not exceed max_wait.
fn retry_exp<F: FnMut() -> RCopyResult<()>>(max_wait: Duration, mut f: F) {
    let mut n = 0;
    while f().is_err() {
        let mut ms = Duration::milliseconds(1 << n);
        if ms > max_wait {
            ms = max_wait;
        }
        n += 1;
        std::io::timer::sleep(ms);
    }
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
            Err(e) => return Err(FromError::from_error(e)),
        }
    }
    try!(w.write(buf[..pos]));
    Ok(pos)
}

fn read_position(fpath: &Path) -> RCopyResult<i64> {
    let mut f = match fs::File::open(fpath) {
        Ok(f) => f,
        Err(io::IoError{kind: io::FileNotFound, ..}) => {
            return Err(RCopyError::ProgFileNotFound);
        },
        Err(e) => return Err(FromError::from_error(e)),
    };
    Ok(try!(f.read_be_i64()))
}

fn write_position(fpath: &Path, position: i64) -> RCopyResult<()> {
    let mut f = try!(fs::File::create(fpath));
    Ok(try!(f.write_be_i64(position)))
}

pub fn resumable_file_copy(dst_path: &Path, src_path: &Path) -> Receiver<ProgressInfo> {
    // Copy these so they can be captured by the retry
    let (src_path, dst_path) = (src_path.clone(), dst_path.clone());
    let (tx, rx) = channel();
    tx.send(ProgressInfo{current: 100, total: 100});
    spawn(proc() { retry_exp(Duration::seconds(4), || {
        let mut src_file = try!(fs::File::open(&src_path));
        let file_size = try!(fs::stat(&src_path)).size as i64;
        let mut dst_file = try!(fs::File::open_mode(&dst_path, std::io::Open, std::io::Write));
        let ext = dst_path.extension_str().unwrap_or("");
        let prog_path = dst_path.with_extension(format!("{}{}", ext, ".progress"));

        // TODO: One reason read_position might fail could be that there is already a file called
        // dst_file_path.progress. In this case, what is the right thing to do? Certainly I don't
        // want to overwrite some file that's already there unless it was a progress file created
        // by me.
        let mut position = match read_position(&prog_path) {
            Ok(p) => p,
            Err(RCopyError::ProgFileNotFound) => 0,
            Err(e) => return Err(FromError::from_error(e)),
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
    })});
    rx
}

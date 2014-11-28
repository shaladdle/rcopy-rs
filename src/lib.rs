#![feature(unboxed_closures)]
#![feature(slicing_syntax)]

use std::error::Error;
use std::error::FromError;
use std::io::net::ip::ToSocketAddr;
use std::io::net::ip::SocketAddr;
use std::io::IoError;
use std::io::{Reader,Writer};
use std::io::fs;
use std::comm::channel;
use std::time::duration::Duration;

pub struct RCopyError(String);

const CHUNK_SIZE : uint = 8 << 20; // 8MiB

impl FromError<IoError> for RCopyError {
    fn from_error(io_error: IoError) -> RCopyError {
        RCopyError(io_error.description().to_string())
    }
}

pub type RCopyResult<T> = Result<T, RCopyError>;

impl Error for RCopyError {
    fn description(&self) -> &str {
        let RCopyError(ref s) = *self;
        s.as_slice()
    }
}

#[allow(dead_code)]
pub struct RCopyDaemon {
    hostport: SocketAddr,
}

impl RCopyDaemon {
    pub fn new<A: ToSocketAddr>(hostport: A) -> RCopyResult<RCopyDaemon> {
        Ok(RCopyDaemon{hostport: try!(hostport.to_socket_addr())})
    }
    pub fn serve(&mut self) -> RCopyError {
        RCopyError("not implemented".to_string())
    }
}

pub struct ProgressInfo {
    pub current: i64,
    pub total: i64,
}

// Retries f until it returns false, backing off exponentially each time. The total
// time waited after a retry will not exceed max_wait.
fn retry_exp<F: FnMut<(), bool>>(max_wait: Duration, mut f: F) {
    let mut n = 0;
    while f() {
        let mut ms = Duration::milliseconds(1 << n);
        if ms > max_wait {
            ms = max_wait;
        }
        n += 1;
        std::io::timer::sleep(ms);
    }
}

fn copy_chunk<R: Reader, W: Writer>(w: &mut W, r: &mut R) -> RCopyResult<()> {
    let mut buf = [0, ..CHUNK_SIZE];
    match r.read_at_least(CHUNK_SIZE, buf[mut]) {
        Ok(n) => {
            if n > CHUNK_SIZE {
                return Err(RCopyError("n was bigger than CHUNK_SIZE, which should be impossible..".to_string()));
            }
        },
        Err(e) => return Err(FromError::from_error(e)),
    }

    try!(w.write(buf[]));

    return Ok(())
}

fn read_position(fpath: &Path) -> Option<i64> {
    match fs::File::open(fpath) {
        Ok(mut f) => match f.read_be_i64() {
            Ok(n) => Some(n),
            Err(_) => None,
        },
        Err(_) => None,
    }
}

fn write_position(fpath: &Path, position: i64) -> RCopyResult<()> {
    let mut f = try!(fs::File::create(fpath));
    Ok(try!(f.write_be_i64(position)))
}

pub fn resumable_file_copy(dst_path: &Path, src_path: &Path) -> Receiver<ProgressInfo> {
    let (_, rx) = channel();
    retry_exp(Duration::seconds(4), || {
        let mut src_file = match fs::File::open(src_path) {
            Ok(f) => f,
            Err(_) => return true,
        };

        let file_size = match fs::stat(src_path) {
            Ok(info) => info.size as i64,
            Err(_) => return true,
        };

        let mut dst_file = match fs::File::open_mode(src_path, std::io::Open, std::io::Write) {
            Ok(f) => f,
            Err(_) => return true,
        };

        let ext = match dst_path.extension_str() {
            Some(ext) => ext,
            None => return true,
        };
        let prog_path = dst_path.with_extension(format!("{}{}", ext, ".progress"));
        let mut position = match read_position(&prog_path) {
            Some(p) => p,
            None => 0,
        };

        match src_file.seek(position, std::io::SeekSet) {
            Err(_) => return true,
            Ok(_) => (),
        }

        match dst_file.seek(position, std::io::SeekSet) {
            Err(_) => return true,
            Ok(_) => (),
        }

        while position < file_size {
            match copy_chunk(&mut dst_file, &mut src_file) {
                Err(_) => return true,
                Ok(_) => (),
            }
            position += CHUNK_SIZE as i64;
            match write_position(&prog_path, position) {
                Err(_) => return true,
                Ok(_) => (),
            }
        }

        return false;
    });
    return rx;
}

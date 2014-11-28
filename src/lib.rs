#![feature(unboxed_closures)]
#![feature(slicing_syntax)]
#![feature(if_let)]

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

const CHUNK_SIZE : uint = 1 << 20; // 1MiB

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

fn copy_chunk<R: Reader, W: Writer>(w: &mut W, r: &mut R) -> RCopyResult<()> {
    let mut buf = [0, ..CHUNK_SIZE];
    let mut pos = 0;
    while pos < CHUNK_SIZE {
        match r.read(buf[mut]) {
            Ok(n) => {
                pos += n;
                if pos == CHUNK_SIZE {
                    break
                }
            },
            Err(IoError{kind: std::io::EndOfFile, ..}) => break,
            Err(e) => return Err(FromError::from_error(e)),
        }
    }
    try!(w.write(buf[..pos]));
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
        let mut position = read_position(&prog_path).unwrap_or(0);
        try!(src_file.seek(position, std::io::SeekSet));
        try!(dst_file.seek(position, std::io::SeekSet));

        while position < file_size {
            tx.send(ProgressInfo{current: position, total: file_size});
            try!(copy_chunk(&mut dst_file, &mut src_file));
            position += CHUNK_SIZE as i64;
            try!(write_position(&prog_path, position));
        }
        if let Err(e) = fs::unlink(&prog_path) {
            println!("Error removing progress file: {}", e);
        };

        Ok(())
    })});
    return rx;
}

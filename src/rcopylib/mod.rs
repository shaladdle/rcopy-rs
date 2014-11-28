use std::error::Error;
use std::error::FromError;
use std::io::net::ip::ToSocketAddr;
use std::io::net::ip::SocketAddr;
use std::io::IoError;

pub struct RCopyError(
    String,
);

impl FromError<IoError> for RCopyError {
    fn from_error(io_error: IoError) -> RCopyError {
        RCopyError(io_error.description().to_string())
    }
}

pub type RCopyResult<T> = Result<T, RCopyError>;

impl Error for RCopyError {
    fn description(&self) -> &str {
        match *self { RCopyError(ref s) => s.as_slice() }
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

pub struct Notifier;

impl Notifier {
    pub fn get_progress() -> ProgressInfo {
        ProgressInfo{current: 0, total: 0}
    }
}

pub fn ResumableFileCopy(dst_path: Path, src_path: Path) {
}

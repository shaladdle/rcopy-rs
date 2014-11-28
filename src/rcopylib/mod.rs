use std::error::Error;
use std::error::FromError;
use std::io::net::ip::ToSocketAddr;
use std::io::net::ip::SocketAddr;
use std::io::IoError;
use std::comm::Receiver;
use std::comm::Messages;
use std::comm::channel;

pub struct RCopyError(String);

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

fn retry<F: FnMut<(), ()>>() {
}

pub fn ResumableFileCopy(dst_path: Path, src_path: Path) -> Messages<ProgressInfo> {
    let (_, rx) = channel();
    return rx.iter()
}

use std::error::Error;

mod rcopylib;

fn main() {
    let mut daemon = match rcopylib::RCopyDaemon::new("localhost:9000") {
        Ok(daemon) => daemon,
        Err(ref err) => {
            println!("error occurred creating the daemon: {}", err.description());
            return;
        },
    };
    let err = daemon.serve();
    println!("Error occurred while serving: {}", err.description());
}

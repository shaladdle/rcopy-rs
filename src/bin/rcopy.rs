extern crate rcopy;

use std::io::fs;
use std::os;

fn main() {
    let (src_dir, _) = match os::args().as_slice() {
        [_, ref s, ref d] => (Path::new(s), Path::new(d)),
        _ => {
            println!("usage: rcopy src_dir dst_dir");
            return;
        },
    };

    let mut elems = match fs::walk_dir(&src_dir) {
        Ok(x) => x,
        Err(e) => {
            println!("walk_dir error: {}", e);
            return;
        }
    };
    for dir in elems {
        println!("{}", dir.display());
    }
}

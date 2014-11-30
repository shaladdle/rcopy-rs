rcopy-rs - Resumable Copy Utility
========

Meant as an alternative to your OS's default copy utility. rcopy allows you to resume copies that have stopped mid way.

Goals
=======
- Learn some rust
- Create a useful copy utility for moving files between my home machines

Building
========
Clone the repository and from the root do
    cargo build

Run rcopy with
    target/rcopy src_dir dst_dir

usage
========
    rcopy src_dir dst_dir

This recursively copies all files in src_dir to dst_dir. If the transfer is interrupted, rcopy will retry until it completes or an unrecoverable error has occurred. rcopy is not 

Status
========

rcopy currently works, but should not be trusted with important data.

Desired Features
========
- Implement rcopyd, a daemon that can be used to queue copies so that they happen in the background.
- Implement an rcopy server that either the daemon or the rcopy program can send files to from anywhere on the net. One use case is being able to 'fire and forget' an archival operation to free up space on a laptop.

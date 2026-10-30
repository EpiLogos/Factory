//! The one way Factory's tests write a file they will exec.
//!
//! A test that writes an executable in-process and then execs it races every
//! parallel test that forks: the fork inherits the open write handle and keeps
//! it until it execs, and an exec of the file in that window fails with
//! ETXTBSY (os error 26, `ExecutableFileBusy`). Linux CI failed this way three
//! times; measured on Linux at 49-66 of 2400 execs under six spawning threads,
//! 0 written through a child (Factory#292). Writing to a staged name and
//! renaming it into place in-process does not help: the inode, and its open
//! write handle, stay the same. Data files that are never exec'd stay written
//! in-process.
//!
//! The library's unit tests include this file too (`crate::test_executable`),
//! and `tests/test_executables.rs` refuses an executable mode granted anywhere
//! else under `factory/tests`.

use std::io::Write;
use std::path::Path;
use std::process::{Command, Stdio};

/// Write `contents` to `path` as an owner-only executable, so this process
/// never holds a writable descriptor to a file something will exec: a child
/// shell writes the bytes to a staged name beside `path`, makes them
/// executable and renames them into place. The rename gives a rewrite a fresh
/// inode, so a process still running the old file is untouched, and a path
/// several test processes share appears whole or not at all.
pub fn write_executable(path: impl AsRef<Path>, contents: impl AsRef<[u8]>) {
    let path = path.as_ref();
    let mut child = Command::new("/bin/sh")
        .args([
            "-c",
            "umask 077; staged=\"$1.$$\"; cat > \"$staged\" && chmod 700 \"$staged\" && mv -f \"$staged\" \"$1\" || { rm -f \"$staged\"; exit 1; }",
            "sh",
        ])
        .arg(path)
        .stdin(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(contents.as_ref())
        .unwrap();
    assert!(
        child.wait().unwrap().success(),
        "the test executable was not written at {}",
        path.display()
    );
}

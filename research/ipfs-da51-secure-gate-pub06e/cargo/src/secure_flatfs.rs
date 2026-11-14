#![cfg(target_os = "linux")]

use ipld_core::cid::Cid;
use sha2::{Digest, Sha256};

use std::ffi::{CString, OsStr};
use std::fs::File;
use std::io::Read;
use std::os::fd::{AsRawFd, FromRawFd};
use std::os::unix::ffi::OsStrExt;
use std::path::{Component, Path};

const O_RDONLY: i32 = 0;
const O_DIRECTORY: i32 = 0o200000;
const O_NOFOLLOW: i32 = 0o400000;
const O_CLOEXEC: i32 = 0o2000000;

pub const MAX_BYTES: u64 = 64 * 1024;

unsafe extern "C" {
    fn openat(
        dirfd: i32,
        pathname: *const std::ffi::c_char,
        flags: i32,
        ...
    ) -> i32;
}

fn open_relative(
    parent: &File,
    component: &OsStr,
    directory: bool,
) -> Option<File> {
    let name = CString::new(component.as_bytes()).ok()?;

    if name.as_bytes().is_empty()
        || name.as_bytes().contains(&b'/')
        || name.as_bytes() == b"."
        || name.as_bytes() == b".."
    {
        return None;
    }

    let mut flags = O_RDONLY | O_CLOEXEC | O_NOFOLLOW;

    if directory {
        flags |= O_DIRECTORY;
    }

    let fd = unsafe {
        openat(parent.as_raw_fd(), name.as_ptr(), flags)
    };

    if fd < 0 {
        return None;
    }

    let file = unsafe { File::from_raw_fd(fd) };

    let metadata = file.metadata().ok()?;

    if directory {
        if !metadata.is_dir() {
            return None;
        }
    } else if !metadata.is_file() {
        return None;
    }

    Some(file)
}

fn open_absolute_dir_no_symlinks(path: &Path) -> Option<File> {
    if !path.is_absolute() {
        return None;
    }

    let mut current = File::open("/").ok()?;
    let mut root_seen = false;

    for component in path.components() {
        match component {
            Component::RootDir if !root_seen => {
                root_seen = true;
            }
            Component::Normal(name) if root_seen => {
                current = open_relative(&current, name, true)?;
            }
            _ => return None,
        }
    }

    if !root_seen {
        return None;
    }

    Some(current)
}

/// Verifies ONE block only.
/// CIDv0 DAG-PB or CIDv1 raw/DAG-PB, SHA2-256.
/// Does not parse DAG-PB or reconstruct UnixFS.
/// Linux-only experimental implementation.
pub fn read_secure_block(
    repo: &Path,
    cid_text: &str,
) -> Option<Vec<u8>> {
    let cid: Cid = cid_text.parse().ok()?;

    if cid.codec() != 0x55 && cid.codec() != 0x70 {
        return None;
    }

    let mh = cid.hash();

    if mh.code() != 0x12 || mh.digest().len() != 32 {
        return None;
    }

    let key = data_encoding::BASE32_NOPAD
        .encode(&mh.to_bytes());

    if key.len() < 3 {
        return None;
    }

    let shard = &key[key.len() - 3..key.len() - 1];

    // Each directory descriptor is opened relative
    // to the already opened parent descriptor.
    let repo_fd = open_absolute_dir_no_symlinks(repo)?;
    let blocks = open_relative(
        &repo_fd,
        OsStr::new("blocks"),
        true,
    )?;
    let shard_fd = open_relative(
        &blocks,
        OsStr::new(shard),
        true,
    )?;

    let filename = format!("{key}.data");

    let file = open_relative(
        &shard_fd,
        OsStr::new(&filename),
        false,
    )?;

    let metadata = file.metadata().ok()?;

    if metadata.len() > MAX_BYTES {
        return None;
    }

    // An already-open descriptor is read, not a path
    // that can be redirected by a symlink swap.
    let mut limited = file.take(MAX_BYTES + 1);
    let mut bytes = Vec::new();

    limited.read_to_end(&mut bytes).ok()?;

    if bytes.len() as u64 > MAX_BYTES {
        return None;
    }

    if Sha256::digest(&bytes).as_slice() != mh.digest() {
        return None;
    }

    Some(bytes)
}

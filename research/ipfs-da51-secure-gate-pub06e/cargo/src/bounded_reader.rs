use ipld_core::cid::Cid;
use sha2::{Digest, Sha256};
use std::fs::{self, File};
use std::io::Read;
use std::path::Path;

pub const MAX_BLOCK_BYTES: u64 = 64 * 1024;

/// Experimental verification of ONE flatfs block.
///
/// Accepted:
/// - CIDv0, DAG-PB codec
/// - CIDv1, raw or DAG-PB codec
/// - SHA2-256 multihash with 32-byte digest
///
/// This does not decode DAG-PB or reconstruct UnixFS files.
pub fn read_verified_block(
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

    let path = repo
        .join("blocks")
        .join(shard)
        .join(format!("{key}.data"));

    // Reject symlinked final paths.
    let metadata = fs::symlink_metadata(&path).ok()?;

    if !metadata.file_type().is_file() ||
       metadata.len() > MAX_BLOCK_BYTES {
        return None;
    }

    let file = File::open(&path).ok()?;

    // Bounded allocation even if the file changes
    // between the metadata check and the read.
    let mut limited = file.take(MAX_BLOCK_BYTES + 1);
    let mut bytes = Vec::new();

    limited.read_to_end(&mut bytes).ok()?;

    if bytes.len() as u64 > MAX_BLOCK_BYTES {
        return None;
    }

    if Sha256::digest(&bytes).as_slice() != mh.digest() {
        return None;
    }

    Some(bytes)
}

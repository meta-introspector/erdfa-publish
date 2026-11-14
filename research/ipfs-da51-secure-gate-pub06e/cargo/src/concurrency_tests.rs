#[cfg(all(test, target_os = "linux"))]
mod tests {
    use crate::secure_flatfs::read_secure_block;
    use ipld_core::cid::{Cid, multihash::Multihash};
    use sha2::{Digest, Sha256};

    use std::fs;
    use std::os::unix::fs::symlink;
    use std::path::PathBuf;
    use std::sync::{Arc, Barrier};
    use std::thread;

    const VALID: &[u8] =
        b"PUB06E15L verified immutable original block";

    const CORRUPT: &[u8] =
        b"PUB06E15L malicious replacement block bytes";

    fn base() -> PathBuf {
        let path = PathBuf::from(
            std::env::var("ZOS_CONCURRENCY_ROOT").unwrap()
        );

        let text = path.to_str().unwrap();

        assert!(text.contains("ZOS_PUB06E15I."));
        assert!(text.ends_with("/concurrency"));

        path
    }

    fn cid_for(bytes: &[u8]) -> Cid {
        let digest = Sha256::digest(bytes);

        let mh = Multihash::<64>::wrap(
            0x12,
            &digest,
        ).unwrap();

        Cid::new_v1(0x55, mh)
    }

    fn location(repo: &PathBuf, cid: &Cid)
        -> (PathBuf, String)
    {
        let key = data_encoding::BASE32_NOPAD
            .encode(&cid.hash().to_bytes());

        let shard = &key[key.len() - 3..key.len() - 1];

        (
            repo.join("blocks").join(shard),
            key,
        )
    }

    #[test]
    fn concurrent_shard_symlink_swaps_never_return_corrupt_bytes() {
        let repo = base().join("swap-shard");
        let outside = base().join("outside-shard");
        let cid = cid_for(VALID);

        let (shard, key) = location(&repo, &cid);
        let parked = base().join("parked-shard");

        fs::create_dir_all(&shard).unwrap();
        fs::create_dir_all(&outside).unwrap();

        fs::write(
            shard.join(format!("{key}.data")),
            VALID,
        ).unwrap();

        fs::write(
            outside.join(format!("{key}.data")),
            CORRUPT,
        ).unwrap();

        let barrier = Arc::new(Barrier::new(2));

        let writer_barrier = barrier.clone();

        let writer = thread::spawn(move || {
            writer_barrier.wait();

            for _ in 0..300 {
                fs::rename(&shard, &parked).unwrap();

                symlink(&outside, &shard).unwrap();

                thread::yield_now();

                fs::remove_file(&shard).unwrap();

                fs::rename(&parked, &shard).unwrap();

                thread::yield_now();
            }

            300usize
        });

        let reader_barrier = barrier.clone();
        let cid_text = cid.to_string();

        let reader = thread::spawn(move || {
            reader_barrier.wait();

            let mut accepted = 0usize;
            let mut rejected = 0usize;

            for _ in 0..3000 {
                match read_secure_block(&repo, &cid_text) {
                    Some(bytes) => {
                        assert_eq!(bytes, VALID);
                        accepted += 1;
                    }
                    None => rejected += 1,
                }

                thread::yield_now();
            }

            (accepted, rejected)
        });

        let mutations = writer.join().unwrap();
        let (accepted, rejected) = reader.join().unwrap();

        assert_eq!(mutations, 300);
        assert_eq!(accepted + rejected, 3000);

        println!(
            "SHARD_SWAP accepted={} rejected={} mutations={}",
            accepted, rejected, mutations
        );
    }

    #[test]
    fn concurrent_block_replacements_never_accept_wrong_digest() {
        let repo = base().join("swap-file");
        let cid = cid_for(VALID);

        let (shard, key) = location(&repo, &cid);

        fs::create_dir_all(&shard).unwrap();

        let path = shard.join(format!("{key}.data"));

        fs::write(&path, VALID).unwrap();

        let barrier = Arc::new(Barrier::new(2));

        let writer_barrier = barrier.clone();

        let writer = thread::spawn(move || {
            writer_barrier.wait();

            for index in 0..300 {
                let bytes = if index % 2 == 0 {
                    CORRUPT
                } else {
                    VALID
                };

                let staging = shard.join("staging.tmp");

                fs::write(&staging, bytes).unwrap();

                // Atomic replacement of the directory entry.
                // A reader that already opened the old inode
                // keeps its original descriptor.
                fs::rename(&staging, &path).unwrap();

                thread::yield_now();
            }

            300usize
        });

        let reader_barrier = barrier.clone();
        let cid_text = cid.to_string();

        let reader = thread::spawn(move || {
            reader_barrier.wait();

            let mut accepted = 0usize;
            let mut rejected = 0usize;

            for _ in 0..3000 {
                match read_secure_block(&repo, &cid_text) {
                    Some(bytes) => {
                        assert_eq!(bytes, VALID);
                        accepted += 1;
                    }
                    None => rejected += 1,
                }

                thread::yield_now();
            }

            (accepted, rejected)
        });

        let mutations = writer.join().unwrap();
        let (accepted, rejected) = reader.join().unwrap();

        assert_eq!(mutations, 300);
        assert_eq!(accepted + rejected, 3000);

        println!(
            "BLOCK_SWAP accepted={} rejected={} mutations={}",
            accepted, rejected, mutations
        );
    }
}

#[cfg(all(test, unix))]
mod tests {
    use crate::bounded_reader::{
        read_verified_block,
        MAX_BLOCK_BYTES,
    };

    use ipld_core::cid::{Cid, multihash::Multihash};
    use sha2::{Digest, Sha256};
    use std::fs;
    use std::os::unix::fs::symlink;
    use std::path::PathBuf;

    fn lab() -> PathBuf {
        let path = PathBuf::from(
            std::env::var("ZOS_PATH_AUDIT_ROOT").unwrap()
        );

        let name = path.to_str().unwrap();

        assert!(name.contains("ZOS_PUB06E15I."));
        assert!(name.ends_with("/path-audit"));

        path
    }

    fn cid(bytes: &[u8]) -> Cid {
        let digest = Sha256::digest(bytes);
        let mh = Multihash::<64>::wrap(0x12, &digest).unwrap();
        Cid::new_v1(0x55, mh)
    }

    fn location(
        root: &PathBuf,
        cid: &Cid,
    ) -> (PathBuf, String) {
        let key = data_encoding::BASE32_NOPAD
            .encode(&cid.hash().to_bytes());

        let shard = &key[key.len() - 3..key.len() - 1];

        (root.join("blocks").join(shard), key)
    }

    #[test]
    fn parent_directory_symlink_is_followed_by_old_reader() {
        let bytes = b"PUB15I authentic bytes outside trusted tree";
        let id = cid(bytes);

        let repo = lab().join("parent-case-repo");
        let external = lab().join("parent-case-external");

        let (shard_dir, key) = location(&repo, &id);

        fs::create_dir_all(&external).unwrap();
        fs::create_dir_all(repo.join("blocks")).unwrap();

        fs::write(
            external.join(format!("{key}.data")),
            bytes,
        ).unwrap();

        // The shard directory is a symlink to a different
        // test-owned directory, outside the selected repo.
        symlink(&external, &shard_dir).unwrap();

        assert!(fs::symlink_metadata(&shard_dir)
            .unwrap().file_type().is_symlink());

        // This asserts the observed pre-remediation behavior:
        // the block digest is valid, but its path escapes
        // the trusted flatfs directory.
        assert_eq!(
            read_verified_block(&repo, &id.to_string()),
            Some(bytes.to_vec())
        );
    }

    #[test]
    fn final_file_symlink_is_rejected() {
        let bytes = b"PUB15I final symlink test";
        let id = cid(bytes);

        let repo = lab().join("final-case-repo");
        let external = lab().join("final-case-target");

        let (shard_dir, key) = location(&repo, &id);

        fs::create_dir_all(&shard_dir).unwrap();
        fs::create_dir_all(&external).unwrap();

        let target = external.join("block.data");
        fs::write(&target, bytes).unwrap();

        symlink(
            &target,
            shard_dir.join(format!("{key}.data")),
        ).unwrap();

        assert_eq!(
            read_verified_block(&repo, &id.to_string()),
            None
        );
    }

    #[test]
    fn exact_size_limit_passes_and_plus_one_fails() {
        let repo = lab().join("size-case-repo");

        for (size, should_pass) in [
            (MAX_BLOCK_BYTES as usize, true),
            (MAX_BLOCK_BYTES as usize + 1, false),
        ] {
            let bytes = vec![0x37; size];
            let id = cid(&bytes);
            let (shard_dir, key) = location(&repo, &id);

            fs::create_dir_all(&shard_dir).unwrap();

            fs::write(
                shard_dir.join(format!("{key}.data")),
                &bytes,
            ).unwrap();

            let result =
                read_verified_block(&repo, &id.to_string());

            assert_eq!(
                result.is_some(),
                should_pass,
                "incorrect result for size {size}"
            );
        }
    }
}

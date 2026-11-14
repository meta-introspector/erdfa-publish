#[cfg(all(test, target_os = "linux"))]
mod tests {
    use crate::secure_flatfs::{
        read_secure_block, MAX_BYTES,
    };

    use ipld_core::cid::{Cid, multihash::Multihash};
    use sha2::{Digest, Sha256};

    use std::fs;
    use std::os::unix::fs::symlink;
    use std::path::PathBuf;

    fn base() -> PathBuf {
        let path = PathBuf::from(
            std::env::var("ZOS_PATH_AUDIT_ROOT").unwrap()
        );

        let value = path.to_str().unwrap();

        assert!(value.contains("ZOS_PUB06E15I."));
        assert!(value.ends_with("/path-audit"));

        path
    }

    fn make_cid(bytes: &[u8], codec: u64) -> Cid {
        let digest = Sha256::digest(bytes);

        let mh = Multihash::<64>::wrap(
            0x12,
            &digest,
        ).unwrap();

        if codec == 0x70 {
            Cid::new_v0(mh).unwrap()
        } else {
            Cid::new_v1(codec, mh)
        }
    }

    fn location(
        repo: &PathBuf,
        cid: &Cid,
    ) -> (PathBuf, String) {
        let key = data_encoding::BASE32_NOPAD
            .encode(&cid.hash().to_bytes());

        let shard = &key[key.len() - 3..key.len() - 1];

        (
            repo.join("blocks").join(shard),
            key
        )
    }

    fn store(
        repo: &PathBuf,
        cid: &Cid,
        bytes: &[u8],
    ) -> PathBuf {
        let (directory, key) = location(repo, cid);

        fs::create_dir_all(&directory).unwrap();

        let path = directory.join(format!("{key}.data"));
        fs::write(&path, bytes).unwrap();

        path
    }

    #[test]
    fn secure_raw_valid_accepted() {
        let repo = base().join("j-raw");
        let bytes = b"PUB15J verified raw content";
        let cid = make_cid(bytes, 0x55);

        store(&repo, &cid, bytes);

        assert_eq!(
            read_secure_block(&repo, &cid.to_string()),
            Some(bytes.to_vec())
        );
    }

    #[test]
    fn secure_cidv0_valid_block_accepted() {
        let repo = base().join("j-cidv0");
        let bytes = b"PUB15J DAG-PB-labelled bytes";
        let cid = make_cid(bytes, 0x70);

        store(&repo, &cid, bytes);

        assert_eq!(
            read_secure_block(&repo, &cid.to_string()),
            Some(bytes.to_vec())
        );
    }

    #[test]
    fn secure_corrupted_block_rejected() {
        let repo = base().join("j-corruption");
        let bytes = b"PUB15J expected original";
        let cid = make_cid(bytes, 0x55);

        store(&repo, &cid, b"PUB15J modified content");

        assert_eq!(
            read_secure_block(&repo, &cid.to_string()),
            None
        );
    }

    #[test]
    fn secure_parent_symlink_rejected() {
        let repo = base().join("j-parent-repo");
        let external = base().join("j-parent-external");

        let bytes = b"PUB15J external valid block";
        let cid = make_cid(bytes, 0x55);

        let (shard, key) = location(&repo, &cid);

        fs::create_dir_all(repo.join("blocks")).unwrap();
        fs::create_dir_all(&external).unwrap();

        fs::write(
            external.join(format!("{key}.data")),
            bytes,
        ).unwrap();

        symlink(&external, &shard).unwrap();

        assert_eq!(
            read_secure_block(&repo, &cid.to_string()),
            None
        );
    }

    #[test]
    fn secure_final_symlink_rejected() {
        let repo = base().join("j-final-repo");
        let bytes = b"PUB15J final symlink block";
        let cid = make_cid(bytes, 0x55);

        let file = store(&repo, &cid, bytes);
        let external = base().join("j-final-target");

        fs::write(&external, bytes).unwrap();
        fs::remove_file(&file).unwrap();
        symlink(&external, &file).unwrap();

        assert_eq!(
            read_secure_block(&repo, &cid.to_string()),
            None
        );
    }

    #[test]
    fn secure_repository_symlink_rejected() {
        let actual = base().join("j-real-repo");
        let alias = base().join("j-repo-alias");

        let bytes = b"PUB15J repository symlink";
        let cid = make_cid(bytes, 0x55);

        store(&actual, &cid, bytes);
        symlink(&actual, &alias).unwrap();

        assert_eq!(
            read_secure_block(&alias, &cid.to_string()),
            None
        );
    }

    #[test]
    fn secure_exact_limit_and_plus_one() {
        let repo = base().join("j-size");

        for (size, expected) in [
            (MAX_BYTES as usize, true),
            (MAX_BYTES as usize + 1, false),
        ] {
            let bytes = vec![0x35; size];
            let cid = make_cid(&bytes, 0x55);

            store(&repo, &cid, &bytes);

            assert_eq!(
                read_secure_block(&repo, &cid.to_string())
                    .is_some(),
                expected,
            );
        }
    }
}

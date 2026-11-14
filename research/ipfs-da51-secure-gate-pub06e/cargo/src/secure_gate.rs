#![cfg(target_os = "linux")]

use crate::{
    authorize_pinned_claims,
    integration::GateError,
    secure_flatfs::read_secure_block,
    TrustedPolicy,
};

use ipld_core::cid::Cid;
use std::path::Path;

pub fn authorize_secure_ipfs_witness(
    repo: &Path,
    cid_text: &str,
    policy: &TrustedPolicy<'_>,
) -> Result<(), GateError> {
    let cid: Cid = cid_text
        .parse()
        .map_err(|_| GateError::UnsupportedCid)?;

    // A Semantic Witness is stored as one raw block.
    // DAG-PB and UnixFS are not accepted here.
    if cid.codec() != 0x55 {
        return Err(GateError::UnsupportedCid);
    }

    let bytes = read_secure_block(repo, cid_text)
        .ok_or(GateError::BlockIntegrity)?;

    authorize_pinned_claims(&bytes, policy)
        .map_err(GateError::Semantic)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Reject, FIXTURE_SHA256};

    use ciborium::Value;
    use ipld_core::cid::multihash::Multihash;
    use sha2::{Digest, Sha256};
    use std::fs;
    use std::os::unix::fs::symlink;
    use std::path::PathBuf;

    const WITNESS: &[u8] =
        include_bytes!("../fixtures/witness.cbor");

    fn root() -> PathBuf {
        let path = PathBuf::from(
            std::env::var("ZOS_SECURE_GATE_ROOT").unwrap()
        );

        let text = path.to_str().unwrap();

        assert!(text.contains("ZOS_PUB06E15I."));
        assert!(text.ends_with("/secure-gate"));

        path
    }

    fn make_cid(bytes: &[u8]) -> Cid {
        let digest = Sha256::digest(bytes);
        let mh = Multihash::<64>::wrap(
            0x12,
            &digest,
        ).unwrap();

        Cid::new_v1(0x55, mh)
    }

    fn location(
        repo: &Path,
        cid: &Cid,
    ) -> (PathBuf, String) {
        let key = data_encoding::BASE32_NOPAD
            .encode(&cid.hash().to_bytes());

        let shard = &key[key.len() - 3..key.len() - 1];

        (repo.join("blocks").join(shard), key)
    }

    fn store(repo: &Path, cid: &Cid, bytes: &[u8]) {
        let (dir, key) = location(repo, cid);
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join(format!("{key}.data")), bytes)
            .unwrap();
    }

    fn policy() -> TrustedPolicy<'static> {
        TrustedPolicy {
            expected_sha256: FIXTURE_SHA256,
        }
    }

    fn forged() -> Vec<u8> {
        let mut value: Value =
            ciborium::from_reader(WITNESS).unwrap();

        let Value::Tag(55889, inner) = &mut value else {
            panic!("wrong witness tag");
        };

        let Value::Map(fields) = inner.as_mut() else {
            panic!("wrong witness structure");
        };

        let (_, claims_value) = fields.iter_mut()
            .find(|(key, _)| {
                matches!(
                    key,
                    Value::Text(s) if s == "claims"
                )
            })
            .unwrap();

        let Value::Array(claims) = claims_value else {
            panic!("claims not an array");
        };

        let Value::Map(first) = &mut claims[0] else {
            panic!("claim not a map");
        };

        let (_, theorem) = first.iter_mut()
            .find(|(key, _)| {
                matches!(
                    key,
                    Value::Text(s) if s == "theorem"
                )
            })
            .unwrap();

        *theorem = Value::Text(
            "UNAUTHORIZED_SECURE_GATE_THEOREM".into()
        );

        let mut bytes = Vec::new();
        ciborium::into_writer(&value, &mut bytes)
            .unwrap();

        assert_ne!(bytes, WITNESS);
        bytes
    }

    #[test]
    fn secure_gate_valid_witness_accepted() {
        let repo = root().join("valid");
        let cid = make_cid(WITNESS);

        store(&repo, &cid, WITNESS);

        assert_eq!(
            authorize_secure_ipfs_witness(
                &repo,
                &cid.to_string(),
                &policy(),
            ),
            Ok(())
        );
    }

    #[test]
    fn secure_gate_corrupted_block_rejected() {
        let repo = root().join("corrupted");
        let cid = make_cid(WITNESS);

        let mut altered = WITNESS.to_vec();
        altered[20] ^= 1;

        store(&repo, &cid, &altered);

        assert_eq!(
            authorize_secure_ipfs_witness(
                &repo,
                &cid.to_string(),
                &policy(),
            ),
            Err(GateError::BlockIntegrity)
        );
    }

    #[test]
    fn secure_gate_valid_cid_forged_claim_rejected() {
        let repo = root().join("forged");
        let bytes = forged();
        let cid = make_cid(&bytes);

        store(&repo, &cid, &bytes);

        // First verify that the secure reader accepts
        // this content-valid but semantically altered block.
        assert_eq!(
            read_secure_block(&repo, &cid.to_string()),
            Some(bytes.clone())
        );

        let repinned_hash =
            format!("{:x}", Sha256::digest(&bytes));

        let repinned = TrustedPolicy {
            expected_sha256: &repinned_hash,
        };

        // Re-pinning content identity cannot bypass
        // the separately trusted semantic claim policy.
        assert_eq!(
            authorize_secure_ipfs_witness(
                &repo,
                &cid.to_string(),
                &repinned,
            ),
            Err(GateError::Semantic(Reject::Claims))
        );
    }

    #[test]
    fn secure_gate_parent_symlink_rejected() {
        let repo = root().join("parent-repo");
        let external = root().join("parent-external");
        let cid = make_cid(WITNESS);

        let (shard_dir, key) = location(&repo, &cid);

        fs::create_dir_all(repo.join("blocks")).unwrap();
        fs::create_dir_all(&external).unwrap();

        fs::write(
            external.join(format!("{key}.data")),
            WITNESS,
        ).unwrap();

        symlink(&external, &shard_dir).unwrap();

        assert_eq!(
            authorize_secure_ipfs_witness(
                &repo,
                &cid.to_string(),
                &policy(),
            ),
            Err(GateError::BlockIntegrity)
        );
    }
}

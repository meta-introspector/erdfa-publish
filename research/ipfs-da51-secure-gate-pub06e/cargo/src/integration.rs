use crate::{
    authorize_pinned_claims,
    bounded_reader::read_verified_block,
    Reject,
    TrustedPolicy,
};

use ipld_core::cid::Cid;
use std::path::Path;

#[derive(Debug, PartialEq, Eq)]
pub enum GateError {
    UnsupportedCid,
    BlockIntegrity,
    Semantic(Reject),
}

pub fn authorize_ipfs_witness(
    repo: &Path,
    cid_text: &str,
    policy: &TrustedPolicy<'_>,
) -> Result<(), GateError> {
    let cid: Cid = cid_text
        .parse()
        .map_err(|_| GateError::UnsupportedCid)?;

    // This path accepts only raw CIDv1 blocks.
    // DAG-PB and UnixFS require a separate decoder.
    if cid.codec() != 0x55 {
        return Err(GateError::UnsupportedCid);
    }

    let bytes = read_verified_block(repo, cid_text)
        .ok_or(GateError::BlockIntegrity)?;

    authorize_pinned_claims(&bytes, policy)
        .map_err(GateError::Semantic)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::FIXTURE_SHA256;
    use crate::bounded_reader::MAX_BLOCK_BYTES;
    use ciborium::Value;
    use ipld_core::cid::multihash::Multihash;
    use sha2::{Digest, Sha256};
    use std::fs;
    use std::path::PathBuf;

    const WITNESS: &[u8] =
        include_bytes!("../fixtures/witness.cbor");

    fn repo() -> PathBuf {
        let root = PathBuf::from(
            std::env::var("ZOS_TEST_FLATFS").unwrap()
        );

        let name = root.to_str().unwrap();
        assert!(name.contains("ZOS_PUB06E15I."));
        assert!(name.ends_with("/synthetic-flatfs"));

        root
    }

    fn cid_for(bytes: &[u8], codec: u64) -> Cid {
        let digest = Sha256::digest(bytes);
        let multihash =
            Multihash::<64>::wrap(0x12, &digest).unwrap();

        Cid::new_v1(codec, multihash)
    }

    fn store(cid: &Cid, bytes: &[u8]) {
        let key = data_encoding::BASE32_NOPAD
            .encode(&cid.hash().to_bytes());

        let shard = &key[key.len() - 3..key.len() - 1];
        let directory = repo().join("blocks").join(shard);

        fs::create_dir_all(&directory).unwrap();
        fs::write(directory.join(format!("{key}.data")), bytes)
            .unwrap();
    }

    fn pinned() -> TrustedPolicy<'static> {
        TrustedPolicy {
            expected_sha256: FIXTURE_SHA256,
        }
    }

    fn forged_witness() -> Vec<u8> {
        let mut value: Value =
            ciborium::from_reader(WITNESS).unwrap();

        let Value::Tag(55889, inner) = &mut value else {
            panic!("fixture tag");
        };

        let Value::Map(root) = inner.as_mut() else {
            panic!("fixture map");
        };

        let (_, claim_value) = root.iter_mut()
            .find(|(k, _)| {
                matches!(k, Value::Text(s) if s == "claims")
            })
            .unwrap();

        let Value::Array(claims) = claim_value else {
            panic!("fixture claims");
        };

        let Value::Map(fields) = &mut claims[0] else {
            panic!("fixture claim structure");
        };

        let (_, theorem) = fields.iter_mut()
            .find(|(k, _)| {
                matches!(k, Value::Text(s) if s == "theorem")
            })
            .unwrap();

        *theorem = Value::Text(
            "UNAUTHORIZED_PUBLISHED_THEOREM".into()
        );

        let mut output = Vec::new();
        ciborium::into_writer(&value, &mut output).unwrap();

        assert_ne!(output, WITNESS);
        output
    }

    #[test]
    fn valid_ipfs_and_valid_semantic_witness_pass() {
        let cid = cid_for(WITNESS, 0x55);
        store(&cid, WITNESS);

        assert_eq!(
            authorize_ipfs_witness(
                &repo(), &cid.to_string(), &pinned()
            ),
            Ok(())
        );
    }

    #[test]
    fn corrupted_ipfs_block_fails_before_semantics() {
        let cid = cid_for(WITNESS, 0x55);
        let mut corrupted = WITNESS.to_vec();
        corrupted[20] ^= 1;
        store(&cid, &corrupted);

        assert_eq!(
            authorize_ipfs_witness(
                &repo(), &cid.to_string(), &pinned()
            ),
            Err(GateError::BlockIntegrity)
        );
    }

    #[test]
    fn valid_ipfs_but_forged_claim_is_rejected() {
        let forged = forged_witness();
        let cid = cid_for(&forged, 0x55);
        store(&cid, &forged);

        // Re-pin the object identity deliberately.
        let digest = format!("{:x}", Sha256::digest(&forged));
        let policy = TrustedPolicy {
            expected_sha256: &digest,
        };

        // The object has valid IPFS content integrity,
        // but its theorem differs from the audited fixture.
        assert_eq!(
            read_verified_block(&repo(), &cid.to_string()),
            Some(forged.clone())
        );

        assert_eq!(
            authorize_ipfs_witness(
                &repo(), &cid.to_string(), &policy
            ),
            Err(GateError::Semantic(Reject::Claims))
        );
    }

    #[test]
    fn wrong_trusted_identity_is_rejected() {
        let forged = forged_witness();
        let cid = cid_for(&forged, 0x55);
        store(&cid, &forged);

        assert_eq!(
            authorize_ipfs_witness(
                &repo(), &cid.to_string(), &pinned()
            ),
            Err(GateError::Semantic(Reject::Identity))
        );
    }

    #[test]
    fn oversized_ipfs_block_fails_closed() {
        let bytes = vec![0x5a; MAX_BLOCK_BYTES as usize + 1];
        let cid = cid_for(&bytes, 0x55);
        store(&cid, &bytes);

        assert_eq!(
            authorize_ipfs_witness(
                &repo(), &cid.to_string(), &pinned()
            ),
            Err(GateError::BlockIntegrity)
        );
    }

    #[test]
    fn dagpb_codec_is_not_treated_as_raw_witness() {
        let cid = cid_for(WITNESS, 0x70);
        store(&cid, WITNESS);

        assert_eq!(
            authorize_ipfs_witness(
                &repo(), &cid.to_string(), &pinned()
            ),
            Err(GateError::UnsupportedCid)
        );
    }
}

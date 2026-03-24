//! Privacy layer for erdfa shards.
//!
//! Field-level Merkle commitments: each key-value pair in a shard gets its own
//! leaf. The Merkle root is published; individual fields can be selectively
//! revealed with a proof, or redacted (replaced by their hash).
//!
//! This is the interface where lattirust LaBRADOR proofs plug in:
//!   "I know field values such that MerkleRoot = R and predicate P holds"
//! without revealing the values.

use sha2::{Sha256, Digest};
use serde::{Serialize, Deserialize};

// ── Merkle tree ─────────────────────────────────────────────────

fn hash_leaf(data: &[u8]) -> [u8; 32] {
    let mut h = Sha256::new();
    h.update(b"leaf:");
    h.update(data);
    h.finalize().into()
}

fn hash_node(left: &[u8; 32], right: &[u8; 32]) -> [u8; 32] {
    let mut h = Sha256::new();
    h.update(b"node:");
    h.update(left);
    h.update(right);
    h.finalize().into()
}

/// Binary Merkle tree over byte slices.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MerkleTree {
    pub leaves: Vec<[u8; 32]>,
    pub root: [u8; 32],
}

/// Sibling hash + direction (false=left, true=right) for one level.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ProofStep {
    pub hash: [u8; 32],
    pub is_right: bool,
}

/// Merkle inclusion proof for one leaf.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MerkleProof {
    pub leaf_index: usize,
    pub leaf_hash: [u8; 32],
    pub path: Vec<ProofStep>,
    pub root: [u8; 32],
}

impl MerkleTree {
    pub fn from_data(items: &[&[u8]]) -> Self {
        let leaves: Vec<[u8; 32]> = items.iter().map(|d| hash_leaf(d)).collect();
        let root = Self::compute_root(&leaves);
        Self { leaves, root }
    }

    fn compute_root(leaves: &[[u8; 32]]) -> [u8; 32] {
        if leaves.is_empty() { return [0u8; 32]; }
        let mut layer = leaves.to_vec();
        while layer.len() > 1 {
            let mut next = Vec::with_capacity((layer.len() + 1) / 2);
            for i in (0..layer.len()).step_by(2) {
                let right = if i + 1 < layer.len() { &layer[i + 1] } else { &layer[i] };
                next.push(hash_node(&layer[i], right));
            }
            layer = next;
        }
        layer[0]
    }

    /// Generate inclusion proof for leaf at `index`.
    pub fn prove(&self, index: usize) -> Option<MerkleProof> {
        if index >= self.leaves.len() { return None; }
        let mut path = Vec::new();
        let mut layer = self.leaves.clone();
        let mut idx = index;
        while layer.len() > 1 {
            let sibling = if idx % 2 == 0 {
                let s = if idx + 1 < layer.len() { idx + 1 } else { idx };
                ProofStep { hash: layer[s], is_right: true }
            } else {
                ProofStep { hash: layer[idx - 1], is_right: false }
            };
            path.push(sibling);
            let mut next = Vec::with_capacity((layer.len() + 1) / 2);
            for i in (0..layer.len()).step_by(2) {
                let right = if i + 1 < layer.len() { &layer[i + 1] } else { &layer[i] };
                next.push(hash_node(&layer[i], right));
            }
            layer = next;
            idx /= 2;
        }
        Some(MerkleProof { leaf_index: index, leaf_hash: self.leaves[index], path, root: self.root })
    }

    /// Verify an inclusion proof.
    pub fn verify(proof: &MerkleProof) -> bool {
        let mut current = proof.leaf_hash;
        for step in &proof.path {
            current = if step.is_right {
                hash_node(&current, &step.hash)
            } else {
                hash_node(&step.hash, &current)
            };
        }
        current == proof.root
    }
}

// ── Privacy shard ───────────────────────────────────────────────

/// A field that is either revealed or redacted (committed).
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum PrivacyField {
    Revealed { key: String, value: String },
    Redacted { key: String, commitment: String },
}

/// A shard with field-level privacy: Merkle root over all fields,
/// each field independently revealable or redactable.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PrivacyShard {
    pub id: String,
    pub merkle_root: String,
    pub fields: Vec<PrivacyField>,
    pub tags: Vec<String>,
}

impl PrivacyShard {
    /// Create from key-value pairs. All fields start revealed.
    pub fn from_pairs(id: &str, pairs: &[(String, String)], tags: Vec<String>) -> Self {
        let tree = Self::build_tree(pairs);
        let fields = pairs.iter().map(|(k, v)| PrivacyField::Revealed {
            key: k.clone(), value: v.clone(),
        }).collect();
        Self {
            id: id.into(),
            merkle_root: hex::encode(tree.root),
            fields,
            tags,
        }
    }

    /// Redact specified field keys — replace values with commitments.
    pub fn redact(&mut self, keys: &[&str]) {
        for field in &mut self.fields {
            if let PrivacyField::Revealed { key, value } = field {
                if keys.contains(&key.as_str()) {
                    let commit = hex::encode(hash_leaf(
                        format!("{}={}", key, value).as_bytes()
                    ));
                    *field = PrivacyField::Redacted { key: key.clone(), commitment: commit };
                }
            }
        }
    }

    /// Generate Merkle proof for a specific field index.
    pub fn prove_field(&self, pairs: &[(String, String)], index: usize) -> Option<MerkleProof> {
        let tree = Self::build_tree(pairs);
        tree.prove(index)
    }

    fn build_tree(pairs: &[(String, String)]) -> MerkleTree {
        let leaves: Vec<Vec<u8>> = pairs.iter()
            .map(|(k, v)| format!("{}={}", k, v).into_bytes())
            .collect();
        let refs: Vec<&[u8]> = leaves.iter().map(|v| v.as_slice()).collect();
        MerkleTree::from_data(&refs)
    }

    /// Encode as DA51-tagged CBOR.
    pub fn to_cbor(&self) -> Vec<u8> {
        let mut buf = Vec::new();
        let val = ciborium::Value::serialized(self).unwrap();
        let tagged = ciborium::Value::Tag(55889, Box::new(val));
        ciborium::into_writer(&tagged, &mut buf).unwrap();
        buf
    }

    /// Canonical bytes for signing (merkle_root || id || field count).
    pub fn signable_bytes(&self) -> Vec<u8> {
        format!("{}:{}:{}", self.merkle_root, self.id, self.fields.len()).into_bytes()
    }
}

// ── Post-quantum signatures (ML-DSA-44) ─────────────────────────

/// A PrivacyShard with a post-quantum signature.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SignedPrivacyShard {
    pub shard: PrivacyShard,
    pub signature: Vec<u8>,
    pub public_key: Vec<u8>,
}

impl SignedPrivacyShard {
    /// Sign a PrivacyShard with ML-DSA-44.
    pub fn sign(shard: PrivacyShard) -> Result<Self, String> {
        use lattice_safe_suite::dilithium::{MlDsaKeyPair, ML_DSA_44};
        let kp = MlDsaKeyPair::generate(ML_DSA_44).map_err(|e| e.to_string())?;
        let msg = shard.signable_bytes();
        let sig = kp.sign(&msg, b"erdfa-privacy-v1").map_err(|e| e.to_string())?;
        Ok(Self {
            shard,
            signature: sig.as_bytes().to_vec(),
            public_key: kp.public_key().to_vec(),
        })
    }

    /// Verify the PQ signature.
    pub fn verify(&self) -> bool {
        use lattice_safe_suite::dilithium::{MlDsaKeyPair, MlDsaSignature, ML_DSA_44};
        let msg = self.shard.signable_bytes();
        let sig = MlDsaSignature::from_slice(&self.signature);
        MlDsaKeyPair::verify(&self.public_key, &sig, &msg, b"erdfa-privacy-v1", ML_DSA_44)
    }

    /// Encode as DA51-tagged CBOR.
    pub fn to_cbor(&self) -> Vec<u8> {
        let mut buf = Vec::new();
        let val = ciborium::Value::serialized(self).unwrap();
        let tagged = ciborium::Value::Tag(55889, Box::new(val));
        ciborium::into_writer(&tagged, &mut buf).unwrap();
        buf
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn merkle_roundtrip() {
        let items: Vec<&[u8]> = vec![b"a", b"b", b"c", b"d"];
        let tree = MerkleTree::from_data(&items);
        for i in 0..4 {
            let proof = tree.prove(i).unwrap();
            assert!(MerkleTree::verify(&proof));
        }
    }

    #[test]
    fn privacy_shard_redact() {
        let pairs = vec![
            ("cycles".into(), "1000000".into()),
            ("ip".into(), "0xdeadbeef".into()),
            ("event".into(), "cache-misses".into()),
        ];
        let mut ps = PrivacyShard::from_pairs("test", &pairs, vec!["perf".into()]);
        assert!(matches!(&ps.fields[1], PrivacyField::Revealed { .. }));
        ps.redact(&["ip"]);
        assert!(matches!(&ps.fields[1], PrivacyField::Redacted { .. }));
        // Merkle root unchanged
        assert_eq!(ps.merkle_root, hex::encode(PrivacyShard::build_tree(&pairs).root));
    }
}

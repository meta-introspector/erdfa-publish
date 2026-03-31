//! seal.rs — Public seal format: content-addressed envelope with orbifold metadata.
//!
//! This is the PUBLIC seal format. It wraps data with a SHA-256 hash,
//! orbifold coordinates, and DASL address. No steganography — just a
//! verifiable envelope. Use ZOS seal service for image-embedded seals.

use sha2::{Sha256, Digest};
use serde::{Serialize, Deserialize};

/// Public seal envelope — verifiable content-addressed wrapper.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Seal {
    pub hash: String,
    pub orbifold: (u8, u8, u8),
    pub dasl: String,
    pub size: usize,
    pub payload: Vec<u8>,
}

impl Seal {
    /// Wrap data in a seal envelope.
    pub fn wrap(data: &[u8]) -> Self {
        let h = Sha256::digest(data);
        let v = u64::from_le_bytes(h[0..8].try_into().unwrap());
        Self {
            hash: hex::encode(h),
            orbifold: ((v % 71) as u8, (v % 59) as u8, (v % 47) as u8),
            dasl: format!("0xda51{:012x}", v & 0xffffffffffff),
            size: data.len(),
            payload: data.to_vec(),
        }
    }

    /// Verify seal integrity.
    pub fn verify(&self) -> bool {
        hex::encode(Sha256::digest(&self.payload)) == self.hash
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wrap_and_verify() {
        let seal = Seal::wrap(b"test data");
        assert!(seal.verify());
        assert!(seal.dasl.starts_with("0xda51"));
        assert!(seal.orbifold.0 < 71);
    }
}

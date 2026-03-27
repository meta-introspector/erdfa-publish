//! Hecke-Maass sharding: distribute files across 71 shards using
//! Hecke eigenvalues scaled by the first 71 primes, with Maass form weights.
//!
//! Reference: swab_the_deck.sh (bash prototype), now replaced by this module.
//! See ~/DOCS/services/solfunmeme-deployments/README.md for architecture.

use sha2::{Sha256, Digest};
use serde::{Serialize, Deserialize};

/// First 71 primes — one per shard, used as Hecke operator scaling factors.
pub const PRIMES_71: [u64; 71] = [
    2, 3, 5, 7, 11, 13, 17, 19, 23, 29, 31, 37, 41, 43, 47, 53, 59, 61, 67, 71,
    73, 79, 83, 89, 97, 101, 103, 107, 109, 113, 127, 131, 137, 139, 149, 151,
    157, 163, 167, 173, 179, 181, 191, 193, 197, 199, 211, 223, 227, 229, 233,
    239, 241, 251, 257, 263, 269, 271, 277, 281, 283, 293, 307, 311, 313, 317,
    331, 337, 347, 349, 353,
];

/// Hecke eigenvalue for a single file/entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HeckeEigenvalue {
    pub shard_id: usize,
    pub prime: u64,
    pub re: f64,
    pub im: f64,
    pub norm: f64,
    pub maass_weight: f64,
}

/// Compute the Hecke eigenvalue for content with known line count and byte size.
///
/// Shard assignment: `(lines*7 + size*3 + hash_prefix) % 71`
/// Eigenvalue: hash bytes → complex number scaled by `2√p`
/// Maass weight: `exp(-2πn / (1 + |Im|))`
pub fn hecke_eigenvalue(data: &[u8], lines: usize, size: usize) -> HeckeEigenvalue {
    let hash = Sha256::digest(data);
    let h_hi = u64::from_be_bytes(hash[0..8].try_into().unwrap());
    let h_lo = u64::from_be_bytes(hash[8..16].try_into().unwrap());

    let shard_id = ((lines as u64 * 7 + size as u64 * 3 + h_hi) % 71) as usize;
    let p = PRIMES_71[shard_id];
    let scale = 2.0 * (p as f64).sqrt();

    let re = ((h_hi % 10000) as f64 / 5000.0 - 1.0) * scale;
    let im = ((h_lo % 10000) as f64 / 5000.0 - 1.0) * scale;
    let norm = (re * re + im * im).sqrt();

    let n = shard_id as f64 + 1.0;
    let maass_weight = (-2.0 * std::f64::consts::PI * n / (1.0 + im.abs())).exp();

    HeckeEigenvalue { shard_id, prime: p, re, im, norm, maass_weight }
}

/// Distribute entries across 71 shards by Hecke eigenvalue.
/// Input: slice of `(data, line_count, byte_size)`.
/// Output: 71 vectors of entry indices.
pub fn hecke_shard(entries: &[(Vec<u8>, usize, usize)]) -> Vec<Vec<usize>> {
    let mut shards = vec![Vec::new(); 71];
    for (i, (data, lines, size)) in entries.iter().enumerate() {
        let ev = hecke_eigenvalue(data, *lines, *size);
        shards[ev.shard_id].push(i);
    }
    shards
}

/// Orbifold coordinates on the Monster Group torus: `(mod 71, mod 59, mod 47)`.
/// Crown product: 47 × 59 × 71 = 196,883.
pub fn orbifold_coords(data: &[u8]) -> (u8, u8, u8) {
    let hash = Sha256::digest(data);
    let v = u64::from_le_bytes(hash[0..8].try_into().unwrap());
    ((v % 71) as u8, (v % 59) as u8, (v % 47) as u8)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn primes_count() {
        assert_eq!(PRIMES_71.len(), 71);
        assert_eq!(PRIMES_71[0], 2);
        assert_eq!(PRIMES_71[70], 353);
    }

    #[test]
    fn shard_id_bounded() {
        let ev = hecke_eigenvalue(b"test data", 10, 100);
        assert!(ev.shard_id < 71);
        assert!(PRIMES_71.contains(&ev.prime));
    }

    #[test]
    fn orbifold_bounded() {
        let (a, b, c) = orbifold_coords(b"hello");
        assert!(a < 71);
        assert!(b < 59);
        assert!(c < 47);
    }

    #[test]
    fn crown_product() {
        assert_eq!(47u64 * 59 * 71, 196_883);
    }

    #[test]
    fn hecke_shard_distributes() {
        let entries: Vec<(Vec<u8>, usize, usize)> = (0..100)
            .map(|i| (format!("file_{}", i).into_bytes(), i, i * 10))
            .collect();
        let shards = hecke_shard(&entries);
        assert_eq!(shards.len(), 71);
        let total: usize = shards.iter().map(|s| s.len()).sum();
        assert_eq!(total, 100);
    }
}

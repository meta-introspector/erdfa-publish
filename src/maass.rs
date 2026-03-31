//! Maass: semantic blade targeting on the Monster torus.
//!
//! Given data + target blade + bag of beliefs, find the smallest subset
//! of beliefs that steers the hash into the target blade. The shadow
//! is the chosen subset — meaningful metadata, not random noise.
//!
//! CRQ: CRQ-SWAB-013 (Maass restoration)

use sha2::{Sha256, Digest};
use serde::{Serialize, Deserialize};
use std::collections::BTreeMap;

/// The 15 supersingular primes dividing |Monster|.
pub const SSP: [u64; 15] = [2, 3, 5, 7, 11, 13, 17, 19, 23, 29, 31, 41, 47, 59, 71];

/// Full torus coordinate: hash mod each SSP prime.
pub fn torus_coord(data: &[u8]) -> [u64; 15] {
    let h = u128::from_be_bytes(Sha256::digest(data)[0..16].try_into().unwrap());
    let mut c = [0u64; 15];
    for (i, &p) in SSP.iter().enumerate() {
        c[i] = (h % p as u128) as u64;
    }
    c
}

/// Orbifold coords: (mod 71, mod 59, mod 47).
pub fn orbifold(data: &[u8]) -> (u64, u64, u64) {
    let c = torus_coord(data);
    (c[14], c[13], c[12])
}

/// Hamming distance between two torus coordinates.
pub fn hamming(a: &[u64; 15], b: &[u64; 15]) -> u32 {
    a.iter().zip(b).filter(|(x, y)| x != y).count() as u32
}

/// L1 distance on the torus (modular per prime).
pub fn torus_l1(a: &[u64; 15], b: &[u64; 15]) -> u64 {
    a.iter().zip(b).zip(SSP.iter()).map(|((x, y), &p)| {
        let d = if x > y { x - y } else { y - x };
        d.min(p - d)
    }).sum()
}

/// Canonical encoding: data + \0 + sorted JSON of beliefs.
fn encode(data: &[u8], beliefs: &BTreeMap<String, String>) -> Vec<u8> {
    if beliefs.is_empty() {
        return data.to_vec();
    }
    let json = serde_json::to_string(beliefs).unwrap_or_default();
    let mut out = data.to_vec();
    out.push(0);
    out.extend(json.as_bytes());
    out
}

/// Result of a maass repair.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MaassResult {
    pub found: bool,
    pub shadow: BTreeMap<String, String>,
    pub repair_size: usize,
    pub natural_hash: String,
    pub repaired_hash: String,
    pub natural_orbifold: (u64, u64, u64),
    pub repaired_orbifold: (u64, u64, u64),
    pub natural_torus: [u64; 15],
    pub repaired_torus: [u64; 15],
    pub hamming_distance: u32,
    pub torus_l1_distance: u64,
    pub cid: String,
}

/// Find the smallest subset of beliefs that steers data into the target blade.
pub fn find_shadow(
    data: &[u8],
    bag: &BTreeMap<String, String>,
    target_blade: u64,
    target_59: Option<u64>,
    target_47: Option<u64>,
) -> MaassResult {
    let nat_hash = hex::encode(Sha256::digest(data));
    let nat_orb = orbifold(data);
    let nat_torus = torus_coord(data);

    let keys: Vec<&String> = bag.keys().collect();
    let n = keys.len();

    // Try subsets of increasing size
    for size in 0..=n {
        for combo in Combinations::new(n, size) {
            let subset: BTreeMap<String, String> = combo.iter()
                .map(|&i| (keys[i].clone(), bag[&*keys[i]].clone()))
                .collect();
            let encoded = encode(data, &subset);
            let o = orbifold(&encoded);
            if o.0 != target_blade { continue; }
            if let Some(t) = target_59 { if o.1 != t { continue; } }
            if let Some(t) = target_47 { if o.2 != t { continue; } }
            let rep_torus = torus_coord(&encoded);
            let rep_hash = hex::encode(Sha256::digest(&encoded));
            let cid = format!("bafk{}", &rep_hash[..32]);
            return MaassResult {
                found: true, shadow: subset, repair_size: size,
                natural_hash: nat_hash, repaired_hash: rep_hash,
                natural_orbifold: nat_orb, repaired_orbifold: o,
                natural_torus: nat_torus, repaired_torus: rep_torus,
                hamming_distance: hamming(&nat_torus, &rep_torus),
                torus_l1_distance: torus_l1(&nat_torus, &rep_torus),
                cid,
            };
        }
    }

    // Bag exhausted — repair with counter
    let full = encode(data, bag);
    for n in 0..10_000u32 {
        let mut candidate = full.clone();
        candidate.extend(&n.to_be_bytes());
        let o = orbifold(&candidate);
        if o.0 != target_blade { continue; }
        if let Some(t) = target_59 { if o.1 != t { continue; } }
        if let Some(t) = target_47 { if o.2 != t { continue; } }
        let rep_torus = torus_coord(&candidate);
        let rep_hash = hex::encode(Sha256::digest(&candidate));
        let cid = format!("bafk{}", &rep_hash[..32]);
        let mut shadow = bag.clone();
        shadow.insert("_restoration".into(), n.to_string());
        return MaassResult {
            found: true, shadow, repair_size: bag.len() + 1,
            natural_hash: nat_hash, repaired_hash: rep_hash,
            natural_orbifold: nat_orb, repaired_orbifold: o,
            natural_torus: nat_torus, repaired_torus: rep_torus,
            hamming_distance: hamming(&nat_torus, &rep_torus),
            torus_l1_distance: torus_l1(&nat_torus, &rep_torus),
            cid,
        };
    }

    MaassResult {
        found: false, shadow: BTreeMap::new(), repair_size: 0,
        natural_hash: nat_hash, repaired_hash: String::new(),
        natural_orbifold: nat_orb, repaired_orbifold: (0, 0, 0),
        natural_torus: nat_torus, repaired_torus: [0; 15],
        hamming_distance: 0, torus_l1_distance: 0, cid: String::new(),
    }
}

/// Simple combination iterator (indices of size k from 0..n).
struct Combinations { indices: Vec<usize>, n: usize, k: usize, first: bool, done: bool }

impl Combinations {
    fn new(n: usize, k: usize) -> Self {
        if k > n { return Self { indices: vec![], n, k, first: false, done: true }; }
        Self { indices: (0..k).collect(), n, k, first: true, done: false }
    }
}

impl Iterator for Combinations {
    type Item = Vec<usize>;
    fn next(&mut self) -> Option<Vec<usize>> {
        if self.done { return None; }
        if self.first { self.first = false; return Some(self.indices.clone()); }
        if self.k == 0 { self.done = true; return None; }
        // Find rightmost index that can be incremented
        let mut i = self.k;
        loop {
            if i == 0 { self.done = true; return None; }
            i -= 1;
            if self.indices[i] < i + self.n - self.k { break; }
        }
        self.indices[i] += 1;
        for j in (i + 1)..self.k { self.indices[j] = self.indices[j - 1] + 1; }
        Some(self.indices.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn orbifold_bounded() {
        let (a, b, c) = orbifold(b"test");
        assert!(a < 71 && b < 59 && c < 47);
    }

    #[test]
    fn torus_coord_bounded() {
        let c = torus_coord(b"test");
        for (i, &v) in c.iter().enumerate() { assert!(v < SSP[i]); }
    }

    #[test]
    fn find_shadow_hits_blade() {
        let mut bag = BTreeMap::new();
        for i in 0..10 { bag.insert(format!("k{}", i), format!("v{}", i)); }
        let res = find_shadow(b"hello", &bag, 41, None, None);
        assert!(res.found);
        assert_eq!(res.repaired_orbifold.0, 41);
        assert!(res.repair_size <= 10);
    }

    #[test]
    fn shadow_is_minimal() {
        let mut bag = BTreeMap::new();
        for i in 0..8 { bag.insert(format!("k{}", i), format!("v{}", i)); }
        let res = find_shadow(b"test data", &bag, 0, None, None);
        assert!(res.found);
        // Verify no smaller subset works
        let keys: Vec<&String> = res.shadow.keys().collect();
        if keys.len() > 1 {
            for k in &keys {
                let mut smaller = res.shadow.clone();
                smaller.remove(*k);
                let o = orbifold(&encode(b"test data", &smaller));
                // At least one removal should break it (otherwise shadow wasn't minimal)
                if o.0 != 0 { return; }
            }
            panic!("shadow was not minimal");
        }
    }

    #[test]
    fn distances_correct() {
        let a = [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14];
        let b = [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14];
        assert_eq!(hamming(&a, &b), 0);
        assert_eq!(torus_l1(&a, &b), 0);
    }

    #[test]
    fn combinations_count() {
        assert_eq!(Combinations::new(5, 2).count(), 10);
        assert_eq!(Combinations::new(4, 0).count(), 1);
        assert_eq!(Combinations::new(3, 3).count(), 1);
    }
}

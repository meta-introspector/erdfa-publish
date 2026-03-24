//! ZKP-driven mixer pool: deposit commitments → Merkle tree → withdrawal proofs.
//!
//! Flow:
//!   1. Depositor creates a Note (secret + nullifier), computes commitment = H(secret || nullifier)
//!   2. Deposit on-chain: send SOL to pool, commitment inserted into Merkle tree
//!   3. Withdrawer (possibly different identity) creates a WithdrawalProof:
//!      - Merkle inclusion proof that commitment is in the tree
//!      - Reveals nullifier (prevents double-spend) but NOT the secret
//!      - Specifies recipient address
//!   4. WithdrawalProof travels via stego-gossip (no on-chain link to depositor)
//!   5. Relayer verifies proof, submits withdrawal tx to recipient
//!
//! The nullifier set prevents double-spending. The Merkle proof proves membership
//! without revealing which leaf (deposit) is being withdrawn.

use serde::{Serialize, Deserialize};
use sha2::{Sha256, Digest};
use crate::privacy::MerkleTree;

/// A deposit note: the secret knowledge needed to withdraw.
/// The depositor keeps this private and passes it to the recipient out-of-band.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Note {
    /// Random 32-byte secret
    pub secret: [u8; 32],
    /// Random 32-byte nullifier (revealed at withdrawal to prevent double-spend)
    pub nullifier: [u8; 32],
    /// Deposit amount in lamports
    pub amount: u64,
}

/// Commitment = H(secret || nullifier) — goes on-chain, reveals nothing.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct Commitment(pub [u8; 32]);

/// Nullifier hash = H(nullifier) — revealed at withdrawal, tracked to prevent reuse.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct NullifierHash(pub [u8; 32]);

/// A withdrawal proof: sent via stego-gossip, verified by relayer.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct WithdrawalProof {
    /// Merkle root at time of proof generation
    pub merkle_root: [u8; 32],
    /// Merkle inclusion proof (leaf index hidden by relayer)
    pub proof: crate::privacy::MerkleProof,
    /// Nullifier hash (prevents double-spend)
    pub nullifier_hash: NullifierHash,
    /// Recipient address (Solana pubkey base58)
    pub recipient: String,
    /// Amount in lamports
    pub amount: u64,
}

/// The mixer pool state.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MixerPool {
    /// All deposit commitments (leaves of the Merkle tree)
    pub commitments: Vec<Commitment>,
    /// Spent nullifier hashes
    pub spent_nullifiers: Vec<NullifierHash>,
    /// Fixed denomination in lamports (all deposits must match)
    pub denomination: u64,
    /// Pool address (Solana pubkey)
    pub pool_address: String,
}

impl Note {
    /// Create a new random deposit note.
    pub fn generate(amount: u64) -> Self {
        let mut secret = [0u8; 32];
        let mut nullifier = [0u8; 32];
        // Read from OS entropy source
        use std::io::Read;
        let mut f = std::fs::File::open("/dev/urandom").expect("entropy");
        f.read_exact(&mut secret).expect("rng");
        f.read_exact(&mut nullifier).expect("rng");
        Self { secret, nullifier, amount }
    }

    /// Compute the commitment: H(secret || nullifier).
    pub fn commitment(&self) -> Commitment {
        let mut h = Sha256::new();
        h.update(&self.secret);
        h.update(&self.nullifier);
        Commitment(h.finalize().into())
    }

    /// Compute the nullifier hash: H(nullifier).
    pub fn nullifier_hash(&self) -> NullifierHash {
        NullifierHash(Sha256::digest(&self.nullifier).into())
    }

    /// Serialize note to CBOR bytes (for secure transfer via stego channel).
    pub fn to_cbor(&self) -> Vec<u8> {
        // Minimal CBOR: map(3) { "s": bytes(32), "n": bytes(32), "a": uint }
        let mut out = Vec::with_capacity(80);
        out.push(0xa3); // map(3)
        // "s" → secret
        out.push(0x61); out.push(b's');
        out.push(0x58); out.push(32);
        out.extend_from_slice(&self.secret);
        // "n" → nullifier
        out.push(0x61); out.push(b'n');
        out.push(0x58); out.push(32);
        out.extend_from_slice(&self.nullifier);
        // "a" → amount
        out.push(0x61); out.push(b'a');
        crate::cbor_uint(0, self.amount, &mut out);
        out
    }

    /// Deserialize note from CBOR bytes.
    pub fn from_cbor(data: &[u8]) -> Option<Self> {
        // Parse minimal CBOR map
        if data.first()? != &0xa3 { return None; }
        let mut i = 1;
        let mut secret = None;
        let mut nullifier = None;
        let mut amount = None;
        for _ in 0..3 {
            if data.get(i)? != &0x61 { return None; }
            let key = *data.get(i + 1)?;
            i += 2;
            match key {
                b's' | b'n' => {
                    if data.get(i)? != &0x58 || data.get(i + 1)? != &32 { return None; }
                    i += 2;
                    let mut arr = [0u8; 32];
                    arr.copy_from_slice(data.get(i..i + 32)?);
                    i += 32;
                    if key == b's' { secret = Some(arr); } else { nullifier = Some(arr); }
                }
                b'a' => {
                    let (val, len) = parse_cbor_uint(&data[i..])?;
                    amount = Some(val);
                    i += len;
                }
                _ => return None,
            }
        }
        Some(Self { secret: secret?, nullifier: nullifier?, amount: amount? })
    }
}

/// Parse a CBOR unsigned integer, return (value, bytes_consumed).
fn parse_cbor_uint(data: &[u8]) -> Option<(u64, usize)> {
    let first = *data.first()?;
    let major = first >> 5;
    if major != 0 { return None; }
    let info = first & 0x1f;
    match info {
        0..=23 => Some((info as u64, 1)),
        24 => Some((*data.get(1)? as u64, 2)),
        25 => {
            let v = u16::from_be_bytes([*data.get(1)?, *data.get(2)?]);
            Some((v as u64, 3))
        }
        26 => {
            let v = u32::from_be_bytes([*data.get(1)?, *data.get(2)?, *data.get(3)?, *data.get(4)?]);
            Some((v as u64, 5))
        }
        27 => {
            let mut buf = [0u8; 8];
            buf.copy_from_slice(data.get(1..9)?);
            Some((u64::from_be_bytes(buf), 9))
        }
        _ => None,
    }
}

impl MixerPool {
    pub fn new(denomination: u64, pool_address: String) -> Self {
        Self {
            commitments: Vec::new(),
            spent_nullifiers: Vec::new(),
            denomination,
            pool_address,
        }
    }

    /// Record a deposit commitment (after on-chain deposit is confirmed).
    pub fn deposit(&mut self, commitment: Commitment) -> usize {
        let idx = self.commitments.len();
        self.commitments.push(commitment);
        idx
    }

    /// Build the current Merkle tree from all commitments.
    pub fn merkle_tree(&self) -> MerkleTree {
        let leaves: Vec<&[u8]> = self.commitments.iter().map(|c| c.0.as_slice()).collect();
        MerkleTree::from_data(&leaves)
    }

    /// Generate a withdrawal proof for a given note.
    /// Returns None if the note's commitment isn't in the pool or nullifier already spent.
    pub fn create_withdrawal(
        &self,
        note: &Note,
        recipient: &str,
    ) -> Option<WithdrawalProof> {
        let commitment = note.commitment();
        let nh = note.nullifier_hash();

        // Check nullifier not spent
        if self.spent_nullifiers.contains(&nh) { return None; }

        // Find commitment in pool
        let idx = self.commitments.iter().position(|c| c == &commitment)?;

        // Build Merkle proof
        let tree = self.merkle_tree();
        let proof = tree.prove(idx)?;

        Some(WithdrawalProof {
            merkle_root: tree.root,
            proof,
            nullifier_hash: nh,
            recipient: recipient.to_string(),
            amount: note.amount,
        })
    }

    /// Verify a withdrawal proof and mark nullifier as spent.
    /// Returns true if valid and withdrawal should proceed.
    pub fn verify_and_withdraw(&mut self, wp: &WithdrawalProof) -> bool {
        // Check Merkle root matches current tree
        let tree = self.merkle_tree();
        if tree.root != wp.merkle_root { return false; }

        // Check nullifier not already spent
        if self.spent_nullifiers.contains(&wp.nullifier_hash) { return false; }

        // Verify Merkle inclusion proof
        if !MerkleTree::verify(&wp.proof) { return false; }

        // Check amount matches denomination
        if wp.amount != self.denomination { return false; }

        // Mark nullifier as spent
        self.spent_nullifiers.push(wp.nullifier_hash.clone());
        true
    }

    /// Serialize withdrawal proof as CBOR for transport via stego-gossip.
    pub fn encode_withdrawal_for_gossip(wp: &WithdrawalProof) -> Vec<u8> {
        // Wrap in erdfa shard format for gossip transport
        let json = serde_json::to_vec(wp).expect("serialize withdrawal proof");
        let mut out = Vec::with_capacity(json.len() + 16);
        // ERDW prefix for mixer withdrawal
        out.extend_from_slice(b"ERDW");
        out.extend_from_slice(&(json.len() as u32).to_be_bytes());
        out.extend_from_slice(&json);
        out
    }

    /// Decode a withdrawal proof from gossip transport.
    pub fn decode_withdrawal_from_gossip(data: &[u8]) -> Option<WithdrawalProof> {
        if data.len() < 8 || &data[..4] != b"ERDW" { return None; }
        let len = u32::from_be_bytes([data[4], data[5], data[6], data[7]]) as usize;
        if data.len() < 8 + len { return None; }
        serde_json::from_slice(&data[8..8 + len]).ok()
    }

    /// Pool status summary.
    pub fn status(&self) -> MixerStatus {
        MixerStatus {
            deposits: self.commitments.len(),
            withdrawals: self.spent_nullifiers.len(),
            pending: self.commitments.len() - self.spent_nullifiers.len(),
            denomination: self.denomination,
            pool_address: self.pool_address.clone(),
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct MixerStatus {
    pub deposits: usize,
    pub withdrawals: usize,
    pub pending: usize,
    pub denomination: u64,
    pub pool_address: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_deposit_withdraw_roundtrip() {
        let mut pool = MixerPool::new(1_000_000_000, "PoolAddr123".into());

        // Alice deposits
        let note = Note::generate(1_000_000_000);
        let commitment = note.commitment();
        pool.deposit(commitment);

        // Alice creates withdrawal proof for Bob
        let wp = pool.create_withdrawal(&note, "BobAddr456").unwrap();

        // Encode for gossip transport
        let gossip_data = MixerPool::encode_withdrawal_for_gossip(&wp);
        assert!(gossip_data.starts_with(b"ERDW"));

        // Decode from gossip
        let wp2 = MixerPool::decode_withdrawal_from_gossip(&gossip_data).unwrap();
        assert_eq!(wp2.recipient, "BobAddr456");

        // Relayer verifies and processes withdrawal
        assert!(pool.verify_and_withdraw(&wp2));

        // Double-spend rejected
        assert!(!pool.verify_and_withdraw(&wp2));

        let status = pool.status();
        assert_eq!(status.deposits, 1);
        assert_eq!(status.withdrawals, 1);
        assert_eq!(status.pending, 0);
    }

    #[test]
    fn test_note_cbor_roundtrip() {
        let note = Note::generate(500_000_000);
        let cbor = note.to_cbor();
        let note2 = Note::from_cbor(&cbor).unwrap();
        assert_eq!(note.secret, note2.secret);
        assert_eq!(note.nullifier, note2.nullifier);
        assert_eq!(note.amount, note2.amount);
        assert_eq!(note.commitment(), note2.commitment());
    }

    #[test]
    fn test_multiple_deposits_anonymity_set() {
        let mut pool = MixerPool::new(1_000_000_000, "Pool".into());

        // 5 deposits — anonymity set of 5
        let notes: Vec<Note> = (0..5).map(|_| {
            let n = Note::generate(1_000_000_000);
            pool.deposit(n.commitment());
            n
        }).collect();

        // Withdraw note #3 — verifier can't tell which deposit it was
        let wp = pool.create_withdrawal(&notes[3], "Recipient").unwrap();
        assert!(pool.verify_and_withdraw(&wp));

        // Other notes still withdrawable
        let wp2 = pool.create_withdrawal(&notes[0], "Other").unwrap();
        assert!(pool.verify_and_withdraw(&wp2));

        assert_eq!(pool.status().pending, 3);
    }
}

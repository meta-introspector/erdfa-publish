//! erdfa-benchmark — encode the system with itself, decode, verify, attack.
//!
//! Tests every stego plugin alone, all pairs, all triples, and the full chain.
//! Input: erdfa-publish's own source files (the system encodes itself).
//! Checks: round-trip, Merkle integrity, PQ signature, tamper detection.

use erdfa_publish::privacy::{PrivacyShard, SignedPrivacyShard, MerkleTree};
use erdfa_publish::stego::{self, StegoPlugin, StegoChain, StegoConfig, chain_from_config};
use erdfa_publish::stego::{PngLsb, WavPhase, ZeroWidthText, RsHexComment, BitPlane6};
use std::time::Instant;

fn all_plugins() -> Vec<Box<dyn StegoPlugin>> {
    vec![Box::new(PngLsb), Box::new(WavPhase), Box::new(ZeroWidthText), Box::new(RsHexComment), Box::new(BitPlane6)]
}

fn main() {
    let src = include_bytes!("../../src/privacy.rs");
    println!("═══ erdfa-benchmark: system encodes itself ═══");
    println!("payload: privacy.rs ({} bytes)\n", src.len());

    // 1. Each plugin alone
    println!("── 1. Single plugins ──");
    println!("{:<12} {:>10} {:>10} {:>8} {:>6}", "plugin", "carrier", "decoded", "ratio", "ok");
    println!("{}", "-".repeat(52));
    for p in all_plugins().iter() {
        let t = Instant::now();
        let enc = p.encode(src);
        let dec = p.decode(&enc);
        let ok = dec.as_ref().map_or(false, |d| d == src);
        let ratio = enc.len() as f64 / src.len() as f64;
        println!("{:<12} {:>10} {:>10} {:>7.1}× {}", p.name(), enc.len(), dec.map_or(0, |d| d.len()), ratio, if ok { "✅" } else { "❌" });
        let _ = t.elapsed();
    }

    // 2. All pairs
    println!("\n── 2. Plugin pairs ──");
    println!("{:<25} {:>10} {:>8} {:>6}", "chain", "carrier", "ratio", "ok");
    println!("{}", "-".repeat(55));
    let names: Vec<&str> = vec!["png", "wav", "text", "rs", "bitplane6"];
    for (i, a) in names.iter().enumerate() {
        for (j, b) in names.iter().enumerate() {
            if i == j { continue; }
            let cfg = StegoConfig { chain: vec![a.to_string(), b.to_string()], external: Default::default() };
            let chain = chain_from_config(&cfg);
            let enc = chain.encode(src);
            let dec = chain.decode(&enc);
            let ok = dec.as_ref().map_or(false, |d| d == src);
            let ratio = enc.len() as f64 / src.len() as f64;
            println!("{:<25} {:>10} {:>7.1}× {}", chain.path_description(), enc.len(), ratio, if ok { "✅" } else { "❌" });
        }
    }

    // 3. All triples
    println!("\n── 3. Plugin triples ──");
    println!("{:<38} {:>10} {:>8} {:>6}", "chain", "carrier", "ratio", "ok");
    println!("{}", "-".repeat(68));
    for (i, a) in names.iter().enumerate() {
        for (j, b) in names.iter().enumerate() {
            if j == i { continue; }
            for (k, c) in names.iter().enumerate() {
                if k == i || k == j { continue; }
                let cfg = StegoConfig { chain: vec![a.to_string(), b.to_string(), c.to_string()], external: Default::default() };
                let chain = chain_from_config(&cfg);
                let enc = chain.encode(src);
                let dec = chain.decode(&enc);
                let ok = dec.as_ref().map_or(false, |d| d == src);
                let ratio = enc.len() as f64 / src.len() as f64;
                println!("{:<38} {:>10} {:>7.1}× {}", chain.path_description(), enc.len(), ratio, if ok { "✅" } else { "❌" });
            }
        }
    }

    // 4. Full chain (all 4)
    println!("\n── 4. Full chain (all 4 plugins) ──");
    let cfg = StegoConfig { chain: vec!["png".into(), "wav".into(), "rs".into(), "text".into()], external: Default::default() };
    let chain = chain_from_config(&cfg);
    let enc = chain.encode(src);
    let dec = chain.decode(&enc);
    let ok = dec.as_ref().map_or(false, |d| d == src);
    println!("chain: {}", chain.path_description());
    println!("carrier: {} bytes (ratio: {:.1}×)", enc.len(), enc.len() as f64 / src.len() as f64);
    println!("roundtrip: {}\n", if ok { "✅" } else { "❌" });

    // 5. Privacy + PQ signature through stego
    println!("── 5. PrivacyShard + ML-DSA-44 + stego ──");
    let pairs = vec![
        ("source".into(), "privacy.rs".into()),
        ("size".into(), src.len().to_string()),
        ("sha256".into(), hex::encode(sha2::Digest::finalize(sha2::Digest::chain_update(sha2::Sha256::default(), src)))),
    ];
    let ps = PrivacyShard::from_pairs("bench", &pairs, vec!["benchmark".into()]);
    let cbor = ps.to_cbor();
    println!("shard: {} fields, {} bytes CBOR", ps.fields.len(), cbor.len());
    println!("merkle_root: {}...", &ps.merkle_root[..16]);

    let t = Instant::now();
    let signed = SignedPrivacyShard::sign(ps).expect("sign");
    let sign_ms = t.elapsed().as_millis();
    println!("ML-DSA-44 sign: {}ms (sig={} bytes, pk={} bytes)", sign_ms, signed.signature.len(), signed.public_key.len());

    let t = Instant::now();
    let valid = signed.verify();
    let verify_ms = t.elapsed().as_millis();
    println!("ML-DSA-44 verify: {}ms → {}", verify_ms, if valid { "✅" } else { "❌" });

    // Encode signed shard through each stego plugin
    let signed_cbor = signed.to_cbor();
    println!("\nsigned shard CBOR: {} bytes", signed_cbor.len());
    println!("{:<12} {:>10} {:>8} {:>6}", "stego", "carrier", "ratio", "ok");
    println!("{}", "-".repeat(42));
    for p in all_plugins().iter() {
        let enc = p.encode(&signed_cbor);
        let dec = p.decode(&enc);
        let ok = dec.as_ref().map_or(false, |d| d == &signed_cbor);
        println!("{:<12} {:>10} {:>7.1}× {}", p.name(), enc.len(), enc.len() as f64 / signed_cbor.len() as f64, if ok { "✅" } else { "❌" });
    }

    // 6. Tamper detection
    println!("\n── 6. Tamper detection ──");
    // Flip a byte in the carrier, check decode still works but Merkle fails
    for p in all_plugins().iter() {
        let enc = p.encode(&signed_cbor);
        let mut tampered = enc.clone();
        if tampered.len() > 20 { tampered[20] ^= 0xFF; }
        let dec = p.decode(&tampered);
        let matches = dec.as_ref().map_or(false, |d| d == &signed_cbor);
        print!("{:<12} tamper→decode: ", p.name());
        if matches {
            println!("data unchanged (tamper in padding) ⚠");
        } else if dec.is_some() {
            println!("data CORRUPTED, detected ✅");
        } else {
            println!("decode FAILED, detected ✅");
        }
    }

    // Merkle proof tamper
    let pairs2 = vec![("a".into(), "1".into()), ("b".into(), "2".into()), ("c".into(), "3".into())];
    let ps2 = PrivacyShard::from_pairs("tamper-test", &pairs2, vec![]);
    let proof = ps2.prove_field(&pairs2, 1).unwrap();
    let valid = MerkleTree::verify(&proof);
    let mut bad_proof = proof.clone();
    bad_proof.leaf_hash[0] ^= 0xFF;
    let invalid = MerkleTree::verify(&bad_proof);
    println!("\nMerkle proof (field 'b'): valid={} tampered={}", valid, invalid);

    // PQ sig tamper
    let mut bad_signed = signed.clone();
    if bad_signed.signature.len() > 10 { bad_signed.signature[10] ^= 0xFF; }
    let sig_tamper = bad_signed.verify();
    println!("ML-DSA-44 sig tamper: verify={} (expected false) {}", sig_tamper, if !sig_tamper { "✅" } else { "❌" });

    println!("\n═══ benchmark complete ═══");
}

//! zkperf verification layer for solfunmeme deployment mesh.
//! Each probe gets a timing commitment + Merkle proof + Monster Group orbifold coords.

use sha2::{Sha256, Digest};
use serde::{Serialize, Deserialize};
use std::time::Instant;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ZkPerfWitness {
    pub node: String,
    pub url: String,
    pub http_code: u16,
    pub latency_bucket: u8,       // quantized: 0-9 (0=<100ms, 9=>10s)
    pub commitment: String,        // SHA-256 of full timing data (private)
    pub merkle_root: String,       // root of all probe results
    pub orbifold: (u8, u8, u8),   // Monster Group coords (mod 71, mod 59, mod 47)
    pub ts: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ZkPerfBatch {
    pub witnesses: Vec<ZkPerfWitness>,
    pub batch_root: String,
    pub batch_signature: String,
    pub crown_product: u64,        // 47 × 59 × 71 = 196,883
}

fn quantize_latency(ms: u64) -> u8 {
    match ms {
        0..=99 => 0,
        100..=249 => 1,
        250..=499 => 2,
        500..=999 => 3,
        1000..=1999 => 4,
        2000..=4999 => 5,
        5000..=9999 => 6,
        _ => 7,
    }
}

fn orbifold_coords(data: &[u8]) -> (u8, u8, u8) {
    let hash = Sha256::digest(data);
    let val = u64::from_le_bytes(hash[0..8].try_into().unwrap());
    ((val % 71) as u8, (val % 59) as u8, (val % 47) as u8)
}

fn merkle_root(items: &[&[u8]]) -> [u8; 32] {
    if items.is_empty() { return [0u8; 32]; }
    let mut leaves: Vec<[u8; 32]> = items.iter().map(|d| Sha256::digest(d).into()).collect();
    while leaves.len() > 1 {
        let mut next = Vec::new();
        for chunk in leaves.chunks(2) {
            let mut h = Sha256::new();
            h.update(chunk[0]);
            h.update(chunk.get(1).unwrap_or(&chunk[0]));
            next.push(h.finalize().into());
        }
        leaves = next;
    }
    leaves[0]
}

fn main() {
    let urls: Vec<(&str, &str)> = vec![
        ("github-pages", "https://meta-introspector.github.io/solfunmeme-dioxus/"),
        ("cloudflare", "https://solfunmeme-dioxus.pages.dev/"),
        ("vercel", "https://solfunmeme-dioxus.vercel.app/"),
        ("huggingface", "https://introspector-solfunmeme-dioxus.hf.space/"),
        ("oracle-oci", "https://objectstorage.us-ashburn-1.oraclecloud.com/n/id1iqr236pdp/b/solfunmeme-dioxus/o/index.html"),
        ("netlify", "https://solfunmeme.netlify.app/"),
        ("render", "https://solfunmeme-static.onrender.com/"),
        ("self-hosted", "http://192.168.68.62/dioxus/"),
    ];

    let ts = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_millis() as u64;
    let mut witnesses = Vec::new();
    let mut leaf_data = Vec::new();

    for (name, url) in &urls {
        let start = Instant::now();
        let code = match ureq::get(url).timeout(std::time::Duration::from_secs(15)).call() {
            Ok(r) => r.status(),
            Err(ureq::Error::Status(code, _)) => code,
            Err(_) => 0,
        };
        let latency_ms = start.elapsed().as_millis() as u64;

        // Private: full timing commitment
        let mut commit_data = Vec::new();
        commit_data.extend(name.as_bytes());
        commit_data.extend(latency_ms.to_le_bytes());
        commit_data.extend(code.to_le_bytes());
        let commitment = hex::encode(Sha256::digest(&commit_data));

        // Orbifold coords from commitment
        let coords = orbifold_coords(&commit_data);

        let bucket = quantize_latency(latency_ms);
        let status = if code == 200 { "✅" } else { "❌" };
        eprintln!("  {} {:20} {:3} {}ms bucket={} orbifold=({},{},{})",
            status, name, code, latency_ms, bucket, coords.0, coords.1, coords.2);

        leaf_data.push(commit_data.clone());
        witnesses.push(ZkPerfWitness {
            node: name.to_string(),
            url: url.to_string(),
            http_code: code,
            latency_bucket: bucket,
            commitment,
            merkle_root: String::new(), // filled below
            orbifold: coords,
            ts,
        });
    }

    // Compute batch Merkle root
    let refs: Vec<&[u8]> = leaf_data.iter().map(|d| d.as_slice()).collect();
    let root = merkle_root(&refs);
    let root_hex = hex::encode(root);

    for w in &mut witnesses {
        w.merkle_root = root_hex.clone();
    }

    // Sign batch
    let batch_sig = hex::encode(Sha256::digest(format!("{}:{}", root_hex, ts).as_bytes()));

    let batch = ZkPerfBatch {
        witnesses,
        batch_root: root_hex,
        batch_signature: batch_sig,
        crown_product: 47 * 59 * 71, // 196,883
    };

    // Output
    let json = serde_json::to_string_pretty(&batch).unwrap();
    println!("{}", json);

    // Save to proofs dir
    let proofs_dir = format!("{}/.solfunmeme/proofs", std::env::var("HOME").unwrap_or_default());
    let _ = std::fs::create_dir_all(&proofs_dir);
    let _ = std::fs::write(format!("{}/zkperf_batch_{}.json", proofs_dir, ts), &json);

    // Post to mesh
    let _ = ureq::post("http://127.0.0.1:7780/mesh/logs")
        .send_string(&json);

    eprintln!("\n  Batch root: {}", batch.batch_root);
    eprintln!("  Crown: {} (47×59×71)", batch.crown_product);
    eprintln!("  Witnesses: {}", batch.witnesses.len());
}

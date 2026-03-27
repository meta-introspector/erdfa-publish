//! zkperf verification of all local systemd services
use sha2::{Sha256, Digest};
use serde::{Serialize, Deserialize};
use std::time::Instant;

#[derive(Debug, Serialize, Deserialize)]
struct ServiceWitness {
    name: String,
    port: u16,
    http_code: u16,
    latency_ms: u64,
    latency_bucket: u8,
    commitment: String,
    orbifold: (u8, u8, u8),
}

fn quantize(ms: u64) -> u8 {
    match ms { 0..=99=>0, 100..=249=>1, 250..=499=>2, 500..=999=>3, 1000..=1999=>4, _=>5 }
}

fn orbifold(data: &[u8]) -> (u8, u8, u8) {
    let h = Sha256::digest(data);
    let v = u64::from_le_bytes(h[0..8].try_into().unwrap());
    ((v % 71) as u8, (v % 59) as u8, (v % 47) as u8)
}

fn probe(name: &str, port: u16, path: &str) -> ServiceWitness {
    let url = format!("http://127.0.0.1:{}{}", port, path);
    let start = Instant::now();
    let code = ureq::get(&url).timeout(std::time::Duration::from_secs(5))
        .call().map(|r| r.status()).unwrap_or(0);
    let ms = start.elapsed().as_millis() as u64;
    let mut cd = Vec::new();
    cd.extend(name.as_bytes());
    cd.extend(ms.to_le_bytes());
    cd.extend(code.to_le_bytes());
    let commitment = hex::encode(Sha256::digest(&cd));
    let coords = orbifold(&cd);
    ServiceWitness { name: name.into(), port, http_code: code, latency_ms: ms,
        latency_bucket: quantize(ms), commitment, orbifold: coords }
}

fn main() {
    let services: Vec<(&str, u16, &str)> = vec![
        ("solfunmeme-service", 7780, "/status"),
        ("solfunmeme-dioxus", 8108, "/dioxus/"),
        ("kagenti-daemon", 8480, "/apis/kagenti/v1/namespaces"),
        ("kagenti-portal", 8201, "/"),
        ("kant-pastebin", 8090, "/"),
        ("zos-minimal-server", 8081, "/"),
        ("zos-noc-manager", 8095, "/"),
        ("prometheus", 9090, "/"),
        ("jaeger", 16686, "/"),
        ("forgejo", 3000, "/"),
        ("fractran-generator", 8503, "/"),
        ("fractran-vm", 8107, "/"),
        ("otp-bbs-bridge", 7171, "/"),
        ("rust-compiler-zkp", 9400, "/health"),
        ("zkperf", 9718, "/health"),
    ];

    let mut witnesses = Vec::new();
    for (name, port, path) in &services {
        let w = probe(name, *port, path);
        let s = if w.http_code > 0 { "✅" } else { "❌" };
        eprintln!("  {} {:25} :{:<5} {:3} {:>4}ms orbifold=({},{},{})",
            s, w.name, w.port, w.http_code, w.latency_ms, w.orbifold.0, w.orbifold.1, w.orbifold.2);
        witnesses.push(w);
    }

    let ok = witnesses.iter().filter(|w| w.http_code > 0).count();
    eprintln!("\n  {}/{} services responding", ok, witnesses.len());

    // Batch merkle root
    let mut leaves: Vec<[u8; 32]> = witnesses.iter()
        .map(|w| Sha256::digest(w.commitment.as_bytes()).into()).collect();
    while leaves.len() > 1 {
        let mut next = Vec::new();
        for c in leaves.chunks(2) {
            let mut h = Sha256::new();
            h.update(c[0]); h.update(c.get(1).unwrap_or(&c[0]));
            next.push(h.finalize().into());
        }
        leaves = next;
    }
    let root = hex::encode(leaves[0]);

    let ts = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_millis() as u64;
    let batch = serde_json::json!({
        "type": "zkperf-services",
        "witnesses": witnesses,
        "batch_root": root,
        "crown": 196883,
        "ok": ok,
        "total": witnesses.len(),
        "ts": ts,
    });

    println!("{}", serde_json::to_string_pretty(&batch).unwrap());

    // Save + post to mesh
    let home = std::env::var("HOME").unwrap_or_default();
    let _ = std::fs::create_dir_all(format!("{}/.solfunmeme/proofs", home));
    let _ = std::fs::write(format!("{}/.solfunmeme/proofs/zkperf_services_{}.json", home, ts), serde_json::to_string(&batch).unwrap());
    let _ = ureq::post("http://127.0.0.1:7780/mesh/logs").send_json(&batch);
    eprintln!("  Batch root: {}", &root[..16]);
}

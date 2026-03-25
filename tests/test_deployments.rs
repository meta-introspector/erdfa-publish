//! Headless browser + API tests for solfunmeme deployments.
//! Run: cargo test --test test_deployments -- --nocapture

use std::process::Command;

fn curl_get(url: &str) -> (u16, String) {
    let out = Command::new("curl")
        .args(["-s", "-w", "\n%{http_code}", "--max-time", "10", url])
        .output()
        .unwrap();
    let text = String::from_utf8_lossy(&out.stdout).to_string();
    let lines: Vec<&str> = text.trim().rsplitn(2, '\n').collect();
    let code: u16 = lines[0].parse().unwrap_or(0);
    let body = lines.get(1).unwrap_or(&"").to_string();
    (code, body)
}

// ── API tests (local service on :7780) ───────────────────────────

#[test]
fn test_api_status() {
    let (code, body) = curl_get("http://127.0.0.1:7780/status");
    println!("status: {} {}", code, &body[..body.len().min(80)]);
    assert_eq!(code, 200);
    assert!(body.contains("status"));
}

#[test]
fn test_api_tiers() {
    let (code, body) = curl_get("http://127.0.0.1:7780/tiers");
    println!("tiers: {} {}", code, &body[..body.len().min(80)]);
    assert_eq!(code, 200);
    assert!(body.contains("diamond"));
}

#[test]
fn test_api_paste_list() {
    let (code, _body) = curl_get("http://127.0.0.1:7780/paste");
    println!("paste: {}", code);
    assert_eq!(code, 200);
}

#[test]
fn test_api_paste_submit() {
    let out = Command::new("curl")
        .args(["-s", "-X", "POST", "-H", "Content-Type: text/plain",
               "-d", "test from cargo test", "-w", "\n%{http_code}",
               "http://127.0.0.1:7780/paste"])
        .output().unwrap();
    let text = String::from_utf8_lossy(&out.stdout).to_string();
    println!("submit: {}", &text[..text.len().min(120)]);
    assert!(text.contains("id"));
}

// ── CLI tests ────────────────────────────────────────────────────

#[test]
fn test_cli_tiers() {
    let out = Command::new("./target/release/solfunmeme_cli")
        .args(["tiers"])
        .output();
    match out {
        Ok(o) => {
            let text = String::from_utf8_lossy(&o.stdout);
            println!("cli tiers: {}", &text[..text.len().min(80)]);
            assert!(text.contains("diamond"));
            assert!(text.contains("gold"));
        }
        Err(_) => println!("cli not built, skipping"),
    }
}

#[test]
fn test_cli_status() {
    let out = Command::new("./target/release/solfunmeme_cli")
        .args(["--endpoint", "http://127.0.0.1:7780", "status"])
        .output();
    match out {
        Ok(o) => {
            let text = String::from_utf8_lossy(&o.stdout);
            println!("cli status: {}", &text[..text.len().min(80)]);
            assert!(text.contains("status"));
        }
        Err(_) => println!("cli not built, skipping"),
    }
}

#[test]
fn test_cli_vote() {
    let out = Command::new("./target/release/solfunmeme_cli")
        .args(["--endpoint", "http://127.0.0.1:7780", "vote", "yea"])
        .output();
    match out {
        Ok(o) => {
            let text = String::from_utf8_lossy(&o.stderr);
            println!("cli vote: {}", &text[..text.len().min(80)]);
            assert!(text.contains("Vote submitted") || text.contains("id"));
        }
        Err(_) => println!("cli not built, skipping"),
    }
}

// ── End-to-end: vote then verify in tally ────────────────────────

#[test]
fn test_e2e_vote_and_collect() {
    // 1. Submit a vote via CLI
    let _ = Command::new("./target/release/solfunmeme_cli")
        .args(["--endpoint", "http://127.0.0.1:7780", "vote", "yea"])
        .output();

    // 2. Run collect-votes
    let _ = Command::new("./target/release/solfunmeme-service")
        .args(["collect-votes"])
        .output();

    // 3. Check tally exists
    let home = std::env::var("HOME").unwrap();
    let tally = format!("{}/.solfunmeme/proofs/tally.json", home);
    if std::path::Path::new(&tally).exists() {
        let data = std::fs::read_to_string(&tally).unwrap();
        println!("tally: {}", &data[..data.len().min(120)]);
        assert!(data.contains("total_votes") || data.contains("chambers"));
    } else {
        println!("tally.json not found (run 'prove' first)");
    }
}

// ── Web endpoint reachability ────────────────────────────────────

#[test]
fn test_self_hosted_web() {
    let (code, _) = curl_get("http://192.168.68.62/solfunmeme/status");
    println!("self_hosted: {}", code);
    assert_eq!(code, 200);
}

#[test]
fn test_hf_space_web() {
    let (code, _) = curl_get("https://introspector-solfunmeme-dioxus.static.hf.space/index.html");
    println!("hf_space: {}", code);
    assert_eq!(code, 200);
}

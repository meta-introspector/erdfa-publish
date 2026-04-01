//! Ingest archived tickets (issues/PRs) as DA51 CBOR shards.
//!
//! Reads JSON issue archives from git mirrors, converts each ticket
//! to an erdfa Shard with DA51 tag, outputs .cbor files or tar archive.

use erdfa_publish::{Shard, ShardSet, Component};
use serde::Deserialize;
use std::{fs, path::PathBuf};

#[derive(Deserialize)]
struct Ticket {
    number: Option<u64>,
    title: Option<String>,
    body: Option<String>,
    state: Option<String>,
    #[serde(alias = "createdAt", alias = "created_at")]
    created_at: Option<String>,
}

fn ingest_file(path: &str, repo: &str) -> Vec<Shard> {
    let data = fs::read_to_string(path).unwrap_or_default();
    let tickets: Vec<Ticket> = serde_json::from_str(&data).unwrap_or_default();
    tickets.iter().map(|t| {
        let num = t.number.unwrap_or(0);
        let title = t.title.clone().unwrap_or_default();
        let state = t.state.clone().unwrap_or_default();
        let body = t.body.clone().unwrap_or_default();
        let created = t.created_at.clone().unwrap_or_default();
        let id = format!("{repo}-ticket-{num}");
        let component = Component::KeyValue {
            pairs: vec![
                ("repo".into(), repo.into()),
                ("number".into(), num.to_string()),
                ("title".into(), title),
                ("state".into(), state),
                ("created".into(), created),
                ("body".into(), body.chars().take(500).collect()),
            ],
        };
        Shard::new(id, component)
            .with_tags(vec!["ticket".into(), repo.into()])
    }).collect()
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let git_dir = args.get(1).map(|s| s.as_str())
        .unwrap_or("/mnt/data1/git/solana.solfunmeme.com");
    let out_dir = args.get(2).map(|s| s.as_str())
        .unwrap_or("/tmp/ticket-shards");
    fs::create_dir_all(out_dir).ok();

    let mut set = ShardSet::new("ticket-archive");
    let mut total = 0;

    for entry in fs::read_dir(git_dir).unwrap().flatten() {
        let path = entry.path();
        if !path.extension().map_or(false, |e| e == "git") { continue; }
        let repo = path.file_stem().unwrap().to_string_lossy().replace(".git", "");
        for kind in ["issues", "prs"] {
            let jf = path.join(format!("{kind}.json"));
            if !jf.exists() { continue; }
            let shards = ingest_file(jf.to_str().unwrap(), &format!("{repo}/{kind}"));
            for shard in &shards {
                let cbor = shard.to_cbor();
                fs::write(format!("{out_dir}/{}.cbor", shard.id), &cbor).ok();
                set.add(shard);
            }
            total += shards.len();
            println!("{repo}/{kind}: {} shards", shards.len());
        }
    }

    let manifest = set.to_cbor();
    fs::write(format!("{out_dir}/manifest.cbor"), &manifest).ok();
    println!("\nTotal: {total} shards + manifest -> {out_dir}");
}

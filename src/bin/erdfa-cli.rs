use clap::{Parser, Subcommand};
use erdfa_publish::{Component, Shard, ShardSet};
use erdfa_publish::cft;
use erdfa_publish::render::{decode_shard, render_text};
use serde_json::{json, Value};
use std::collections::{HashMap, HashSet, BTreeMap};
use std::fs;
use std::fs::File;
use std::path::PathBuf;
use parquet::file::reader::{FileReader, SerializedFileReader};
use parquet::record::Field;

#[derive(Parser)]
#[command(name = "erdfa-cli", about = "Manage eRDFa CBOR shards")]
struct Cli {
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// List shards in a directory (JSON output)
    List { dir: PathBuf },
    /// Show a shard as JSON
    Show { file: PathBuf },
    /// Create a new shard
    Create {
        #[arg(long)]
        dir: PathBuf,
        #[arg(long)]
        id: String,
        #[arg(long, default_value = "paragraph")]
        r#type: String,
        #[arg(long)]
        text: String,
        #[arg(long, value_delimiter = ',')]
        tags: Vec<String>,
    },
    /// Import a directory of files with full CFT decomposition
    Import {
        /// Source directory of text/markdown files
        #[arg(long)]
        src: PathBuf,
        /// Output directory for CBOR shards
        #[arg(long)]
        dir: PathBuf,
        /// Max CFT depth: 0=post, 1=paragraph, 2=line, 3=token, 4=emoji, 5=byte
        #[arg(long, default_value = "2")]
        max_depth: u8,
    },
    /// Import Kiro chat parquet exports as CBOR shards
    Parquet {
        /// Directory containing conversations_v2_chunk_*.parquet files
        #[arg(long)]
        src: PathBuf,
        /// Output directory for CBOR shards
        #[arg(long)]
        dir: PathBuf,
        /// Max CFT depth
        #[arg(long, default_value = "1")]
        max_depth: u8,
    },
    /// Incremental parquet import — only process new conversations
    Refresh {
        /// Directory containing conversations_v2_chunk_*.parquet files
        #[arg(long)]
        src: PathBuf,
        /// Output directory for CBOR shards (existing shards preserved)
        #[arg(long)]
        dir: PathBuf,
        /// Max CFT depth
        #[arg(long, default_value = "1")]
        max_depth: u8,
    },
    /// Build indexes from a shard directory
    Index {
        /// Shard directory to index
        #[arg(long)]
        dir: PathBuf,
        /// Output directory for index files (defaults to dir/indexes/)
        #[arg(long)]
        out: Option<PathBuf>,
    },
    /// Collect perf parquet traces as DA51 CBOR shards
    Perf {
        /// Directory containing *_perf.parquet files (from perf2parquet)
        #[arg(long)]
        src: PathBuf,
        /// Output directory for DA51 CBOR shards
        #[arg(long)]
        dir: PathBuf,
    },
    /// Export DA51 CBOR shards as an Agda module
    Agda {
        /// Directory containing .cbor shards
        #[arg(long)]
        dir: PathBuf,
        /// Output .agda file
        #[arg(long, default_value = "PerfHistory.agda")]
        out: PathBuf,
        /// Module name
        #[arg(long, default_value = "PerfHistory")]
        module: String,
    },
}

fn main() {
    let cli = Cli::parse();
    match cli.cmd {
        Cmd::List { dir } => cmd_list(&dir),
        Cmd::Show { file } => cmd_show(&file),
        Cmd::Create { dir, id, r#type, text, tags } => cmd_create(&dir, &id, &r#type, &text, &tags),
        Cmd::Import { src, dir, max_depth } => cmd_import(&src, &dir, max_depth),
        Cmd::Parquet { src, dir, max_depth } => cmd_parquet(&src, &dir, max_depth),
        Cmd::Refresh { src, dir, max_depth } => cmd_refresh(&src, &dir, max_depth),
        Cmd::Index { dir, out } => cmd_index(&dir, out.as_deref()),
        Cmd::Perf { src, dir } => cmd_perf(&src, &dir),
        Cmd::Agda { dir, out, module } => cmd_agda(&dir, &out, &module),
    }
}

fn cmd_list(dir: &PathBuf) {
    let mut entries: Vec<Value> = Vec::new();
    if let Ok(rd) = fs::read_dir(dir) {
        for entry in rd.flatten() {
            let path = entry.path();
            if path.extension().map_or(false, |e| e == "cbor") {
                if let Ok(bytes) = fs::read(&path) {
                    if let Some(shard) = decode_shard(&bytes) {
                        entries.push(json!({
                            "file": path.file_name().unwrap().to_string_lossy(),
                            "id": shard.id,
                            "cid": shard.cid,
                            "tags": shard.tags,
                            "type": component_type(&shard.component),
                        }));
                    }
                }
            }
        }
    }
    println!("{}", serde_json::to_string(&entries).unwrap());
}

fn cmd_show(file: &PathBuf) {
    let bytes = fs::read(file).expect("cannot read file");
    let shard = decode_shard(&bytes).expect("invalid CBOR shard");
    let obj = json!({
        "id": shard.id,
        "cid": shard.cid,
        "tags": shard.tags,
        "type": component_type(&shard.component),
        "text": render_text(&shard),
        "component": serde_json::to_value(&shard.component).unwrap(),
    });
    println!("{}", serde_json::to_string_pretty(&obj).unwrap());
}

fn cmd_create(dir: &PathBuf, id: &str, typ: &str, text: &str, tags: &[String]) {
    let component = match typ {
        "heading" => Component::Heading { level: 1, text: text.into() },
        "code" => Component::Code { language: "text".into(), source: text.into() },
        "list" => Component::List { ordered: false, items: text.split('\n').map(String::from).collect() },
        _ => Component::Paragraph { text: text.into() },
    };
    let shard = Shard::new(id, component).with_tags(tags.to_vec());
    let cbor = shard.to_cbor();
    fs::create_dir_all(dir).ok();
    let path = dir.join(format!("{}.cbor", id));
    fs::write(&path, &cbor).expect("cannot write");
    let obj = json!({ "id": shard.id, "cid": shard.cid, "file": path.to_string_lossy(), "size": cbor.len() });
    println!("{}", serde_json::to_string(&obj).unwrap());
}

fn cmd_import(src: &PathBuf, dir: &PathBuf, max_depth: u8) {
    fs::create_dir_all(dir).ok();
    let mut total_shards = 0usize;
    let mut total_arrows = 0usize;
    let mut files = 0usize;

    let entries: Vec<_> = fs::read_dir(src).expect("cannot read src dir")
        .flatten()
        .filter(|e| {
            let p = e.path();
            p.is_file() && matches!(
                p.extension().and_then(|e| e.to_str()),
                Some("txt" | "md" | "org" | "rs" | "el" | "json" | "toml" | "yaml" | "yml" | "sh" | "py")
            )
        })
        .collect();

    for entry in &entries {
        let path = entry.path();
        let text = match fs::read_to_string(&path) {
            Ok(t) => t,
            Err(_) => continue,
        };
        let stem = path.file_stem().unwrap().to_string_lossy().to_string();
        let (shards, arrows) = cft::decompose(&stem, &text);

        for shard in &shards {
            let cbor = shard.to_cbor();
            // sanitize id for filename
            let fname = shard.id.replace(['/', '→', ':', ' '], "_");
            fs::write(dir.join(format!("{}.cbor", fname)), &cbor).ok();
        }
        for arrow in &arrows {
            let cbor = arrow.to_cbor();
            let fname = arrow.id.replace(['/', '→', ':', ' '], "_");
            fs::write(dir.join(format!("{}.cbor", fname)), &cbor).ok();
        }

        total_shards += shards.len();
        total_arrows += arrows.len();
        files += 1;
    }

    let obj = json!({
        "files": files,
        "shards": total_shards,
        "arrows": total_arrows,
        "dir": dir.to_string_lossy(),
    });
    println!("{}", serde_json::to_string(&obj).unwrap());
}

fn component_type(c: &Component) -> &'static str {
    match c {
        Component::Heading { .. } => "heading",
        Component::Paragraph { .. } => "paragraph",
        Component::Code { .. } => "code",
        Component::Table { .. } => "table",
        Component::Tree { .. } => "tree",
        Component::List { .. } => "list",
        Component::Link { .. } => "link",
        Component::Image { .. } => "image",
        Component::KeyValue { .. } => "keyvalue",
        Component::MapEntity { .. } => "mapentity",
        Component::Group { .. } => "group",
    }
}

fn cmd_parquet(src: &PathBuf, dir: &PathBuf, max_depth: u8) {
    fs::create_dir_all(dir).ok();

    let mut parquet_files: Vec<_> = fs::read_dir(src)
        .expect("cannot read src dir")
        .flatten()
        .filter(|e| {
            e.path().file_name()
                .and_then(|n| n.to_str())
                .map_or(false, |n| n.starts_with("conversations_v2_chunk_") && n.ends_with(".parquet"))
        })
        .map(|e| e.path())
        .collect();
    parquet_files.sort();

    let mut total_convos = 0usize;
    let mut total_shards = 0usize;

    for pf in &parquet_files {
        let file = fs::File::open(pf).expect("cannot open parquet");
        let reader = SerializedFileReader::new(file).expect("invalid parquet");
        let iter = reader.get_row_iter(None).expect("cannot read rows");

        for row in iter {
            let row = row.expect("bad row");
            let mut conv_id = String::new();
            let mut value_json = String::new();
            let mut created_at = 0i64;
            let mut pq_key = String::new();

            for (name, field) in row.get_column_iter() {
                match (name.as_str(), field) {
                    ("conversation_id", Field::Str(s)) => conv_id = s.clone(),
                    ("value", Field::Str(s)) => value_json = s.clone(),
                    ("created_at", Field::Long(n)) => created_at = *n,
                    ("key", Field::Str(s)) => pq_key = s.clone(),
                    _ => {}
                }
            }

            if conv_id.is_empty() || value_json.is_empty() { continue; }

            let v: Value = match serde_json::from_str(&value_json) {
                Ok(v) => v,
                Err(_) => continue,
            };

            // Extract text from conversation
            let mut text = String::new();
            let short_id = &conv_id[..8.min(conv_id.len())];

            // Summary
            if let Some(summary) = v.get("latest_summary") {
                if let Some(arr) = summary.as_array() {
                    for s in arr {
                        if let Some(t) = s.as_str() {
                            text.push_str(t);
                            text.push('\n');
                        }
                    }
                } else if let Some(s) = summary.as_str() {
                    text.push_str(s);
                    text.push('\n');
                }
            }

            // History turns
            if let Some(history) = v.get("history").and_then(|h| h.as_array()) {
                for turn in history {
                    // User message
                    if let Some(prompt) = turn.pointer("/user/content/Prompt/prompt") {
                        if let Some(s) = prompt.as_str() {
                            if !s.is_empty() && !s.starts_with("<tool result") {
                                text.push_str("\n## User\n");
                                text.push_str(s);
                                text.push('\n');
                            }
                        }
                    }
                    // Assistant message
                    if let Some(msg) = turn.get("assistant") {
                        if let Some(s) = msg.get("Message").and_then(|m| m.as_str()) {
                            if !s.is_empty() {
                                text.push_str("\n## Assistant\n");
                                text.push_str(s);
                                text.push('\n');
                            }
                        } else if let Some(content) = msg.pointer("/ToolUse/content").and_then(|c| c.as_str()) {
                            if !content.is_empty() {
                                text.push_str("\n## Assistant\n");
                                text.push_str(content);
                                text.push('\n');
                            }
                        }
                    }
                }
            }

            if text.trim().is_empty() { continue; }

            // Create metadata shard
            let meta_shard = Shard::new(
                &format!("{}_meta", short_id),
                Component::KeyValue {
                    pairs: vec![
                        ("conversation_id".into(), conv_id.clone()),
                        ("created_at".into(), created_at.to_string()),
                        ("key".into(), pq_key.clone()),
                        ("turns".into(), v.get("history").and_then(|h| h.as_array()).map_or(0, |a| a.len()).to_string()),
                    ],
                },
            ).with_tags(vec!["kiro".into(), "chat".into(), "meta".into()]);
            let cbor = meta_shard.to_cbor();
            fs::write(dir.join(format!("{}_meta.cbor", short_id)), &cbor).expect("write failed");

            // CFT decompose the conversation text
            let (shards, arrows) = cft::decompose_depth(short_id, &text, max_depth);
            for s in &shards {
                let s = Shard { tags: { let mut t = s.tags.clone(); t.push("kiro".into()); t.push("chat".into()); t }, ..s.clone() };
                fs::write(dir.join(format!("{}.cbor", s.id)), s.to_cbor()).expect("write failed");
            }
            for a in &arrows {
                fs::write(dir.join(format!("{}.cbor", a.id.replace('→', "_to_"))), a.to_cbor()).expect("write failed");
            }

            total_shards += 1 + shards.len() + arrows.len(); // meta + shards + arrows
            total_convos += 1;
        }
    }

    let obj = json!({
        "parquet_files": parquet_files.len(),
        "conversations": total_convos,
        "shards": total_shards,
        "dir": dir.to_string_lossy(),
    });
    println!("{}", serde_json::to_string(&obj).unwrap());
}

fn cmd_refresh(src: &PathBuf, dir: &PathBuf, max_depth: u8) {
    fs::create_dir_all(dir).ok();

    // Scan existing meta shards to find already-processed conversation IDs
    let mut existing: HashSet<String> = HashSet::new();
    if let Ok(rd) = fs::read_dir(dir) {
        for entry in rd.flatten() {
            let name = entry.file_name().to_string_lossy().to_string();
            if name.ends_with("_meta.cbor") {
                if let Ok(bytes) = fs::read(entry.path()) {
                    if let Some(shard) = decode_shard(&bytes) {
                        if let Component::KeyValue { ref pairs } = shard.component {
                            for (k, v) in pairs {
                                if k == "conversation_id" {
                                    existing.insert(v.clone());
                                }
                            }
                        }
                    }
                }
            }
        }
    }
    eprintln!("found {} existing conversations, scanning for new...", existing.len());

    // Reuse parquet reading logic but skip existing
    let mut parquet_files: Vec<_> = fs::read_dir(src)
        .expect("cannot read src dir")
        .flatten()
        .filter(|e| {
            e.path().file_name()
                .and_then(|n| n.to_str())
                .map_or(false, |n| n.starts_with("conversations_v2_chunk_") && n.ends_with(".parquet"))
        })
        .map(|e| e.path())
        .collect();
    parquet_files.sort();

    let mut new_convos = 0usize;
    let mut new_shards = 0usize;
    let mut skipped = 0usize;

    for pf in &parquet_files {
        let file = fs::File::open(pf).expect("cannot open parquet");
        let reader = SerializedFileReader::new(file).expect("invalid parquet");
        let iter = reader.get_row_iter(None).expect("cannot read rows");

        for row in iter {
            let row = row.expect("bad row");
            let mut conv_id = String::new();
            let mut value_json = String::new();
            let mut created_at = 0i64;
            let mut pq_key = String::new();

            for (name, field) in row.get_column_iter() {
                match (name.as_str(), field) {
                    ("conversation_id", Field::Str(s)) => conv_id = s.clone(),
                    ("value", Field::Str(s)) => value_json = s.clone(),
                    ("created_at", Field::Long(n)) => created_at = *n,
                    ("key", Field::Str(s)) => pq_key = s.clone(),
                    _ => {}
                }
            }

            if conv_id.is_empty() || value_json.is_empty() { continue; }
            if existing.contains(&conv_id) { skipped += 1; continue; }

            let v: Value = match serde_json::from_str(&value_json) {
                Ok(v) => v,
                Err(_) => continue,
            };

            let mut text = String::new();
            let short_id = &conv_id[..8.min(conv_id.len())];

            if let Some(summary) = v.get("latest_summary") {
                if let Some(arr) = summary.as_array() {
                    for s in arr { if let Some(t) = s.as_str() { text.push_str(t); text.push('\n'); } }
                } else if let Some(s) = summary.as_str() { text.push_str(s); text.push('\n'); }
            }
            if let Some(history) = v.get("history").and_then(|h| h.as_array()) {
                for turn in history {
                    if let Some(prompt) = turn.pointer("/user/content/Prompt/prompt") {
                        if let Some(s) = prompt.as_str() {
                            if !s.is_empty() && !s.starts_with("<tool result") {
                                text.push_str("\n## User\n"); text.push_str(s); text.push('\n');
                            }
                        }
                    }
                    if let Some(msg) = turn.get("assistant") {
                        if let Some(s) = msg.get("Message").and_then(|m| m.as_str()) {
                            if !s.is_empty() { text.push_str("\n## Assistant\n"); text.push_str(s); text.push('\n'); }
                        } else if let Some(content) = msg.pointer("/ToolUse/content").and_then(|c| c.as_str()) {
                            if !content.is_empty() { text.push_str("\n## Assistant\n"); text.push_str(content); text.push('\n'); }
                        }
                    }
                }
            }
            if text.trim().is_empty() { continue; }

            let meta_shard = Shard::new(
                &format!("{}_meta", short_id),
                Component::KeyValue {
                    pairs: vec![
                        ("conversation_id".into(), conv_id.clone()),
                        ("created_at".into(), created_at.to_string()),
                        ("key".into(), pq_key.clone()),
                        ("turns".into(), v.get("history").and_then(|h| h.as_array()).map_or(0, |a| a.len()).to_string()),
                    ],
                },
            ).with_tags(vec!["kiro".into(), "chat".into(), "meta".into()]);
            fs::write(dir.join(format!("{}_meta.cbor", short_id)), meta_shard.to_cbor()).expect("write failed");

            let (shards, arrows) = cft::decompose_depth(short_id, &text, max_depth);
            for s in &shards {
                let s = Shard { tags: { let mut t = s.tags.clone(); t.push("kiro".into()); t.push("chat".into()); t }, ..s.clone() };
                fs::write(dir.join(format!("{}.cbor", s.id)), s.to_cbor()).expect("write failed");
            }
            for a in &arrows {
                fs::write(dir.join(format!("{}.cbor", a.id.replace('→', "_to_"))), a.to_cbor()).expect("write failed");
            }

            new_shards += 1 + shards.len() + arrows.len();
            new_convos += 1;
        }
    }

    let obj = json!({
        "existing": existing.len(),
        "skipped": skipped,
        "new_conversations": new_convos,
        "new_shards": new_shards,
    });
    println!("{}", serde_json::to_string(&obj).unwrap());
}

fn cmd_index(dir: &PathBuf, out: Option<&std::path::Path>) {
    let out_dir = out.map(PathBuf::from).unwrap_or_else(|| dir.join("indexes"));
    fs::create_dir_all(&out_dir).ok();

    // Index maps: key → set of shard IDs
    let mut by_tag: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let mut by_hash: BTreeMap<String, String> = BTreeMap::new();       // cid → shard id
    let mut by_name: BTreeMap<String, String> = BTreeMap::new();       // shard id → file
    let mut by_dir: BTreeMap<String, Vec<String>> = BTreeMap::new();   // working dir → shard ids
    let mut by_word: HashMap<String, Vec<String>> = HashMap::new();    // word → shard ids
    let mut by_git: BTreeMap<String, Vec<String>> = BTreeMap::new();   // git key → shard ids

    let mut total = 0usize;

    let entries: Vec<_> = fs::read_dir(dir)
        .expect("cannot read dir")
        .flatten()
        .filter(|e| e.path().extension().map_or(false, |x| x == "cbor"))
        .collect();

    for entry in &entries {
        let path = entry.path();
        let fname = path.file_name().unwrap().to_string_lossy().to_string();
        let bytes = match fs::read(&path) { Ok(b) => b, Err(_) => continue };
        let shard = match decode_shard(&bytes) { Some(s) => s, None => continue };

        total += 1;

        // name index
        by_name.insert(shard.id.clone(), fname);

        // hash index
        by_hash.insert(shard.cid.clone(), shard.id.clone());

        // tag index
        for tag in &shard.tags {
            by_tag.entry(tag.clone()).or_default().push(shard.id.clone());
        }

        // For meta shards, extract git/directory info
        if shard.tags.contains(&"meta".to_string()) {
            if let Component::KeyValue { ref pairs } = shard.component {
                let mut conv_id = String::new();
                let mut key = String::new();
                for (k, v) in pairs {
                    match k.as_str() {
                        "conversation_id" => conv_id = v.clone(),
                        "key" => key = v.clone(),
                        _ => {}
                    }
                }
                if !key.is_empty() {
                    by_git.entry(key.clone()).or_default().push(shard.id.clone());
                    // Extract directory components
                    for ancestor in std::path::Path::new(&key).ancestors().skip(1) {
                        let d = ancestor.to_string_lossy().to_string();
                        if d.is_empty() || d == "/" { break; }
                        by_dir.entry(d).or_default().push(shard.id.clone());
                    }
                }
            }
        }

        // Word index — extract words from content field of KeyValue shards
        if let Component::KeyValue { ref pairs } = shard.component {
            for (k, v) in pairs {
                if k == "content" || k == "bigrams" || k == "trigrams" { continue; }
                for word in v.split_whitespace() {
                    let w = word.trim_matches(|c: char| !c.is_alphanumeric()).to_lowercase();
                    if w.len() >= 3 {
                        by_word.entry(w).or_default().push(shard.id.clone());
                    }
                }
            }
        } else if let Component::Paragraph { ref text } = shard.component {
            for word in text.split_whitespace() {
                let w = word.trim_matches(|c: char| !c.is_alphanumeric()).to_lowercase();
                if w.len() >= 3 {
                    by_word.entry(w).or_default().push(shard.id.clone());
                }
            }
        }
    }

    // Deduplicate word index entries
    for ids in by_word.values_mut() { ids.sort(); ids.dedup(); }
    for ids in by_tag.values_mut() { ids.sort(); ids.dedup(); }
    for ids in by_dir.values_mut() { ids.sort(); ids.dedup(); }
    for ids in by_git.values_mut() { ids.sort(); ids.dedup(); }

    // Sort word index for deterministic output
    let by_word: BTreeMap<_, _> = by_word.into_iter().collect();

    // Write indexes as JSON
    let write_json = |name: &str, val: &Value| {
        let path = out_dir.join(format!("{}.json", name));
        fs::write(&path, serde_json::to_string(val).unwrap()).expect("write failed");
    };

    write_json("tag_index", &json!(by_tag));
    write_json("hash_index", &json!(by_hash));
    write_json("name_index", &json!(by_name));
    write_json("dir_index", &json!(by_dir));
    write_json("word_index", &json!(by_word));
    write_json("git_index", &json!(by_git));

    let obj = json!({
        "shards_indexed": total,
        "tags": by_tag.len(),
        "hashes": by_hash.len(),
        "names": by_name.len(),
        "directories": by_dir.len(),
        "words": by_word.len(),
        "git_keys": by_git.len(),
        "output": out_dir.to_string_lossy(),
    });
    println!("{}", serde_json::to_string(&obj).unwrap());
}

// ── Perf parquet → DA51 CBOR shards ─────────────────────────────

fn cmd_perf(src: &PathBuf, dir: &PathBuf) {
    fs::create_dir_all(dir).unwrap();
    let mut total = 0usize;
    for entry in fs::read_dir(src).expect("read src dir") {
        let entry = entry.unwrap();
        let path = entry.path();
        if path.extension().map(|e| e == "parquet").unwrap_or(false) {
            let file = File::open(&path).unwrap();
            let reader = SerializedFileReader::new(file).unwrap();
            let mut batch = 0usize;
            let iter = reader.get_row_iter(None).unwrap();
            for row in iter {
                let row: parquet::record::Row = row.unwrap();
                let mut pairs = Vec::new();
                for (name, field) in row.get_column_iter() {
                    let val = match field {
                        Field::Str(s) => s.clone(),
                        Field::Long(n) => n.to_string(),
                        Field::Int(n) => n.to_string(),
                        Field::Float(f) => f.to_string(),
                        Field::Double(f) => f.to_string(),
                        Field::Bytes(b) => String::from_utf8_lossy(b.data()).into_owned(),
                        _ => format!("{:?}", field),
                    };
                    pairs.push((name.to_string(), val));
                }
                let kv = Component::KeyValue { pairs };
                let id = format!(
                    "{}_sample_{}",
                    path.file_stem().unwrap().to_string_lossy(),
                    batch
                );
                let shard = Shard::new(id, kv).with_tags(vec!["perf".into(), "da51".into()]);
                let out_path = dir.join(format!("{}.cbor", shard.id));
                fs::write(&out_path, shard.to_cbor()).unwrap();
                batch += 1;
            }
            eprintln!("{}: {} samples → DA51 shards", path.display(), batch);
            total += batch;
        }
    }
    eprintln!("total: {} DA51 CBOR shards in {}", total, dir.display());
}

// ── DA51 CBOR shards → Agda module ─────────────────────────────

fn cmd_agda(dir: &PathBuf, out: &PathBuf, module: &str) {
    let mut agda = String::new();
    agda.push_str(&format!("-- Auto-generated by erdfa-cli agda from {}\n", dir.display()));
    agda.push_str(&format!("module {} where\n\n", module));
    agda.push_str("open import Agda.Builtin.Nat\n");
    agda.push_str("open import Agda.Builtin.List\n");
    agda.push_str("open import Agda.Builtin.String\n");
    agda.push_str("open import Agda.Builtin.Bool\n\n");
    agda.push_str("data CborVal : Set where\n");
    agda.push_str("  cnat  : Nat → CborVal\n");
    agda.push_str("  ctext : String → CborVal\n");
    agda.push_str("  cpair : String → CborVal → CborVal\n");
    agda.push_str("  clist : List CborVal → CborVal\n");
    agda.push_str("  ctag  : Nat → CborVal → CborVal\n\n");

    let mut shards: Vec<PathBuf> = fs::read_dir(dir)
        .expect("read dir")
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().map(|e| e == "cbor").unwrap_or(false))
        .collect();
    shards.sort();

    for (i, path) in shards.iter().enumerate() {
        let raw = fs::read(path).unwrap();
        let cbor_data = if raw.len() > 2 && raw[0] == 0xda && raw[1] == 0x51 {
            &raw[2..]
        } else {
            &raw
        };
        // Decode the CBOR tag wrapper
        let val: ciborium::Value = match ciborium::from_reader(cbor_data) {
            Ok(v) => v,
            Err(_) => continue,
        };
        let _name = path.file_stem().unwrap().to_string_lossy().replace('-', "_").replace('.', "_");
        agda.push_str(&format!("shard-{} : CborVal\n", i));
        agda.push_str(&format!("shard-{} = {}\n\n", i, cbor_to_agda(&val, 0)));
    }

    // Collect all into a list
    agda.push_str("shards : List CborVal\nshards =\n");
    for i in 0..shards.len() {
        if i == 0 {
            agda.push_str(&format!("  shard-{}\n", i));
        } else {
            agda.push_str(&format!("  ∷ shard-{}\n", i));
        }
    }
    if shards.is_empty() {
        agda.push_str("  []\n");
    } else {
        agda.push_str("  ∷ []\n");
    }

    fs::write(out, &agda).unwrap();
    eprintln!("wrote {} ({} shards)", out.display(), shards.len());
}

fn cbor_to_agda(val: &ciborium::Value, depth: usize) -> String {
    use ciborium::Value::*;
    let _ind = "  ".repeat(depth);
    match val {
        Integer(n) => {
            let n128: i128 = (*n).into();
            if n128 >= 0 { format!("(cnat {})", n128) } else { format!("(cnat 0)") }
        }
        Text(s) => {
            let escaped = s.replace('\\', "\\\\").replace('"', "\\\"");
            format!("(ctext \"{}\")", escaped)
        }
        Bytes(bs) => {
            let elems: Vec<String> = bs.iter().map(|b| format!("(cnat {})", b)).collect();
            if elems.is_empty() {
                "(clist [])".into()
            } else {
                format!("(clist ({} ∷ []))", elems.join(" ∷ "))
            }
        }
        Array(xs) => {
            if xs.is_empty() {
                "(clist [])".into()
            } else {
                let items: Vec<String> = xs.iter().map(|x| cbor_to_agda(x, depth + 1)).collect();
                format!("(clist\n{}({} ∷ []))", "  ".repeat(depth + 1), items.join(" ∷ "))
            }
        }
        Map(kvs) => {
            if kvs.is_empty() {
                "(clist [])".into()
            } else {
                let items: Vec<String> = kvs.iter().map(|(k, v)| {
                    let key = match k {
                        Text(s) => s.clone(),
                        Integer(n) => { let n128: i128 = (*n).into(); n128.to_string() }
                        _ => format!("{:?}", k),
                    };
                    format!("(cpair \"{}\" {})", key, cbor_to_agda(v, depth + 2))
                }).collect();
                format!("(clist\n{}({} ∷ []))", "  ".repeat(depth + 1), items.join(" ∷ "))
            }
        }
        Tag(t, inner) => {
            format!("(ctag {} {})", t, cbor_to_agda(inner, depth + 1))
        }
        Bool(b) => format!("(ctext \"{}\")", b),
        Null => "(ctext \"null\")".into(),
        Float(f) => format!("(cnat {})", *f as u64),
        _ => "(ctext \"?\")".into(),
    }
}

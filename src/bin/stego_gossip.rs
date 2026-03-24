/// stego-gossip: UDP gossip layer that carries erdfa shards between peers.
///
/// Each peer:
///   1. Listens on UDP for incoming shard fragments
///   2. Gossips received fragments to known peers
///   3. Posts complete shards to local Solana validator as memo txns
///   4. Periodically announces its shard inventory
///
/// Wire format (UDP datagram, max 1200 bytes):
///   magic: "ERDG" (4)
///   msg_type: u8 (0=shard, 1=inventory, 2=request, 3=peer_announce)
///   sender_id: [u8; 8] (truncated sha256 of listen addr)
///   seq: u32 (monotonic sequence number)
///   payload: variable
///
/// Shard payload:
///   shard_hash: [u8; 16] (truncated sha256 of full shard)
///   chunk_idx: u16
///   chunk_total: u16
///   data: [u8; ..] (up to 1164 bytes)

use std::collections::{HashMap, HashSet};
use std::net::{SocketAddr, UdpSocket};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

#[cfg(feature = "native")]
use clap::Parser;

const MAGIC: &[u8; 4] = b"ERDG";
const MAX_DGRAM: usize = 1200;
const HEADER_LEN: usize = 17; // 4 magic + 1 type + 8 sender + 4 seq
const SHARD_HEADER: usize = 20; // 16 hash + 2 idx + 2 total
const MAX_CHUNK_DATA: usize = MAX_DGRAM - HEADER_LEN - SHARD_HEADER;
const GOSSIP_FANOUT: usize = 3;
const INVENTORY_INTERVAL: Duration = Duration::from_secs(30);
const SOLANA_POST_INTERVAL: Duration = Duration::from_secs(5);

#[cfg(feature = "native")]
#[derive(Parser)]
#[command(name = "stego-gossip", about = "eRDFa stego P2P gossip layer")]
struct Args {
    /// UDP listen address
    #[arg(long, default_value = "0.0.0.0:7700")]
    listen: String,
    /// Solana RPC URL
    #[arg(long, default_value = "http://127.0.0.1:8899")]
    rpc: String,
    /// Peer addresses (comma-separated)
    #[arg(long, value_delimiter = ',')]
    peers: Vec<String>,
    /// Peers file (JSON array of "host:port")
    #[arg(long)]
    peers_file: Option<String>,
    /// Directory to watch for new shards to gossip
    #[arg(long)]
    watch: Option<String>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
enum MsgType {
    Shard = 0,
    Inventory = 1,
    Request = 2,
    PeerAnnounce = 3,
}

impl MsgType {
    fn from_u8(v: u8) -> Option<Self> {
        match v {
            0 => Some(Self::Shard),
            1 => Some(Self::Inventory),
            2 => Some(Self::Request),
            3 => Some(Self::PeerAnnounce),
            _ => None,
        }
    }
}

/// Reassembly buffer for chunked shards
struct ShardAssembly {
    chunks: HashMap<u16, Vec<u8>>,
    total: u16,
    first_seen: Instant,
}

/// Shared gossip state
struct GossipState {
    /// Our sender ID (8 bytes)
    sender_id: [u8; 8],
    /// Known peers
    peers: HashSet<SocketAddr>,
    /// Complete shards we have (hash → data)
    complete: HashMap<[u8; 16], Vec<u8>>,
    /// Shards being reassembled
    assembling: HashMap<[u8; 16], ShardAssembly>,
    /// Shards pending Solana post
    pending_post: Vec<([u8; 16], Vec<u8>)>,
    /// Monotonic sequence
    seq: u32,
}

impl GossipState {
    fn new(listen_addr: &str) -> Self {
        let hash = sha256(listen_addr.as_bytes());
        let mut sender_id = [0u8; 8];
        sender_id.copy_from_slice(&hash[..8]);
        Self {
            sender_id,
            peers: HashSet::new(),
            complete: HashMap::new(),
            assembling: HashMap::new(),
            pending_post: Vec::new(),
            seq: 0,
        }
    }

    fn next_seq(&mut self) -> u32 {
        self.seq += 1;
        self.seq
    }
}

fn sha256(data: &[u8]) -> [u8; 32] {
    use sha2::{Sha256, Digest};
    let mut h = Sha256::new();
    h.update(data);
    h.finalize().into()
}

fn shard_hash(data: &[u8]) -> [u8; 16] {
    let h = sha256(data);
    let mut out = [0u8; 16];
    out.copy_from_slice(&h[..16]);
    out
}

/// Build a wire message
fn build_msg(state: &mut GossipState, msg_type: MsgType, payload: &[u8]) -> Vec<u8> {
    let seq = state.next_seq();
    let mut buf = Vec::with_capacity(HEADER_LEN + payload.len());
    buf.extend_from_slice(MAGIC);
    buf.push(msg_type as u8);
    buf.extend_from_slice(&state.sender_id);
    buf.extend_from_slice(&seq.to_be_bytes());
    buf.extend_from_slice(payload);
    buf
}

/// Build shard chunk messages
fn build_shard_chunks(state: &mut GossipState, data: &[u8]) -> Vec<Vec<u8>> {
    let hash = shard_hash(data);
    let total = ((data.len() + MAX_CHUNK_DATA - 1) / MAX_CHUNK_DATA) as u16;
    (0..total).map(|i| {
        let start = i as usize * MAX_CHUNK_DATA;
        let end = (start + MAX_CHUNK_DATA).min(data.len());
        let mut payload = Vec::with_capacity(SHARD_HEADER + end - start);
        payload.extend_from_slice(&hash);
        payload.extend_from_slice(&i.to_be_bytes());
        payload.extend_from_slice(&total.to_be_bytes());
        payload.extend_from_slice(&data[start..end]);
        build_msg(state, MsgType::Shard, &payload)
    }).collect()
}

/// Build inventory message (list of shard hashes we have)
fn build_inventory(state: &mut GossipState) -> Vec<u8> {
    let hashes: Vec<u8> = state.complete.keys().flat_map(|h| h.iter().copied()).collect();
    build_msg(state, MsgType::Inventory, &hashes)
}

/// Build request message for a shard hash
fn build_request(state: &mut GossipState, hash: &[u8; 16]) -> Vec<u8> {
    build_msg(state, MsgType::Request, hash)
}

/// Build peer announce
fn build_peer_announce(state: &mut GossipState, listen_addr: &str) -> Vec<u8> {
    build_msg(state, MsgType::PeerAnnounce, listen_addr.as_bytes())
}

/// Handle incoming datagram
fn handle_msg(
    state: &Arc<Mutex<GossipState>>,
    sock: &UdpSocket,
    buf: &[u8],
    from: SocketAddr,
) {
    if buf.len() < HEADER_LEN || &buf[0..4] != MAGIC { return; }
    let msg_type = match MsgType::from_u8(buf[4]) { Some(t) => t, None => return };
    let payload = &buf[HEADER_LEN..];
    let mut st = state.lock().unwrap();

    // Add sender as peer
    st.peers.insert(from);

    match msg_type {
        MsgType::Shard => {
            if payload.len() < SHARD_HEADER { return; }
            let mut hash = [0u8; 16];
            hash.copy_from_slice(&payload[..16]);
            let idx = u16::from_be_bytes([payload[16], payload[17]]);
            let total = u16::from_be_bytes([payload[18], payload[19]]);
            let chunk_data = &payload[20..];

            // Already have this shard?
            if st.complete.contains_key(&hash) { return; }

            let asm = st.assembling.entry(hash).or_insert_with(|| ShardAssembly {
                chunks: HashMap::new(),
                total,
                first_seen: Instant::now(),
            });
            asm.chunks.insert(idx, chunk_data.to_vec());

            // Check if complete
            if asm.chunks.len() == asm.total as usize {
                let mut data = Vec::new();
                for i in 0..asm.total {
                    if let Some(c) = asm.chunks.get(&i) {
                        data.extend_from_slice(c);
                    }
                }
                // Verify hash
                if shard_hash(&data) == hash {
                    eprintln!("✓ shard complete: {} ({} bytes, {} chunks)",
                        hex::encode(&hash[..4]), data.len(), total);
                    st.pending_post.push((hash, data.clone()));
                    st.complete.insert(hash, data);
                }
                st.assembling.remove(&hash);

                // Gossip to peers (forward complete shard)
                let peers: Vec<SocketAddr> = st.peers.iter()
                    .filter(|p| **p != from)
                    .take(GOSSIP_FANOUT)
                    .copied()
                    .collect();
                // Re-chunk and forward (drop lock first)
                drop(st);
                let data_ref = state.lock().unwrap().complete.get(&hash).cloned();
                if let Some(data) = data_ref {
                    let chunks = build_shard_chunks(&mut state.lock().unwrap(), &data);
                    for peer in peers {
                        for chunk in &chunks {
                            let _ = sock.send_to(chunk, peer);
                        }
                    }
                }
            }
        }
        MsgType::Inventory => {
            // Parse list of 16-byte hashes, request any we don't have
            let mut requests = Vec::new();
            for chunk in payload.chunks_exact(16) {
                let mut hash = [0u8; 16];
                hash.copy_from_slice(chunk);
                if !st.complete.contains_key(&hash) {
                    requests.push(build_request(&mut st, &hash));
                }
            }
            drop(st);
            for req in requests {
                let _ = sock.send_to(&req, from);
            }
        }
        MsgType::Request => {
            if payload.len() < 16 { return; }
            let mut hash = [0u8; 16];
            hash.copy_from_slice(&payload[..16]);
            if let Some(data) = st.complete.get(&hash).cloned() {
                let chunks = build_shard_chunks(&mut st, &data);
                drop(st);
                for chunk in chunks {
                    let _ = sock.send_to(&chunk, from);
                }
            }
        }
        MsgType::PeerAnnounce => {
            if let Ok(addr_str) = std::str::from_utf8(payload) {
                if let Ok(addr) = addr_str.parse::<SocketAddr>() {
                    st.peers.insert(addr);
                    eprintln!("+ peer: {}", addr);
                }
            }
        }
    }
}

/// Post pending shards to Solana as memo transactions
fn post_to_solana(state: &Arc<Mutex<GossipState>>, rpc: &str) {
    let pending: Vec<([u8; 16], Vec<u8>)> = {
        let mut st = state.lock().unwrap();
        std::mem::take(&mut st.pending_post)
    };
    for (hash, data) in pending {
        // Use erdfa-publish SolanaMemo encoding
        let memo = erdfa_publish::stego::SolanaMemo;
        let encoded = <erdfa_publish::stego::SolanaMemo as erdfa_publish::stego::StegoPlugin>::encode(&memo, &data);
        let memo_str = String::from_utf8_lossy(&encoded);

        // Post via solana CLI (subprocess — avoids SDK dep)
        // Self-transfer with memo: send dust to ourselves
        let pubkey = std::process::Command::new("solana")
            .args(["--url", rpc, "address"])
            .output()
            .ok()
            .and_then(|o| String::from_utf8(o.stdout).ok())
            .unwrap_or_default()
            .trim()
            .to_string();

        if pubkey.is_empty() {
            eprintln!("✗ cannot get solana pubkey");
            continue;
        }

        let status = std::process::Command::new("solana")
            .args(["transfer", "--allow-unfunded-recipient",
                   "--with-memo", &memo_str,
                   "--url", rpc,
                   &pubkey, // self-transfer
                   "0.000001"])
            .output();

        match status {
            Ok(out) if out.status.success() => {
                eprintln!("◎ posted shard {} to Solana ({} bytes)",
                    hex::encode(&hash[..4]), data.len());
            }
            Ok(out) => {
                eprintln!("✗ solana post failed: {}",
                    String::from_utf8_lossy(&out.stderr).lines().next().unwrap_or(""));
            }
            Err(e) => eprintln!("✗ solana not available: {}", e),
        }
    }
}

/// Watch a directory for new .cbor files to gossip
fn scan_watch_dir(state: &Arc<Mutex<GossipState>>, sock: &UdpSocket, dir: &str) {
    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return,
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.extension().map(|e| e == "cbor").unwrap_or(false) { continue; }
        let data = match std::fs::read(&path) {
            Ok(d) => d,
            Err(_) => continue,
        };
        let hash = shard_hash(&data);
        let mut st = state.lock().unwrap();
        if st.complete.contains_key(&hash) { continue; }

        eprintln!("→ ingesting {} ({} bytes)", path.display(), data.len());
        st.complete.insert(hash, data.clone());
        st.pending_post.push((hash, data.clone()));

        // Gossip to all peers
        let peers: Vec<SocketAddr> = st.peers.iter().copied().collect();
        let chunks = build_shard_chunks(&mut st, &data);
        drop(st);
        for peer in peers {
            for chunk in &chunks {
                let _ = sock.send_to(chunk, peer);
            }
        }

        // Remove ingested file
        let _ = std::fs::rename(&path, path.with_extension("cbor.sent"));
    }
}

#[cfg(feature = "native")]
fn main() {
    let args = Args::parse();

    let sock = UdpSocket::bind(&args.listen).expect("bind UDP");
    sock.set_read_timeout(Some(Duration::from_millis(100))).ok();
    eprintln!("◎ stego-gossip listening on {}", args.listen);

    let state = Arc::new(Mutex::new(GossipState::new(&args.listen)));

    // Add initial peers
    let mut all_peers: Vec<String> = args.peers;
    if let Some(ref pf) = args.peers_file {
        if let Ok(data) = std::fs::read_to_string(pf) {
            if let Ok(list) = serde_json::from_str::<Vec<String>>(&data) {
                all_peers.extend(list);
            }
        }
    }
    {
        let mut st = state.lock().unwrap();
        for p in &all_peers {
            if let Ok(addr) = p.parse::<SocketAddr>() {
                st.peers.insert(addr);
                eprintln!("+ peer: {}", addr);
            }
        }
    }

    // Announce ourselves to peers
    {
        let mut st = state.lock().unwrap();
        let announce = build_peer_announce(&mut st, &args.listen);
        let peers: Vec<SocketAddr> = st.peers.iter().copied().collect();
        drop(st);
        for peer in peers {
            let _ = sock.send_to(&announce, peer);
        }
    }

    let mut last_inventory = Instant::now();
    let mut last_solana_post = Instant::now();
    let mut last_watch_scan = Instant::now();
    let mut buf = [0u8; MAX_DGRAM];

    eprintln!("◎ rpc: {}", args.rpc);
    if let Some(ref w) = args.watch {
        eprintln!("◎ watching: {}", w);
    }

    loop {
        // Receive
        match sock.recv_from(&mut buf) {
            Ok((n, from)) => handle_msg(&state, &sock, &buf[..n], from),
            Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {}
            Err(e) => eprintln!("recv error: {}", e),
        }

        let now = Instant::now();

        // Periodic inventory broadcast
        if now.duration_since(last_inventory) >= INVENTORY_INTERVAL {
            let inv = build_inventory(&mut state.lock().unwrap());
            let peers: Vec<SocketAddr> = state.lock().unwrap().peers.iter().copied().collect();
            for peer in peers {
                let _ = sock.send_to(&inv, peer);
            }
            let st = state.lock().unwrap();
            eprintln!("◎ inventory: {} shards, {} peers",
                st.complete.len(), st.peers.len());
            last_inventory = now;
        }

        // Post to Solana
        if now.duration_since(last_solana_post) >= SOLANA_POST_INTERVAL {
            post_to_solana(&state, &args.rpc);
            last_solana_post = now;
        }

        // Watch directory
        if let Some(ref w) = args.watch {
            if now.duration_since(last_watch_scan) >= Duration::from_secs(2) {
                scan_watch_dir(&state, &sock, w);
                last_watch_scan = now;
            }
        }

        // Expire stale assemblies (>60s)
        {
            let mut st = state.lock().unwrap();
            st.assembling.retain(|_, asm| asm.first_seen.elapsed() < Duration::from_secs(60));
        }
    }
}

#[cfg(not(feature = "native"))]
fn main() {
    eprintln!("stego-gossip requires native feature");
}

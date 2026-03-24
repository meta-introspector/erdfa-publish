/// erdfa-mixer: ZKP-driven deposit/withdraw pool with stego-gossip feedback.
///
/// Commands:
///   deposit  — Generate note, deposit commitment on-chain, save note file
///   withdraw — Load note, generate Merkle proof, send via stego-gossip
///   status   — Show pool state (deposits, withdrawals, pending)
///   relay    — Listen for ERDW gossip messages, verify and execute withdrawals
///
/// The key privacy property: deposits go on-chain, but withdrawal proofs
/// travel through the stego-gossip P2P layer — no direct on-chain link
/// between depositor and recipient.

use std::path::PathBuf;
use std::process::Command;

#[cfg(feature = "native")]
use clap::{Parser, Subcommand};

#[cfg(feature = "native")]
#[derive(Parser)]
#[command(name = "erdfa-mixer")]
struct Args {
    /// Solana RPC URL
    #[arg(long, default_value = "http://127.0.0.1:8899")]
    rpc: String,
    /// Pool state file
    #[arg(long, default_value = "mixer-pool.json")]
    pool: PathBuf,
    #[command(subcommand)]
    cmd: Cmd,
}

#[cfg(feature = "native")]
#[derive(Subcommand)]
enum Cmd {
    /// Create a deposit: generates note, posts commitment on-chain
    Deposit {
        /// Denomination in SOL
        #[arg(long, default_value = "1.0")]
        amount: f64,
        /// Save note to this file (keep secret!)
        #[arg(long, default_value = "note.cbor")]
        note_file: PathBuf,
    },
    /// Withdraw using a note: generates Merkle proof, sends via gossip
    Withdraw {
        /// Note file from deposit step
        #[arg(long)]
        note_file: PathBuf,
        /// Recipient Solana address
        #[arg(long)]
        recipient: String,
        /// Gossip peer to send withdrawal proof to
        #[arg(long, default_value = "127.0.0.1:7700")]
        gossip_peer: String,
    },
    /// Show pool status
    Status,
    /// Listen for ERDW withdrawal proofs on gossip, verify and execute
    Relay {
        /// Gossip listen port
        #[arg(long, default_value = "7701")]
        port: u16,
    },
}

#[cfg(feature = "native")]
fn load_pool(path: &PathBuf) -> erdfa_publish::mixer::MixerPool {
    if path.exists() {
        let data = std::fs::read(path).expect("read pool");
        serde_json::from_slice(&data).expect("parse pool")
    } else {
        let addr = get_address("http://127.0.0.1:8899");
        erdfa_publish::mixer::MixerPool::new(1_000_000_000, addr)
    }
}

#[cfg(feature = "native")]
fn save_pool(path: &PathBuf, pool: &erdfa_publish::mixer::MixerPool) {
    std::fs::write(path, serde_json::to_string_pretty(pool).unwrap()).expect("save pool");
}

#[cfg(feature = "native")]
fn get_address(rpc: &str) -> String {
    Command::new("solana")
        .args(["--url", rpc, "address"])
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .unwrap_or_default()
        .trim()
        .to_string()
}

#[cfg(feature = "native")]
fn main() {
    use erdfa_publish::mixer::*;

    let args = Args::parse();

    match args.cmd {
        Cmd::Deposit { amount, note_file } => {
            let lamports = (amount * 1_000_000_000.0) as u64;
            let note = Note::generate(lamports);
            let commitment = note.commitment();

            // Save note (SECRET — user must protect this)
            std::fs::write(&note_file, note.to_cbor()).expect("save note");
            eprintln!("◎ Note saved to {} (KEEP SECRET!)", note_file.display());

            // Post commitment on-chain as memo
            let commitment_hex = hex::encode(&commitment.0);
            let memo = format!("erdfa-mixer:deposit:{}", commitment_hex);
            eprintln!("  Commitment: {}", commitment_hex);

            let output = Command::new("solana")
                .args(["--url", &args.rpc, "transfer", "--allow-unfunded-recipient",
                       &get_address(&args.rpc), "0.000000001",
                       "--with-memo", &memo])
                .output()
                .expect("solana transfer");

            if output.status.success() {
                let sig = String::from_utf8_lossy(&output.stdout);
                eprintln!("  ✓ Deposit tx: {}", sig.trim());

                // Record in pool
                let mut pool = load_pool(&args.pool);
                pool.denomination = lamports;
                let idx = pool.deposit(commitment);
                save_pool(&args.pool, &pool);
                eprintln!("  ✓ Pool index: {} (anonymity set: {})", idx, pool.commitments.len());
            } else {
                eprintln!("  ✗ Deposit failed: {}", String::from_utf8_lossy(&output.stderr));
            }
        }

        Cmd::Withdraw { note_file, recipient, gossip_peer } => {
            let note_data = std::fs::read(&note_file).expect("read note");
            let note = Note::from_cbor(&note_data).expect("parse note");
            let pool = load_pool(&args.pool);

            eprintln!("◎ Creating withdrawal proof");
            eprintln!("  Recipient: {}", recipient);
            eprintln!("  Anonymity set: {} deposits", pool.commitments.len());

            match pool.create_withdrawal(&note, &recipient) {
                Some(wp) => {
                    // Encode for gossip transport
                    let gossip_data = MixerPool::encode_withdrawal_for_gossip(&wp);
                    eprintln!("  ✓ Proof: {} bytes (ERDW format)", gossip_data.len());

                    // Send via UDP to gossip peer
                    let sock = std::net::UdpSocket::bind("0.0.0.0:0").expect("bind");
                    sock.send_to(&gossip_data, &gossip_peer).expect("send");
                    eprintln!("  ✓ Sent to gossip peer {}", gossip_peer);
                    eprintln!("  Nullifier hash: {}", hex::encode(&wp.nullifier_hash.0));
                    eprintln!("  ◎ Withdrawal proof in transit — no on-chain link to deposit");
                }
                None => {
                    eprintln!("  ✗ Cannot create proof (note not in pool or already spent)");
                }
            }
        }

        Cmd::Status => {
            let pool = load_pool(&args.pool);
            let status = pool.status();
            eprintln!("◎ Mixer Pool Status");
            eprintln!("  Pool address:  {}", status.pool_address);
            eprintln!("  Denomination:  {} SOL", status.denomination as f64 / 1e9);
            eprintln!("  Deposits:      {}", status.deposits);
            eprintln!("  Withdrawals:   {}", status.withdrawals);
            eprintln!("  Pending:       {}", status.pending);
        }

        Cmd::Relay { port } => {
            let mut pool = load_pool(&args.pool);
            let addr = format!("0.0.0.0:{}", port);
            let sock = std::net::UdpSocket::bind(&addr).expect("bind");
            eprintln!("◎ Mixer relay listening on {}", addr);
            eprintln!("  Waiting for ERDW withdrawal proofs via gossip...");

            let mut buf = [0u8; 65536];
            loop {
                let (len, src) = sock.recv_from(&mut buf).expect("recv");
                let data = &buf[..len];

                if let Some(wp) = MixerPool::decode_withdrawal_from_gossip(data) {
                    eprintln!("  ← ERDW from {} ({} bytes)", src, len);
                    eprintln!("    Recipient: {}", wp.recipient);
                    eprintln!("    Nullifier: {}", hex::encode(&wp.nullifier_hash.0));

                    if pool.verify_and_withdraw(&wp) {
                        eprintln!("    ✓ Proof valid — executing withdrawal");
                        let sol = wp.amount as f64 / 1e9;
                        let output = Command::new("solana")
                            .args(["--url", &args.rpc, "transfer",
                                   "--allow-unfunded-recipient",
                                   &wp.recipient, &format!("{}", sol)])
                            .output()
                            .expect("solana transfer");

                        if output.status.success() {
                            eprintln!("    ✓ Withdrawal tx: {}", String::from_utf8_lossy(&output.stdout).trim());
                            save_pool(&args.pool, &pool);
                        } else {
                            eprintln!("    ✗ Withdrawal tx failed: {}", String::from_utf8_lossy(&output.stderr));
                        }
                    } else {
                        eprintln!("    ✗ Proof invalid or nullifier already spent");
                    }
                }
            }
        }
    }
}

#[cfg(not(feature = "native"))]
fn main() { eprintln!("requires native feature"); }

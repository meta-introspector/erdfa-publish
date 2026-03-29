//! ActivityPub federation for SOLFUNMEME witnesses.
//!
//! Wraps zkTLS witnesses as ActivityPub `Note` objects with DASL/CBOR
//! envelopes and IPFS content-addressing. Federated via mesh peers.
//!
//! Based on ActivityStreams 2.0 vocabulary.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// ActivityPub actor (our witness node)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Actor {
    #[serde(rename = "@context")]
    pub context: String,
    pub id: String,
    #[serde(rename = "type")]
    pub kind: String,
    pub name: String,
    pub inbox: String,
    pub outbox: String,
    #[serde(rename = "publicKey")]
    pub public_key: ActorKey,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActorKey {
    pub id: String,
    pub owner: String,
    #[serde(rename = "publicKeyPem")]
    pub public_key_pem: String,
}

/// ActivityPub Note wrapping a witness
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WitnessNote {
    #[serde(rename = "@context")]
    pub context: Vec<String>,
    pub id: String,
    #[serde(rename = "type")]
    pub kind: String,
    #[serde(rename = "attributedTo")]
    pub attributed_to: String,
    pub content: String,
    pub published: String,
    pub url: String,
    /// IPFS CID of the witnessed content
    #[serde(rename = "ipfs:cid")]
    pub ipfs_cid: String,
    /// DASL address
    #[serde(rename = "dasl:addr")]
    pub dasl_addr: String,
    /// Orbifold coordinates on Monster torus
    #[serde(rename = "sheaf:orbifold")]
    pub orbifold: String,
    /// Merkle root of the witness
    #[serde(rename = "erdfa:merkle")]
    pub merkle_root: String,
}

/// Create a WitnessNote from witnessed data
pub fn witness_to_note(
    actor_id: &str,
    url: &str,
    content_hash: &str,
    ipfs_cid: &str,
    dasl_addr: &str,
    orbifold: (u8, u8, u8),
    timestamp: &str,
) -> WitnessNote {
    let merkle = hex::encode(Sha256::digest(format!("{}:{}:{}", url, content_hash, timestamp).as_bytes()));
    let note_id = format!("{}/witnesses/{}", actor_id, &merkle[..16]);

    WitnessNote {
        context: vec![
            "https://www.w3.org/ns/activitystreams".into(),
            "https://solfunmeme.com/ns/erdfa".into(),
        ],
        id: note_id,
        kind: "Note".into(),
        attributed_to: actor_id.into(),
        content: format!("🔒 Witnessed: {} [{}]", url, &content_hash[..16]),
        published: timestamp.into(),
        url: url.into(),
        ipfs_cid: ipfs_cid.into(),
        dasl_addr: dasl_addr.into(),
        orbifold: format!("({},{},{})", orbifold.0, orbifold.1, orbifold.2),
        merkle_root: merkle,
    }
}

/// Create an Actor for this witness node
pub fn create_actor(base_url: &str, name: &str, pubkey_pem: &str) -> Actor {
    Actor {
        context: "https://www.w3.org/ns/activitystreams".into(),
        id: format!("{}/actor", base_url),
        kind: "Service".into(),
        name: name.into(),
        inbox: format!("{}/inbox", base_url),
        outbox: format!("{}/outbox", base_url),
        public_key: ActorKey {
            id: format!("{}/actor#main-key", base_url),
            owner: format!("{}/actor", base_url),
            public_key_pem: pubkey_pem.into(),
        },
    }
}

/// Outbox collection (list of witnessed notes)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Outbox {
    #[serde(rename = "@context")]
    pub context: String,
    pub id: String,
    #[serde(rename = "type")]
    pub kind: String,
    #[serde(rename = "totalItems")]
    pub total_items: usize,
    #[serde(rename = "orderedItems")]
    pub ordered_items: Vec<WitnessNote>,
}

pub fn create_outbox(base_url: &str, notes: Vec<WitnessNote>) -> Outbox {
    Outbox {
        context: "https://www.w3.org/ns/activitystreams".into(),
        id: format!("{}/outbox", base_url),
        kind: "OrderedCollection".into(),
        total_items: notes.len(),
        ordered_items: notes,
    }
}

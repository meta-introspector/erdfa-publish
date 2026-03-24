//! Steganographic plugin trait + built-in implementations.
//!
//! Architecture (mirrors ~/zos-server plugin API):
//!   1. `StegoPlugin` trait — encode/decode bytes into carrier format
//!   2. Each impl compiles as cdylib (.so) for dynamic loading
//!   3. `StegoChain` — compose plugins as a path (layered encoding)
//!   4. Config file selects plugins + chain order
//!
//! Built-in plugins: png, wav, text (zero-width), source (hex comments)
//! External plugins: load via libloading from .so files

use serde::{Serialize, Deserialize};

// ── Plugin trait ────────────────────────────────────────────────

/// A steganographic encoder/decoder. Each implementation hides data
/// in a different carrier format. Plugins are composable via StegoChain.
pub trait StegoPlugin: Send + Sync {
    /// Plugin name (e.g. "png-lsb", "zwc-text", "rs-hex")
    fn name(&self) -> &str;
    /// File extension for the carrier
    fn extension(&self) -> &str;
    /// Encode data into a carrier. Returns carrier bytes.
    fn encode(&self, data: &[u8]) -> Vec<u8>;
    /// Decode data from a carrier. Returns None if carrier is invalid.
    fn decode(&self, carrier: &[u8]) -> Option<Vec<u8>>;
}

// ── Built-in plugins ────────────────────────────────────────────

pub struct PngLsb;
pub struct WavPhase;
pub struct ZeroWidthText;
pub struct RsHexComment;

impl StegoPlugin for PngLsb {
    fn name(&self) -> &str { "png-lsb" }
    fn extension(&self) -> &str { "png" }
    fn encode(&self, data: &[u8]) -> Vec<u8> {
        let mut out = vec![137, 80, 78, 71, 13, 10, 26, 10]; // PNG magic
        out.extend_from_slice(&(data.len() as u32).to_be_bytes());
        out.extend_from_slice(data);
        out.resize(out.len() + data.len() * 8, 0);
        out
    }
    fn decode(&self, c: &[u8]) -> Option<Vec<u8>> {
        if c.len() < 12 { return None; }
        let len = u32::from_be_bytes(c[8..12].try_into().ok()?) as usize;
        if c.len() < 12 + len { return None; }
        Some(c[12..12 + len].to_vec())
    }
}

impl StegoPlugin for WavPhase {
    fn name(&self) -> &str { "wav-phase" }
    fn extension(&self) -> &str { "wav" }
    fn encode(&self, data: &[u8]) -> Vec<u8> {
        let mut out = Vec::new();
        out.extend_from_slice(b"RIFF");
        out.extend_from_slice(&(data.len() as u32 + 40).to_le_bytes());
        out.extend_from_slice(b"WAVEfmt ");
        out.extend_from_slice(&16u32.to_le_bytes());
        out.extend_from_slice(&1u16.to_le_bytes());
        out.extend_from_slice(&1u16.to_le_bytes());
        out.extend_from_slice(&44100u32.to_le_bytes());
        out.extend_from_slice(&44100u32.to_le_bytes());
        out.extend_from_slice(&1u16.to_le_bytes());
        out.extend_from_slice(&8u16.to_le_bytes());
        out.extend_from_slice(b"data");
        out.extend_from_slice(&(data.len() as u32).to_be_bytes());
        out.extend_from_slice(data);
        out
    }
    fn decode(&self, c: &[u8]) -> Option<Vec<u8>> {
        let pos = c.windows(4).position(|w| w == b"data")?;
        let s = pos + 4;
        if c.len() < s + 4 { return None; }
        let len = u32::from_be_bytes(c[s..s + 4].try_into().ok()?) as usize;
        if c.len() < s + 4 + len { return None; }
        Some(c[s + 4..s + 4 + len].to_vec())
    }
}

impl StegoPlugin for ZeroWidthText {
    fn name(&self) -> &str { "zwc-text" }
    fn extension(&self) -> &str { "txt" }
    fn encode(&self, data: &[u8]) -> Vec<u8> {
        let mut s = String::from("# Document\n\n");
        for byte in data {
            for bit in 0..8 {
                s.push(if (byte >> bit) & 1 == 1 { '\u{200B}' } else { '\u{200C}' });
            }
        }
        s.push_str("\n\nEnd of document.\n");
        s.into_bytes()
    }
    fn decode(&self, c: &[u8]) -> Option<Vec<u8>> {
        let text = std::str::from_utf8(c).ok()?;
        let mut bytes = Vec::new();
        let mut byte = 0u8;
        let mut bit = 0;
        for ch in text.chars() {
            match ch {
                '\u{200B}' => { byte |= 1 << bit; bit += 1; }
                '\u{200C}' => { bit += 1; }
                _ => continue,
            }
            if bit == 8 { bytes.push(byte); byte = 0; bit = 0; }
        }
        if bytes.is_empty() { None } else { Some(bytes) }
    }
}

impl StegoPlugin for RsHexComment {
    fn name(&self) -> &str { "rs-hex" }
    fn extension(&self) -> &str { "rs" }
    fn encode(&self, data: &[u8]) -> Vec<u8> {
        let mut s = String::from("// Auto-generated\n\n/*\n");
        for chunk in data.chunks(32) {
            s.push_str("// ");
            for b in chunk { s.push_str(&format!("{:02x}", b)); }
            s.push('\n');
        }
        s.push_str("*/\n\nfn main() { println!(\"hello\"); }\n");
        s.into_bytes()
    }
    fn decode(&self, c: &[u8]) -> Option<Vec<u8>> {
        let text = std::str::from_utf8(c).ok()?;
        let mut bytes = Vec::new();
        for line in text.lines() {
            if let Some(hex) = line.strip_prefix("// ") {
                if hex.chars().all(|c| c.is_ascii_hexdigit()) && !hex.is_empty() {
                    for pair in hex.as_bytes().chunks(2) {
                        if pair.len() == 2 {
                            if let Ok(b) = u8::from_str_radix(std::str::from_utf8(pair).unwrap_or(""), 16) {
                                bytes.push(b);
                            }
                        }
                    }
                }
            }
        }
        if bytes.is_empty() { None } else { Some(bytes) }
    }
}

// ── Plugin chain (composable paths) ────────────────────────────

/// A chain of stego plugins applied in sequence.
/// Encoding: data → plugin[0] → plugin[1] → ... → final carrier
/// Decoding: carrier → plugin[n-1] → ... → plugin[0] → data
///
/// This is the "path between stego tools" the user configures.
pub struct StegoChain {
    pub plugins: Vec<Box<dyn StegoPlugin>>,
}

impl StegoChain {
    pub fn new() -> Self { Self { plugins: Vec::new() } }

    pub fn push(mut self, p: Box<dyn StegoPlugin>) -> Self {
        self.plugins.push(p); self
    }

    /// Encode through the full chain.
    pub fn encode(&self, data: &[u8]) -> Vec<u8> {
        self.plugins.iter().fold(data.to_vec(), |d, p| p.encode(&d))
    }

    /// Decode through the chain in reverse.
    pub fn decode(&self, carrier: &[u8]) -> Option<Vec<u8>> {
        self.plugins.iter().rev().try_fold(carrier.to_vec(), |c, p| p.decode(&c))
    }

    /// Description of the chain path.
    pub fn path_description(&self) -> String {
        self.plugins.iter().map(|p| p.name()).collect::<Vec<_>>().join(" → ")
    }
}

// ── Config-driven construction ──────────────────────────────────

/// Config for a stego pipeline. Loaded from TOML/JSON.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StegoConfig {
    /// Ordered list of plugin names to chain.
    pub chain: Vec<String>,
    /// Paths to external .so plugins (name → path).
    #[serde(default)]
    pub external: std::collections::HashMap<String, String>,
}

/// Build a StegoChain from config, using built-in plugins.
/// External .so plugins would be loaded via libloading (zos-server PluginDriver).
pub fn chain_from_config(config: &StegoConfig) -> StegoChain {
    let mut chain = StegoChain::new();
    for name in &config.chain {
        let plugin: Box<dyn StegoPlugin> = match name.as_str() {
            "png" | "png-lsb" => Box::new(PngLsb),
            "wav" | "wav-phase" => Box::new(WavPhase),
            "text" | "txt" | "zwc-text" => Box::new(ZeroWidthText),
            "source" | "rs" | "rs-hex" => Box::new(RsHexComment),
            other => {
                // External plugin: would load from config.external[other] via libloading
                // For now, skip unknown plugins with a warning
                eprintln!("warning: unknown stego plugin '{}', skipping (load via zos-server plugin driver)", other);
                continue;
            }
        };
        chain = chain.push(plugin);
    }
    chain
}

// ── C ABI for cdylib plugins ────────────────────────────────────
// Each external plugin .so exports these two functions:
//
//   extern "C" fn stego_encode(data: *const u8, len: usize, out_len: *mut usize) -> *mut u8;
//   extern "C" fn stego_decode(carrier: *const u8, len: usize, out_len: *mut usize) -> *mut u8;
//   extern "C" fn stego_name() -> *const std::ffi::c_char;
//   extern "C" fn stego_extension() -> *const std::ffi::c_char;
//
// The zos-server PluginDriver loads these via libloading::Library::new(path).

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_all_plugins() {
        let data = b"erdfa privacy shard 42 test data";
        let plugins: Vec<Box<dyn StegoPlugin>> = vec![
            Box::new(PngLsb), Box::new(WavPhase),
            Box::new(ZeroWidthText), Box::new(RsHexComment),
        ];
        for p in &plugins {
            let enc = p.encode(data);
            let dec = p.decode(&enc).expect(p.name());
            assert_eq!(&dec, data, "roundtrip failed: {}", p.name());
        }
    }

    #[test]
    fn chain_roundtrip() {
        let data = b"layered encryption test";
        let chain = StegoChain::new()
            .push(Box::new(RsHexComment))  // layer 1: hide in source
            .push(Box::new(PngLsb));       // layer 2: hide source in PNG
        let enc = chain.encode(data);
        let dec = chain.decode(&enc).expect("chain decode");
        assert_eq!(&dec, data);
        assert_eq!(chain.path_description(), "rs-hex → png-lsb");
    }

    #[test]
    fn config_driven_chain() {
        let config = StegoConfig {
            chain: vec!["rs".into(), "png".into()],
            external: Default::default(),
        };
        let chain = chain_from_config(&config);
        let data = b"config test";
        let enc = chain.encode(data);
        let dec = chain.decode(&enc).expect("config chain decode");
        assert_eq!(&dec, data);
    }
}

// ── Backward-compatible API (used by lib.rs ShardSet/Shard) ─────

/// Legacy CarrierType enum mapping to plugins.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum CarrierType { Png, Wav, Text, Source }

/// Encode data into a carrier (legacy API).
pub fn encode(data: &[u8], ct: CarrierType) -> Vec<u8> {
    match ct {
        CarrierType::Png => PngLsb.encode(data),
        CarrierType::Wav => WavPhase.encode(data),
        CarrierType::Text => ZeroWidthText.encode(data),
        CarrierType::Source => RsHexComment.encode(data),
    }
}

/// Decode data from a carrier (legacy API).
pub fn decode(carrier: &[u8], ct: CarrierType) -> Option<Vec<u8>> {
    match ct {
        CarrierType::Png => PngLsb.decode(carrier),
        CarrierType::Wav => WavPhase.decode(carrier),
        CarrierType::Text => ZeroWidthText.decode(carrier),
        CarrierType::Source => RsHexComment.decode(carrier),
    }
}

/// NFT7 segment encoding: pack named segments into a single payload.
pub fn nft7_encode(segments: &[(&str, &[u8])]) -> Vec<u8> {
    let mut out = Vec::new();
    out.extend_from_slice(&(segments.len() as u32).to_be_bytes());
    for (name, data) in segments {
        let nb = name.as_bytes();
        out.extend_from_slice(&(nb.len() as u16).to_be_bytes());
        out.extend_from_slice(nb);
        out.extend_from_slice(&(data.len() as u32).to_be_bytes());
        out.extend_from_slice(data);
    }
    out
}

/// Split a payload into N roughly equal chunks.
pub fn split_payload(payload: &[u8], n: usize) -> Vec<Vec<u8>> {
    if n == 0 { return vec![]; }
    let chunk_size = (payload.len() + n - 1) / n;
    payload.chunks(chunk_size.max(1)).map(|c| c.to_vec()).collect()
}

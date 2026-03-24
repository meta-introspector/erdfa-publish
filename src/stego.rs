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
/// 6-layer bit-plane steganography on 512×512 RGB tiles (retro-sync compatible).
pub struct BitPlane6;

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
        use sha2::{Sha256, Digest};
        let checksum: [u8; 32] = Sha256::digest(data).into();
        let mut out = Vec::new();
        out.extend_from_slice(b"RIFF");
        out.extend_from_slice(&(data.len() as u32 + 72).to_le_bytes());
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
        out.extend_from_slice(&checksum);
        out.extend_from_slice(data);
        out
    }
    fn decode(&self, c: &[u8]) -> Option<Vec<u8>> {
        use sha2::{Sha256, Digest};
        let pos = c.windows(4).position(|w| w == b"data")?;
        let s = pos + 4;
        if c.len() < s + 36 { return None; }
        let len = u32::from_be_bytes(c[s..s + 4].try_into().ok()?) as usize;
        let checksum = &c[s + 4..s + 36];
        if c.len() < s + 36 + len { return None; }
        let data = &c[s + 36..s + 36 + len];
        let actual: [u8; 32] = Sha256::digest(data).into();
        if actual.as_slice() != checksum { return None; }
        Some(data.to_vec())
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

// ── BitPlane6: 6-layer bit-plane stego (retro-sync compatible) ──

const BP_W: usize = 512;
const BP_H: usize = 512;
const BP_PIXELS: usize = BP_W * BP_H;
const BP_PLANES: usize = 6;
const BP_TILE_CAP: usize = BP_PIXELS * BP_PLANES / 8; // 196,608 bytes

impl StegoPlugin for BitPlane6 {
    fn name(&self) -> &str { "bitplane6" }
    fn extension(&self) -> &str { "rgb" }
    fn encode(&self, data: &[u8]) -> Vec<u8> {
        let len = data.len().min(BP_TILE_CAP);
        let mut rgb = vec![128u8; BP_PIXELS * 3];
        // Embed length header (4 bytes BE) + data
        let mut payload = (len as u32).to_be_bytes().to_vec();
        payload.extend_from_slice(&data[..len]);
        for (i, &byte) in payload.iter().enumerate() {
            if i >= BP_TILE_CAP { break; }
            for b in 0..8u8 {
                let bit_idx = i * 8 + b as usize;
                let px = bit_idx / BP_PLANES;
                let plane = bit_idx % BP_PLANES;
                if px >= BP_PIXELS { break; }
                let ch = plane % 3;
                let bit_pos = plane / 3;
                let idx = px * 3 + ch;
                let val = (byte >> b) & 1;
                rgb[idx] = (rgb[idx] & !(1 << bit_pos)) | (val << bit_pos);
            }
        }
        rgb
    }
    fn decode(&self, rgb: &[u8]) -> Option<Vec<u8>> {
        if rgb.len() < BP_PIXELS * 3 { return None; }
        let extract_bytes = |length: usize| -> Vec<u8> {
            (0..length.min(BP_TILE_CAP)).map(|i| {
                (0..8u8).map(|b| {
                    let bit_idx = i * 8 + b as usize;
                    let px = bit_idx / BP_PLANES;
                    let plane = bit_idx % BP_PLANES;
                    if px >= BP_PIXELS { return 0; }
                    let ch = plane % 3;
                    let bit_pos = plane / 3;
                    let idx = px * 3 + ch;
                    ((rgb[idx] >> bit_pos) & 1) << b
                }).sum()
            }).collect()
        };
        // Read 4-byte length header
        let hdr = extract_bytes(4);
        let len = u32::from_be_bytes(hdr[..4].try_into().ok()?) as usize;
        if len > BP_TILE_CAP - 4 { return None; }
        let all = extract_bytes(4 + len);
        Some(all[4..4 + len].to_vec())
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
/// External .so plugins loaded via libloading (zos-server PluginDriver pattern).
pub fn chain_from_config(config: &StegoConfig) -> StegoChain {
    let mut chain = StegoChain::new();
    for name in &config.chain {
        let plugin: Box<dyn StegoPlugin> = match name.as_str() {
            "png" | "png-lsb" => Box::new(PngLsb),
            "wav" | "wav-phase" => Box::new(WavPhase),
            "text" | "txt" | "zwc-text" => Box::new(ZeroWidthText),
            "source" | "rs" | "rs-hex" => Box::new(RsHexComment),
            "bitplane" | "bitplane6" | "bp6" => Box::new(BitPlane6),
            other => {
                if let Some(path) = config.external.get(other) {
                    match ExternalPlugin::load(path) {
                        Ok(p) => Box::new(p),
                        Err(e) => {
                            eprintln!("error loading plugin '{}' from {}: {}", other, path, e);
                            continue;
                        }
                    }
                } else {
                    eprintln!("warning: unknown stego plugin '{}', not in external map", other);
                    continue;
                }
            }
        };
        chain = chain.push(plugin);
    }
    chain
}

// ── External .so plugin via libloading ──────────────────────────
//
// C ABI contract — each cdylib exports:
//   extern "C" fn stego_encode(data: *const u8, len: usize, out_len: *mut usize) -> *mut u8;
//   extern "C" fn stego_decode(carrier: *const u8, len: usize, out_len: *mut usize) -> *mut u8;
//   extern "C" fn stego_name() -> *const std::ffi::c_char;
//   extern "C" fn stego_extension() -> *const std::ffi::c_char;
//   extern "C" fn stego_free(ptr: *mut u8, len: usize);

use std::sync::Arc;

/// A stego plugin loaded from a shared object (.so / .dylib).
pub struct ExternalPlugin {
    _lib: Arc<libloading::Library>,
    name_str: String,
    ext_str: String,
    encode_fn: unsafe extern "C" fn(*const u8, usize, *mut usize) -> *mut u8,
    decode_fn: unsafe extern "C" fn(*const u8, usize, *mut usize) -> *mut u8,
    free_fn: unsafe extern "C" fn(*mut u8, usize),
}

// Safety: the loaded .so is pinned by Arc<Library> and function pointers are valid for its lifetime.
unsafe impl Send for ExternalPlugin {}
unsafe impl Sync for ExternalPlugin {}

impl ExternalPlugin {
    /// Load a stego plugin from a shared object path.
    pub fn load(path: &str) -> Result<Self, String> {
        let lib = unsafe { libloading::Library::new(path) }.map_err(|e| e.to_string())?;
        unsafe {
            let name_fn: libloading::Symbol<unsafe extern "C" fn() -> *const std::ffi::c_char> =
                lib.get(b"stego_name").map_err(|e| e.to_string())?;
            let ext_fn: libloading::Symbol<unsafe extern "C" fn() -> *const std::ffi::c_char> =
                lib.get(b"stego_extension").map_err(|e| e.to_string())?;
            let encode_fn: libloading::Symbol<unsafe extern "C" fn(*const u8, usize, *mut usize) -> *mut u8> =
                lib.get(b"stego_encode").map_err(|e| e.to_string())?;
            let decode_fn: libloading::Symbol<unsafe extern "C" fn(*const u8, usize, *mut usize) -> *mut u8> =
                lib.get(b"stego_decode").map_err(|e| e.to_string())?;
            let free_fn: libloading::Symbol<unsafe extern "C" fn(*mut u8, usize)> =
                lib.get(b"stego_free").map_err(|e| e.to_string())?;

            let name_str = std::ffi::CStr::from_ptr(name_fn()).to_string_lossy().into_owned();
            let ext_str = std::ffi::CStr::from_ptr(ext_fn()).to_string_lossy().into_owned();

            Ok(Self {
                encode_fn: *encode_fn,
                decode_fn: *decode_fn,
                free_fn: *free_fn,
                name_str,
                ext_str,
                _lib: Arc::new(lib),
            })
        }
    }
}

impl StegoPlugin for ExternalPlugin {
    fn name(&self) -> &str { &self.name_str }
    fn extension(&self) -> &str { &self.ext_str }

    fn encode(&self, data: &[u8]) -> Vec<u8> {
        let mut out_len: usize = 0;
        let ptr = unsafe { (self.encode_fn)(data.as_ptr(), data.len(), &mut out_len) };
        if ptr.is_null() { return Vec::new(); }
        let result = unsafe { std::slice::from_raw_parts(ptr, out_len) }.to_vec();
        unsafe { (self.free_fn)(ptr, out_len); }
        result
    }

    fn decode(&self, carrier: &[u8]) -> Option<Vec<u8>> {
        let mut out_len: usize = 0;
        let ptr = unsafe { (self.decode_fn)(carrier.as_ptr(), carrier.len(), &mut out_len) };
        if ptr.is_null() { return None; }
        let result = unsafe { std::slice::from_raw_parts(ptr, out_len) }.to_vec();
        unsafe { (self.free_fn)(ptr, out_len); }
        Some(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_all_plugins() {
        let data = b"erdfa privacy shard 42 test data";
        let plugins: Vec<Box<dyn StegoPlugin>> = vec![
            Box::new(PngLsb), Box::new(WavPhase),
            Box::new(ZeroWidthText), Box::new(RsHexComment),
            Box::new(BitPlane6),
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

    #[test]
    fn lsb_roundtrip() {
        let data = b"Hurrian Hymn h.6 bit-plane test";
        let mut rgb = vec![128u8; PIXELS * 3];
        lsb_embed(&mut rgb, data);
        assert_eq!(&lsb_extract(&rgb, data.len()), data);
    }

    #[test]
    fn lsb_rgba_roundtrip() {
        let data = b"RGBA extract";
        let mut rgb = vec![128u8; PIXELS * 3];
        lsb_embed(&mut rgb, data);
        let mut rgba = vec![255u8; PIXELS * 4];
        for px in 0..PIXELS {
            rgba[px*4] = rgb[px*3]; rgba[px*4+1] = rgb[px*3+1]; rgba[px*4+2] = rgb[px*3+2];
        }
        assert_eq!(&lsb_extract_rgba(&rgba, data.len()), data);
    }

    #[test]
    fn nft7_roundtrip() {
        let segs: Vec<(&str, &[u8])> = vec![("wav", b"RIFF fake"), ("midi", b"MThd")];
        let payload = nft7_encode(&segs);
        assert_eq!(&payload[..4], b"NFT7");
        let decoded = nft7_decode(&payload).unwrap();
        assert_eq!(decoded.len(), 2);
        assert_eq!(decoded[0].name, "wav");
        assert_eq!(decoded[0].data, b"RIFF fake");
    }

    #[test]
    fn split_join_nft7() {
        let segs: Vec<(&str, &[u8])> = vec![("test", &[0xAB; 5000])];
        let payload = nft7_encode(&segs);
        let chunks = split_payload(&payload, 3);
        assert_eq!(chunks.len(), 3);
        assert_eq!(chunks[0].len(), TILE_CAP);
        let decoded = nft7_decode(&join_payload(&chunks)).unwrap();
        assert_eq!(decoded[0].data, vec![0xAB; 5000]);
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

// ── 6-layer bit-plane LSB (real steganography for 512×512 RGB tiles) ──

pub const TILE_W: usize = 512;
pub const TILE_H: usize = 512;
pub const PIXELS: usize = TILE_W * TILE_H;
pub const PLANES: usize = 6;
/// Max bytes per tile: 512×512 × 6 bits / 8 = 196,608 bytes
pub const TILE_CAP: usize = PIXELS * PLANES / 8;

/// Embed `data` into RGB buffer (3 bytes/pixel) using 6 bit planes.
/// Layout per pixel: R0 G0 B0 R1 G1 B1, then next pixel.
pub fn lsb_embed(rgb: &mut [u8], data: &[u8]) {
    for (i, &byte) in data.iter().enumerate() {
        if i >= TILE_CAP { break; }
        for b in 0..8u8 {
            let bit_idx = i * 8 + b as usize;
            let px = bit_idx / PLANES;
            let plane = bit_idx % PLANES;
            if px >= PIXELS { return; }
            let ch = plane % 3;
            let bit_pos = plane / 3;
            let idx = px * 3 + ch;
            let val = (byte >> b) & 1;
            rgb[idx] = (rgb[idx] & !(1 << bit_pos)) | (val << bit_pos);
        }
    }
}

/// Extract `length` bytes from RGB buffer (3 bytes/pixel).
pub fn lsb_extract(rgb: &[u8], length: usize) -> Vec<u8> {
    (0..length.min(TILE_CAP))
        .map(|i| (0..8u8).map(|b| {
            let bit_idx = i * 8 + b as usize;
            let px = bit_idx / PLANES;
            let plane = bit_idx % PLANES;
            if px >= PIXELS { return 0; }
            let idx = px * 3 + (plane % 3);
            ((rgb[idx] >> (plane / 3)) & 1) << b
        }).sum())
        .collect()
}

/// Extract from RGBA buffer (4 bytes/pixel, Canvas getImageData).
pub fn lsb_extract_rgba(rgba: &[u8], length: usize) -> Vec<u8> {
    (0..length.min(TILE_CAP))
        .map(|i| (0..8u8).map(|b| {
            let bit_idx = i * 8 + b as usize;
            let px = bit_idx / PLANES;
            let plane = bit_idx % PLANES;
            if px >= PIXELS { return 0; }
            let idx = px * 4 + (plane % 3); // RGBA stride
            ((rgba[idx] >> (plane / 3)) & 1) << b
        }).sum())
        .collect()
}

// ── NFT7 multi-segment container ──────────────────────────────────

/// Named data segment in an NFT7 container.
#[derive(Debug, Clone)]
pub struct Nft7Segment {
    pub name: String,
    pub data: Vec<u8>,
}

/// Encode segments: `NFT7` magic + count(LE32) + [name_len(LE32) + name + data_len(LE32) + data]...
pub fn nft7_encode(segments: &[(&str, &[u8])]) -> Vec<u8> {
    let mut out = Vec::new();
    out.extend_from_slice(b"NFT7");
    out.extend_from_slice(&(segments.len() as u32).to_le_bytes());
    for (name, data) in segments {
        let nb = name.as_bytes();
        out.extend_from_slice(&(nb.len() as u32).to_le_bytes());
        out.extend_from_slice(nb);
        out.extend_from_slice(&(data.len() as u32).to_le_bytes());
        out.extend_from_slice(data);
    }
    out
}

/// Decode NFT7 → Vec<Nft7Segment>. Returns None on bad magic.
pub fn nft7_decode(data: &[u8]) -> Option<Vec<Nft7Segment>> {
    if data.len() < 8 || &data[0..4] != b"NFT7" { return None; }
    let count = u32::from_le_bytes(data[4..8].try_into().ok()?) as usize;
    let mut off = 8;
    let mut segs = Vec::with_capacity(count);
    for _ in 0..count {
        if off + 4 > data.len() { break; }
        let nl = u32::from_le_bytes(data[off..off+4].try_into().ok()?) as usize;
        off += 4;
        if off + nl + 4 > data.len() { break; }
        let name = String::from_utf8_lossy(&data[off..off+nl]).into_owned();
        off += nl;
        let dl = u32::from_le_bytes(data[off..off+4].try_into().ok()?) as usize;
        off += 4;
        if off + dl > data.len() { break; }
        segs.push(Nft7Segment { name, data: data[off..off+dl].to_vec() });
        off += dl;
    }
    Some(segs)
}

/// Split payload across N tiles, each TILE_CAP bytes (zero-padded).
pub fn split_payload(payload: &[u8], n: usize) -> Vec<Vec<u8>> {
    (0..n).map(|i| {
        let start = i * TILE_CAP;
        let mut chunk = vec![0u8; TILE_CAP];
        if start < payload.len() {
            let end = (start + TILE_CAP).min(payload.len());
            chunk[..end - start].copy_from_slice(&payload[start..end]);
        }
        chunk
    }).collect()
}

/// Reassemble payload from tile chunks.
pub fn join_payload(chunks: &[Vec<u8>]) -> Vec<u8> {
    chunks.iter().flat_map(|c| c.iter().copied()).collect()
}

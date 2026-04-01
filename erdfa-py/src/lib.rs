use pyo3::prelude::*;
use pyo3::types::PyBytes;
use sha2::{Digest, Sha256};

/// First 71 primes for Hecke operator scaling
const PRIMES_71: [u64; 71] = [
    2, 3, 5, 7, 11, 13, 17, 19, 23, 29, 31, 37, 41, 43, 47, 53, 59, 61, 67, 71,
    73, 79, 83, 89, 97, 101, 103, 107, 109, 113, 127, 131, 137, 139, 149, 151,
    157, 163, 167, 173, 179, 181, 191, 193, 197, 199, 211, 223, 227, 229, 233,
    239, 241, 251, 257, 263, 269, 271, 277, 281, 283, 293, 307, 311, 313, 317,
    331, 337, 347, 349, 353,
];

/// Compute orbifold coordinates on Monster Group torus: (mod 71, mod 59, mod 47)
#[pyfunction]
fn orbifold_coords(data: &[u8]) -> (u8, u8, u8) {
    let hash = Sha256::digest(data);
    let v = u64::from_le_bytes(hash[0..8].try_into().unwrap());
    ((v % 71) as u8, (v % 59) as u8, (v % 47) as u8)
}

/// Compute Hecke eigenvalue for content
#[pyfunction]
fn hecke_eigenvalue(data: &[u8], lines: usize, size: usize) -> PyResult<(usize, u64, f64, f64, f64, f64)> {
    let hash = Sha256::digest(data);
    let h_hi = u64::from_be_bytes(hash[0..8].try_into().unwrap());
    let h_lo = u64::from_be_bytes(hash[8..16].try_into().unwrap());
    let shard_id = ((lines as u64 * 7 + size as u64 * 3 + h_hi) % 71) as usize;
    let p = PRIMES_71[shard_id];
    let scale = 2.0 * (p as f64).sqrt();
    let re = ((h_hi % 10000) as f64 / 5000.0 - 1.0) * scale;
    let im = ((h_lo % 10000) as f64 / 5000.0 - 1.0) * scale;
    let norm = (re * re + im * im).sqrt();
    let n = shard_id as f64 + 1.0;
    let maass_weight = (-2.0 * std::f64::consts::PI * n / (1.0 + im.abs())).exp();
    Ok((shard_id, p, re, im, norm, maass_weight))
}

/// Content-address bytes → CIDv0 hash (simplified, no full UnixFS)
#[pyfunction]
fn content_hash(data: &[u8]) -> String {
    hex::encode(Sha256::digest(data))
}

/// DASL address from content
#[pyfunction]
fn dasl_addr(data: &[u8]) -> String {
    let hash = Sha256::digest(data);
    let v = u64::from_be_bytes(hash[0..8].try_into().unwrap());
    format!("0xda51{:012x}", v & 0xffffffffffff)
}

/// Wrap data as DASL/CBOR JSON envelope
#[pyfunction]
fn wrap_dasl(data: &[u8]) -> String {
    let hash = hex::encode(Sha256::digest(data));
    let (a, b, c) = orbifold_coords(data);
    let addr = dasl_addr(data);
    serde_json::json!({
        "dasl:addr": addr,
        "content_hash": hash,
        "sheaf:orbifold": format!("({},{},{})", a, b, c),
        "size": data.len(),
        "crown": 196883,
    }).to_string()
}

/// Encode a game action as FRACTRAN datagram
#[pyfunction]
fn fractran_encode(action: &str, sector: u64, value: u64) -> String {
    let prime_sector = if (sector as usize) < PRIMES_71.len() {
        PRIMES_71[sector as usize]
    } else { 2 };
    format!("SF|1.0|erlan|{}|{}^{}|{}", action, prime_sector, value,
        hex::encode(Sha256::digest(format!("{}:{}:{}", action, sector, value).as_bytes()))[..16].to_string())
}

/// Crown product
#[pyfunction]
fn crown_product() -> u64 {
    47 * 59 * 71
}

const SP: usize = 512 * 512;
const SL: usize = 6;
const SC: usize = SP * SL / 8;
const SH: usize = 36;

fn bp_embed(rgb: &mut [u8], data: &[u8]) {
    for (i, &byte) in data.iter().enumerate() {
        if i >= SC { break; }
        for b in 0..8u8 {
            let bi = i * 8 + b as usize;
            let (px, pl) = (bi / SL, bi % SL);
            if px >= SP { return; }
            let idx = px * 3 + pl % 3;
            let bp = pl / 3;
            rgb[idx] = (rgb[idx] & !(1 << bp)) | (((byte >> b) & 1) << bp);
        }
    }
}

fn bp_extract(rgb: &[u8], len: usize) -> Vec<u8> {
    (0..len.min(SC)).map(|i| (0..8u8).map(|b| {
        let bi = i * 8 + b as usize;
        let (px, pl) = (bi / SL, bi % SL);
        if px >= SP { return 0; }
        ((rgb[px * 3 + pl % 3] >> (pl / 3)) & 1) << b
    }).sum()).collect()
}

#[pyfunction]
fn seal_encode(py: Python<'_>, data: &[u8]) -> PyResult<Py<PyBytes>> {
    if data.len() + SH > SC { return Err(pyo3::exceptions::PyValueError::new_err("too large")); }
    let h = Sha256::digest(data);
    let mut w = Vec::with_capacity(SH + data.len());
    w.extend_from_slice(&(data.len() as u32).to_le_bytes());
    w.extend_from_slice(&h);
    w.extend_from_slice(data);
    let mut rgb = vec![128u8; SP * 3];
    bp_embed(&mut rgb, &w);
    Ok(PyBytes::new_bound(py, &rgb).into())
}

#[pyfunction]
fn seal_decode(py: Python<'_>, rgb: &[u8]) -> PyResult<Py<PyBytes>> {
    if rgb.len() < SP * 3 { return Err(pyo3::exceptions::PyValueError::new_err("too small")); }
    let hdr = bp_extract(rgb, SH);
    let len = u32::from_le_bytes([hdr[0], hdr[1], hdr[2], hdr[3]]) as usize;
    if len == 0 || len + SH > SC { return Err(pyo3::exceptions::PyValueError::new_err("invalid")); }
    let w = bp_extract(rgb, SH + len);
    let payload = &w[36..36 + len];
    if <[u8; 32]>::try_from(&w[4..36]).unwrap() != <[u8; 32]>::from(Sha256::digest(payload)) {
        return Err(pyo3::exceptions::PyValueError::new_err("integrity"));
    }
    Ok(PyBytes::new_bound(py, payload).into())
}

#[pyfunction]
fn seal_pack(py: Python<'_>, state: Vec<f32>, dna: &[u8], wasm: Option<&[u8]>) -> PyResult<Py<PyBytes>> {
    if state.len() != 24 { return Err(pyo3::exceptions::PyValueError::new_err("need 24 floats")); }
    let mut buf = b"SEAL".to_vec();
    for f in &state { buf.extend_from_slice(&f.to_le_bytes()); }
    buf.extend_from_slice(&(dna.len() as u32).to_le_bytes());
    buf.extend_from_slice(dna);
    let w = wasm.unwrap_or(&[]);
    buf.extend_from_slice(&(w.len() as u32).to_le_bytes());
    buf.extend_from_slice(w);
    Ok(PyBytes::new_bound(py, &buf).into())
}

#[pyfunction]
fn seal_unpack(py: Python<'_>, data: &[u8]) -> PyResult<(Vec<f32>, Py<PyBytes>, Py<PyBytes>)> {
    if data.len() < 104 || &data[0..4] != b"SEAL" {
        return Err(pyo3::exceptions::PyValueError::new_err("invalid SEAL"));
    }
    let state: Vec<f32> = (0..24).map(|i| {
        let o = 4 + i * 4;
        f32::from_le_bytes([data[o], data[o+1], data[o+2], data[o+3]])
    }).collect();
    let dl = u32::from_le_bytes([data[100], data[101], data[102], data[103]]) as usize;
    let dna = data[104..104 + dl].to_vec();
    let wo = 104 + dl;
    let wl = u32::from_le_bytes([data[wo], data[wo+1], data[wo+2], data[wo+3]]) as usize;
    let wasm = if wl > 0 { data[wo+4..wo+4+wl].to_vec() } else { vec![] };
    Ok((state, PyBytes::new_bound(py, &dna).into(), PyBytes::new_bound(py, &wasm).into()))
}

#[pymodule]
fn erdfa_py(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(orbifold_coords, m)?)?;
    m.add_function(wrap_pyfunction!(hecke_eigenvalue, m)?)?;
    m.add_function(wrap_pyfunction!(content_hash, m)?)?;
    m.add_function(wrap_pyfunction!(dasl_addr, m)?)?;
    m.add_function(wrap_pyfunction!(wrap_dasl, m)?)?;
    m.add_function(wrap_pyfunction!(fractran_encode, m)?)?;
    m.add_function(wrap_pyfunction!(crown_product, m)?)?;
    m.add_function(wrap_pyfunction!(seal_encode, m)?)?;
    m.add_function(wrap_pyfunction!(seal_decode, m)?)?;
    m.add_function(wrap_pyfunction!(seal_pack, m)?)?;
    m.add_function(wrap_pyfunction!(seal_unpack, m)?)?;
    Ok(())
}

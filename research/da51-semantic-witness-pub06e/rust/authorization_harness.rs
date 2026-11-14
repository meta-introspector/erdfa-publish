use ciborium::Value;
use sha2::{Digest, Sha256};
use std::io::Cursor;

const PINNED_SHA256: &str =
    "df436530f2716cf5f9c92261374638352d4c5e0687e498d1c04b5bd088fb69ec";
const DA51_TAG: u64 = 55889;
const MAX_INPUT_BYTES: usize = 4096;
const REFERENCE: &[u8] = include_bytes!("../fixtures/witness.cbor");

#[derive(Debug, PartialEq, Eq)]
enum Reject {
    Size,
    Identity,
    Decode,
    Trailing,
    Tag,
    Shape,
    Schema,
    Claims,
    Reencode,
}

fn field<'a>(
    values: &'a [(Value, Value)],
    name: &str,
) -> Option<&'a Value> {
    let mut matches = values.iter().filter_map(|(k, v)| {
        if matches!(k, Value::Text(s) if s == name) {
            Some(v)
        } else {
            None
        }
    });

    let result = matches.next()?;
    if matches.next().is_some() {
        return None;
    }
    Some(result)
}

fn decode_complete(data: &[u8]) -> Result<Value, Reject> {
    let mut reader = Cursor::new(data);
    let value: Value =
        ciborium::from_reader(&mut reader)
            .map_err(|_| Reject::Decode)?;

    if reader.position() != data.len() as u64 {
        return Err(Reject::Trailing);
    }

    Ok(value)
}

fn authorize(data: &[u8]) -> Result<(), Reject> {
    if data.len() > MAX_INPUT_BYTES || data.is_empty() {
        return Err(Reject::Size);
    }

    // Authorization is limited to this exact pinned object.
    let hash = Sha256::digest(data);
    let calculated = format!("{hash:x}");

    if calculated != PINNED_SHA256 {
        return Err(Reject::Identity);
    }

    let value = decode_complete(data)?;

    let root = match &value {
        Value::Tag(DA51_TAG, boxed) => match boxed.as_ref() {
            Value::Map(m) => m,
            _ => return Err(Reject::Shape),
        },
        _ => return Err(Reject::Tag),
    };

    match field(root, "schema") {
        Some(Value::Text(s))
            if s == "zos.da51.semantic-witness.experimental.v1" => {}
        _ => return Err(Reject::Schema),
    }

    match field(root, "claims") {
        Some(Value::Array(claims)) if claims.len() == 4 => {}
        _ => return Err(Reject::Claims),
    }

    let mut reencoded = Vec::new();
    ciborium::into_writer(&value, &mut reencoded)
        .map_err(|_| Reject::Reencode)?;

    if reencoded != data {
        return Err(Reject::Reencode);
    }

    Ok(())
}

fn replace_once(data: &[u8], needle: &[u8], replace: &[u8])
    -> Vec<u8>
{
    assert_eq!(needle.len(), replace.len());

    let matches: Vec<usize> = data
        .windows(needle.len())
        .enumerate()
        .filter_map(|(i, window)| {
            if window == needle { Some(i) } else { None }
        })
        .collect();

    assert_eq!(matches.len(), 1, "replacement must be unique");

    let mut output = data.to_vec();
    let start = matches[0];
    output[start..start + replace.len()]
        .copy_from_slice(replace);
    output
}

fn require_rejection(
    name: &str,
    data: &[u8],
    expected: Reject,
) -> Result<(), String> {
    match authorize(data) {
        Err(reason) if reason == expected => {
            println!("REJECTED_{name}={reason:?}");
            Ok(())
        }
        other => Err(format!(
            "UNEXPECTED_RESULT_{name}:{other:?}"
        )),
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    authorize(REFERENCE)
        .map_err(|e| format!("BASELINE_REJECTED:{e:?}"))?;

    println!("PINNED_BASELINE_AUTHORIZED=PASS");

    // First confirm that altered claims can still be
    // syntactically valid CBOR.
    let theorem_change = replace_once(
        REFERENCE,
        b"PUB06E09B.alpha_equivalent",
        b"PUB06E09B.alpha_equivalenX",
    );

    let schema_change = replace_once(
        REFERENCE,
        b"zos.da51.semantic-witness.experimental.v1",
        b"xos.da51.semantic-witness.experimental.v1",
    );

    let statement_change = replace_once(
        REFERENCE,
        b"input Nat 10",
        b"input Nat 11",
    );

    for (name, data) in [
        ("THEOREM", &theorem_change),
        ("SCHEMA", &schema_change),
    ] {
        decode_complete(data)
            .map_err(|e| format!("ALTERED_{name}_NOT_CBOR:{e:?}"))?;
        println!("ALTERED_{name}_CBOR_VALID=YES");
    }

    // The source modification above is checked independently:
    // both possible statements must retain valid CBOR syntax.
    decode_complete(&statement_change)
        .map_err(|e| format!("ALTERED_STATEMENT_NOT_CBOR:{e:?}"))?;
    println!("ALTERED_STATEMENT_CBOR_VALID=YES");

    require_rejection(
        "THEOREM", &theorem_change, Reject::Identity
    )?;
    require_rejection(
        "SCHEMA", &schema_change, Reject::Identity
    )?;
    require_rejection(
        "STATEMENT", &statement_change, Reject::Identity
    )?;

    let evidence_hash_change = replace_once(
        REFERENCE,
        b"6417dfa11363ad5ec39a531dc5301e7e7878b97f2736fc27d14b4226a474b325",
        b"0417dfa11363ad5ec39a531dc5301e7e7878b97f2736fc27d14b4226a474b325",
    );

    require_rejection(
        "EVIDENCE_HASH",
        &evidence_hash_change,
        Reject::Identity,
    )?;

    let mut trailing = REFERENCE.to_vec();
    trailing.push(0);
    require_rejection(
        "TRAILING_BYTES", &trailing, Reject::Identity
    )?;

    let mut truncated = REFERENCE.to_vec();
    truncated.pop();
    require_rejection(
        "TRUNCATED", &truncated, Reject::Identity
    )?;

    let mut wrong_tag = REFERENCE.to_vec();
    wrong_tag[2] ^= 1;
    require_rejection(
        "WRONG_TAG", &wrong_tag, Reject::Identity
    )?;

    require_rejection(
        "DUPLICATE_KEYS",
        &hex_duplicate_keys(),
        Reject::Identity,
    )?;

    // Confirm the lower-level decoder's separate behavior.
    match decode_complete(&trailing) {
        Err(Reject::Trailing) =>
            println!("LOW_LEVEL_TRAILING_REJECTION=PASS"),
        other => return Err(format!(
            "LOW_LEVEL_TRAILING_UNEXPECTED:{other:?}"
        ).into()),
    }

    println!("ADVERSARIAL_CASES=8");
    println!("RUST_PINNED_AUTHORIZATION=PASS");
    Ok(())
}

fn hex_duplicate_keys() -> Vec<u8> {
    vec![
        0xd9, 0xda, 0x51,
        0xa2, 0x61, 0x61, 0x01,
        0x61, 0x61, 0x02,
    ]
}

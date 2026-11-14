use ciborium::Value;
use sha2::{Digest, Sha256};
use std::io::Cursor;

pub const FIXTURE_SHA256: &str =
    "df436530f2716cf5f9c92261374638352d4c5e0687e498d1c04b5bd088fb69ec";

const SCHEMA: &str =
    "zos.da51.semantic-witness.experimental.v1";

const EVIDENCE: [(&str, &str); 5] = [
    ("manifest_sha256",
     "6417dfa11363ad5ec39a531dc5301e7e7878b97f2736fc27d14b4226a474b325"),
    ("proof_source_sha256",
     "6adf4888209b5d8df02323120bcbee72bbfebe1d2de4956320680dfb0522a4b4"),
    ("exact_types_sha256",
     "e6d9b3158361d2400934be33b501bc7055c07fc5e6762e0485a1fd58c28a303c"),
    ("trust_output_sha256",
     "90686a56d0d1e31629031aa348342c5c03f41907af41efc561d8a694e64da617"),
    ("type_report_sha256",
     "1619a250b60b6fcc38feac99c46ba95fc10c90784a313d9474eb6737968ecd0f"),
];

#[derive(Debug, PartialEq, Eq)]
pub enum Reject {
    Size,
    Identity,
    Decode,
    Trailing,
    Tag,
    Canonical,
    Shape,
    Schema,
    Evidence,
    Claims,
}

pub struct TrustedPolicy<'a> {
    // Supplied by trusted application configuration, not by CBOR.
    pub expected_sha256: &'a str,
}

fn encode(value: &Value) -> Result<Vec<u8>, Reject> {
    let mut bytes = Vec::new();
    ciborium::into_writer(value, &mut bytes)
        .map_err(|_| Reject::Canonical)?;
    Ok(bytes)
}

fn map<'a>(value: &'a Value) -> Result<&'a [(Value, Value)], Reject> {
    match value {
        Value::Map(pairs) => Ok(pairs),
        _ => Err(Reject::Shape),
    }
}

fn field<'a>(
    pairs: &'a [(Value, Value)],
    key: &str,
) -> Option<&'a Value> {
    let mut found = None;
    for (k, v) in pairs {
        if matches!(k, Value::Text(s) if s == key) {
            if found.is_some() {
                return None;
            }
            found = Some(v);
        }
    }
    found
}

fn text(value: &Value) -> Option<&str> {
    match value {
        Value::Text(s) => Some(s),
        _ => None,
    }
}

fn validate_profile(value: &Value, depth: usize) -> Result<(), Reject> {
    if depth > 8 {
        return Err(Reject::Shape);
    }

    match value {
        Value::Text(s) if s.as_bytes().len() <= 1024 => Ok(()),
        Value::Integer(_) | Value::Bool(_) | Value::Null => Ok(()),
        Value::Bytes(b) if b.len() <= 4096 => Ok(()),
        Value::Array(values) if values.len() <= 128 => {
            for child in values {
                validate_profile(child, depth + 1)?;
            }
            Ok(())
        }
        Value::Map(pairs) if pairs.len() <= 128 => {
            let mut previous: Option<Vec<u8>> = None;
            for (key, val) in pairs {
                if !matches!(key, Value::Text(_)) {
                    return Err(Reject::Shape);
                }
                validate_profile(key, depth + 1)?;
                validate_profile(val, depth + 1)?;

                let encoded = encode(key)?;
                if let Some(prev) = &previous {
                    if (encoded.len(), encoded.as_slice())
                        <= (prev.len(), prev.as_slice())
                    {
                        return Err(Reject::Canonical);
                    }
                }
                previous = Some(encoded);
            }
            Ok(())
        }
        _ => Err(Reject::Shape),
    }
}

pub fn authorize(
    bytes: &[u8],
    policy: &TrustedPolicy<'_>,
) -> Result<(), Reject> {
    if bytes.is_empty() || bytes.len() > 4096 {
        return Err(Reject::Size);
    }

    let actual = format!("{:x}", Sha256::digest(bytes));

    if actual != policy.expected_sha256 {
        return Err(Reject::Identity);
    }

    let mut reader = Cursor::new(bytes);
    let value: Value =
        ciborium::from_reader(&mut reader)
            .map_err(|_| Reject::Decode)?;

    if reader.position() != bytes.len() as u64 {
        return Err(Reject::Trailing);
    }

    if encode(&value)? != bytes {
        return Err(Reject::Canonical);
    }

    let payload = match &value {
        Value::Tag(55889, inner) => inner.as_ref(),
        _ => return Err(Reject::Tag),
    };

    validate_profile(payload, 0)?;

    let root = map(payload)?;
    if root.len() != 3 {
        return Err(Reject::Shape);
    }

    match field(root, "schema").and_then(text) {
        Some(SCHEMA) => (),
        _ => return Err(Reject::Schema),
    }

    let evidence = field(root, "evidence")
        .ok_or(Reject::Evidence)
        .and_then(|v| map(v).map_err(|_| Reject::Evidence))?;

    if evidence.len() != EVIDENCE.len() {
        return Err(Reject::Evidence);
    }

    for (name, digest) in EVIDENCE {
        if field(evidence, name).and_then(text) != Some(digest) {
            return Err(Reject::Evidence);
        }
    }

    let claims = match field(root, "claims") {
        Some(Value::Array(items)) if items.len() == 4 => items,
        _ => return Err(Reject::Claims),
    };

    for claim in claims {
        let fields = map(claim).map_err(|_| Reject::Claims)?;

        if fields.len() != 7 {
            return Err(Reject::Claims);
        }

        for name in [
            "theorem", "kind", "scope", "lhs",
            "rhs", "statement",
        ] {
            if field(fields, name).and_then(text).is_none() {
                return Err(Reject::Claims);
            }
        }

        match field(fields, "axioms") {
            Some(Value::Array(values))
                if values.iter().all(|v| text(v).is_some()) => (),
            _ => return Err(Reject::Claims),
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const ORIGINAL: &[u8] =
        include_bytes!("../fixtures/witness.cbor");

    fn policy() -> TrustedPolicy<'static> {
        TrustedPolicy {
            expected_sha256: FIXTURE_SHA256,
        }
    }

    fn policy_for(bytes: &[u8]) -> TrustedPolicy<'static> {
        TrustedPolicy {
            expected_sha256: Box::leak(
                format!("{:x}", Sha256::digest(bytes)).into_boxed_str()
            ),
        }
    }

    fn decoded() -> Value {
        ciborium::from_reader(ORIGINAL).unwrap()
    }

    fn mutate_field(key: &str, new_value: Value) -> Vec<u8> {
        let mut value = decoded();

        if let Value::Tag(_, inner) = &mut value {
            if let Value::Map(pairs) = inner.as_mut() {
                let entry = pairs.iter_mut()
                    .find(|(k, _)| matches!(k, Value::Text(s) if s == key))
                    .unwrap();
                entry.1 = new_value;
            }
        }

        encode(&value).unwrap()
    }

    #[test]
    fn original_is_authorized() {
        assert_eq!(authorize(ORIGINAL, &policy()), Ok(()));
    }

    #[test]
    fn altered_schema_rejected_even_under_new_identity() {
        let changed = mutate_field(
            "schema", Value::Text("untrusted.v1".into())
        );
        assert_eq!(
            authorize(&changed, &policy_for(&changed)),
            Err(Reject::Schema)
        );
    }

    #[test]
    fn altered_evidence_rejected_even_under_new_identity() {
        let mut value = decoded();

        if let Value::Tag(_, inner) = &mut value {
            if let Value::Map(root) = inner.as_mut() {
                let evidence = root.iter_mut()
                    .find(|(k, _)| matches!(k, Value::Text(s) if s == "evidence"))
                    .unwrap();

                if let Value::Map(pairs) = &mut evidence.1 {
                    let item = pairs.iter_mut()
                        .find(|(k, _)| matches!(k, Value::Text(s) if s == "proof_source_sha256"))
                        .unwrap();
                    item.1 = Value::Text("0".repeat(64));
                }
            }
        }

        let changed = encode(&value).unwrap();
        assert_eq!(
            authorize(&changed, &policy_for(&changed)),
            Err(Reject::Evidence)
        );
    }

    #[test]
    fn altered_claims_rejected_even_under_new_identity() {
        let changed = mutate_field("claims", Value::Array(vec![]));
        assert_eq!(
            authorize(&changed, &policy_for(&changed)),
            Err(Reject::Claims)
        );
    }

    #[test]
    fn changed_content_rejected_by_trusted_identity() {
        let mut changed = ORIGINAL.to_vec();
        changed[10] ^= 1;
        assert_eq!(
            authorize(&changed, &policy()),
            Err(Reject::Identity)
        );
    }

    #[test]
    fn trailing_data_rejected() {
        let mut changed = ORIGINAL.to_vec();
        changed.push(0);
        assert_eq!(
            authorize(&changed, &policy_for(&changed)),
            Err(Reject::Trailing)
        );
    }

    #[test]
    fn wrong_tag_rejected() {
        let mut changed = decoded();
        if let Value::Tag(tag, _) = &mut changed {
            *tag = 55890;
        }
        let bytes = encode(&changed).unwrap();
        assert_eq!(
            authorize(&bytes, &policy_for(&bytes)),
            Err(Reject::Tag)
        );
    }

    #[test]
    fn duplicate_map_keys_rejected() {
        let bytes = [
            0xd9, 0xda, 0x51,
            0xa2, 0x61, b'a', 0x01,
            0x61, b'a', 0x02,
        ];
        assert_eq!(
            authorize(&bytes, &policy_for(&bytes)),
            Err(Reject::Canonical)
        );
    }

    #[test]
    fn nonminimal_tag_rejected() {
        let mut changed = vec![0xda, 0, 0, 0xda, 0x51];
        changed.extend_from_slice(&ORIGINAL[3..]);

        assert_eq!(
            authorize(&changed, &policy_for(&changed)),
            Err(Reject::Canonical)
        );
    }

    #[test]
    fn excessive_size_rejected() {
        let data = vec![0u8; 4097];
        assert_eq!(
            authorize(&data, &policy_for(&data)),
            Err(Reject::Size)
        );
    }

    #[test]
    fn missing_evidence_rejected() {
        let changed = mutate_field(
            "evidence", Value::Map(vec![])
        );
        assert_eq!(
            authorize(&changed, &policy_for(&changed)),
            Err(Reject::Evidence)
        );
    }
}

#[cfg(test)]
mod adversarial;

/// Experimental fixture-specific semantic claim authorization.
///
/// The caller's trusted object digest and the fixed claim reference
/// are separate authorities. This DOES NOT execute Lean or verify
/// mathematical propositions.
pub fn authorize_pinned_claims(
    bytes: &[u8],
    policy: &TrustedPolicy<'_>,
) -> Result<(), Reject> {
    // Preserve the existing structural and evidence checks.
    authorize(bytes, policy)?;

    const REFERENCE: &[u8] =
        include_bytes!("../fixtures/witness.cbor");

    // Fail closed if the compiled reference is not the audited fixture.
    let reference_digest =
        format!("{:x}", Sha256::digest(REFERENCE));

    if reference_digest != FIXTURE_SHA256 {
        return Err(Reject::Identity);
    }

    let trusted = TrustedPolicy {
        expected_sha256: FIXTURE_SHA256,
    };
    authorize(REFERENCE, &trusted)?;

    fn claim_bytes(data: &[u8]) -> Result<Vec<u8>, Reject> {
        let mut reader = Cursor::new(data);
        let value: Value =
            ciborium::from_reader(&mut reader)
                .map_err(|_| Reject::Decode)?;

        if reader.position() != data.len() as u64 {
            return Err(Reject::Trailing);
        }

        let payload = match &value {
            Value::Tag(55889, inner) => inner.as_ref(),
            _ => return Err(Reject::Tag),
        };

        let root = map(payload)?;
        let claims = field(root, "claims")
            .ok_or(Reject::Claims)?;

        encode(claims)
    }

    if claim_bytes(bytes)? != claim_bytes(REFERENCE)? {
        return Err(Reject::Claims);
    }

    Ok(())
}

#[cfg(test)]
mod pinned_claim_policy_tests {
    use super::*;

    const ORIGINAL: &[u8] =
        include_bytes!("../fixtures/witness.cbor");

    fn repin(bytes: &[u8]) -> TrustedPolicy<'static> {
        TrustedPolicy {
            expected_sha256: Box::leak(
                format!("{:x}", Sha256::digest(bytes))
                    .into_boxed_str()
            ),
        }
    }

    fn modified_claim(field_name: &str, replacement: Value)
        -> Vec<u8>
    {
        let mut value: Value =
            ciborium::from_reader(ORIGINAL).unwrap();

        let Value::Tag(55889, inner) = &mut value else {
            panic!("unexpected tag");
        };

        let Value::Map(root) = inner.as_mut() else {
            panic!("unexpected root");
        };

        let (_, claim_value) = root.iter_mut()
            .find(|(key, _)| {
                matches!(key, Value::Text(s) if s == "claims")
            })
            .unwrap();

        let Value::Array(claims) = claim_value else {
            panic!("unexpected claims");
        };

        let Value::Map(fields) = &mut claims[0] else {
            panic!("unexpected claim");
        };

        let (_, target) = fields.iter_mut()
            .find(|(key, _)| {
                matches!(key, Value::Text(s) if s == field_name)
            })
            .unwrap();

        *target = replacement;

        let mut result = Vec::new();
        ciborium::into_writer(&value, &mut result).unwrap();
        result
    }

    fn require_remediation(field_name: &str, replacement: Value) {
        let changed = modified_claim(field_name, replacement);
        assert_ne!(changed, ORIGINAL);

        let policy = repin(&changed);

        // Prove that the old validator still exhibits
        // the previously identified policy gap.
        assert_eq!(authorize(&changed, &policy), Ok(()));

        // Prove that the new fixture-specific policy closes it.
        assert_eq!(
            authorize_pinned_claims(&changed, &policy),
            Err(Reject::Claims)
        );
    }

    #[test]
    fn original_fixture_authorized() {
        let policy = TrustedPolicy {
            expected_sha256: FIXTURE_SHA256,
        };

        assert_eq!(
            authorize_pinned_claims(ORIGINAL, &policy),
            Ok(())
        );
    }

    #[test]
    fn changed_theorem_rejected_even_when_repinned() {
        require_remediation(
            "theorem",
            Value::Text("UNVERIFIED_THEOREM".into()),
        );
    }

    #[test]
    fn changed_statement_rejected_even_when_repinned() {
        require_remediation(
            "statement",
            Value::Text("unverified mathematical claim".into()),
        );
    }

    #[test]
    fn changed_axioms_rejected_even_when_repinned() {
        require_remediation(
            "axioms",
            Value::Array(vec![
                Value::Text("untrusted_axiom".into()),
            ]),
        );
    }
}

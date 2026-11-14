#[cfg(test)]
mod tests {
    use crate::{authorize, TrustedPolicy};
    use ciborium::Value;
    use sha2::{Digest, Sha256};

    const ORIGINAL: &[u8] =
        include_bytes!("../fixtures/witness.cbor");

    fn mutate(name: &str, replacement: Value) -> Vec<u8> {
        let mut value: Value =
            ciborium::from_reader(ORIGINAL).unwrap();

        let Value::Tag(55889, inner) = &mut value else {
            panic!("unexpected DA51 tag");
        };

        let Value::Map(root) = inner.as_mut() else {
            panic!("unexpected root");
        };

        let (_, claims) = root.iter_mut()
            .find(|(k, _)| {
                matches!(k, Value::Text(s) if s == "claims")
            })
            .unwrap();

        let Value::Array(claims) = claims else {
            panic!("unexpected claims");
        };

        let Value::Map(fields) = &mut claims[0] else {
            panic!("unexpected claim");
        };

        let (_, target) = fields.iter_mut()
            .find(|(k, _)| {
                matches!(k, Value::Text(s) if s == name)
            })
            .unwrap();

        *target = replacement;

        let mut encoded = Vec::new();
        ciborium::into_writer(&value, &mut encoded).unwrap();
        encoded
    }

    fn probe(name: &str, replacement: Value) {
        let bytes = mutate(name, replacement);
        assert_ne!(bytes, ORIGINAL);

        let digest = format!("{:x}", Sha256::digest(&bytes));
        let policy = TrustedPolicy {
            expected_sha256: &digest,
        };

        // Deliberately tests whether the current
        // consumer accepts modified semantic contents
        // under a separately updated trusted digest.
        assert_eq!(
            authorize(&bytes, &policy),
            Ok(()),
            "unexpected rejection: {name}"
        );
    }

    #[test]
    fn modified_theorem_accepted_under_repin() {
        probe(
            "theorem",
            Value::Text("UNVERIFIED_THEOREM".into()),
        );
    }

    #[test]
    fn modified_statement_accepted_under_repin() {
        probe(
            "statement",
            Value::Text("unverified mathematical claim".into()),
        );
    }

    #[test]
    fn modified_axioms_accepted_under_repin() {
        probe(
            "axioms",
            Value::Array(vec![
                Value::Text("untrusted_axiom".into()),
            ]),
        );
    }
}

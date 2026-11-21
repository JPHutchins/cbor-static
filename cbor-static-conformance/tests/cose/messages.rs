//! Whole messages, one case per example: decoded and re-encoded byte for byte, or rejected where
//! the generator broke the tag.

use cbor_static::ErrorKind;
use cbor_static_conformance::cose::Message;

use crate::common::vectors::{COSE_WG, Failure, vector};
use crate::decoded::{kind, message, reencoded};

/// The examples whose tag the generator changed, and how the typed layer rejects each: an
/// unassigned tag, or a Mac-shaped array under `COSE_Mac0`'s tag.
const CHANGED_TAG: [(&str, ErrorKind); 6] = [
    ("encrypted-tests/enc-fail-01", ErrorKind::UnexpectedTag),
    ("enveloped-tests/env-fail-01", ErrorKind::UnexpectedTag),
    ("mac-tests/mac-fail-01", ErrorKind::TypeMismatch),
    ("mac0-tests/mac-fail-01", ErrorKind::UnexpectedTag),
    ("sign-tests/sign-fail-01", ErrorKind::UnexpectedTag),
    ("sign1-tests/sign-fail-01", ErrorKind::UnexpectedTag),
];

/// The example re-encodes byte for byte as the message type its input names, untagged only where
/// the generator removed the tag; or, where it changed the tag, is rejected.
fn reencodes_or_is_rejected(name: &str) {
    let vector = vector(name);
    let ops = vector.expect.ops();
    if let Some(&(_, error)) = CHANGED_TAG.iter().find(|&&(changed, _)| changed == name) {
        assert!(ops.contains(&Failure::ChangeCborTag));
        assert_eq!(message(vector).map(|_| ()).map_err(|e| e.kind), Err(error));
    } else {
        assert!(!ops.contains(&Failure::ChangeCborTag));
        assert_eq!(reencoded(vector).as_deref(), Ok(vector.output));
        assert_eq!(
            message(vector).map(|decoded| kind(&decoded)),
            Ok(vector.kind)
        );
        assert_eq!(
            cbor_static::from_slice::<Message>(vector.output).is_ok(),
            !ops.contains(&Failure::RemoveCborTag)
        );
    }
}

macro_rules! cases {
    ($(($test:ident, $name:literal)),* $(,)?) => {
        $(
            #[test]
            fn $test() {
                reencodes_or_is_rejected($name);
            }
        )*
    };
}

include!(concat!(env!("COSE_WG_VECTORS"), "/cases.rs"));

#[test]
fn the_examples_split_into_passes_failures_and_broken_tags() {
    use crate::common::vectors::Expect;

    let failing = COSE_WG
        .iter()
        .filter(|vector| matches!(vector.expect, Expect::Fail(_)));
    assert_eq!(
        (
            COSE_WG.len(),
            failing.clone().count(),
            failing
                .filter(|vector| vector.expect.ops().contains(&Failure::ChangeCborTag))
                .count(),
            COSE_WG
                .iter()
                .filter(|vector| matches!(vector.expect, Expect::Pass(ops) if !ops.is_empty()))
                .count(),
        ),
        (306, 40, CHANGED_TAG.len(), 12)
    );
}

//! What a type writes, an item or a sequence of items, through every oracle: each reads every item
//! as `Tokens` does, and each one's re-encoding reads back through the type, which writes the same
//! bytes again.

use cbor_static::Error;

use super::{cbor2, legs};

#[derive(Clone, Copy, Debug)]
pub enum Oracle {
    Ciborium,
    Minicbor,
    Cbor2,
}

const ORACLES: [Oracle; 3] = [Oracle::Ciborium, Oracle::Minicbor, Oracle::Cbor2];

/// The oracle's re-encoding of the item it reads in `bytes`.
///
/// # Panics
///
/// If the oracle refuses the item or cannot write it.
pub fn reencoded(oracle: Oracle, bytes: &[u8]) -> Vec<u8> {
    match oracle {
        Oracle::Ciborium => {
            let mut written = Vec::new();
            ciborium::into_writer(
                &ciborium::from_reader::<ciborium::Value, _>(bytes)
                    .unwrap_or_else(|error| panic!("ciborium refuses {bytes:02x?}: {error}")),
                &mut written,
            )
            .expect("ciborium writes its own value");
            written
        }
        Oracle::Minicbor => minicbor::decode::Tokenizer::new(bytes)
            .try_fold(minicbor::Encoder::new(Vec::new()), |mut encoder, token| {
                encoder.encode(
                    token.unwrap_or_else(|error| panic!("minicbor refuses {bytes:02x?}: {error}")),
                )?;
                Ok::<_, minicbor::encode::Error<_>>(encoder)
            })
            .expect("minicbor writes its own tokens")
            .into_writer(),
        Oracle::Cbor2 => match cbor2::reply(bytes) {
            cbor2::Reply::Reencoded { cbor } => cbor,
            cbor2::Reply::Refused { reason } => panic!("cbor2 refuses {bytes:02x?}: {reason}"),
        },
    }
}

/// Asserts that every oracle reads each item of `ours` as `Tokens` does, and that each one's
/// re-encoding of them reads back through `read_back` as `ours`; returns the normalization rows
/// that applied.
///
/// # Panics
///
/// If an oracle disagrees, or the type reads an oracle's re-encoding otherwise.
#[track_caller]
pub fn round_trip(
    ours: &[u8],
    read_back: impl Fn(&[u8]) -> Result<Vec<u8>, Error>,
) -> Vec<legs::Row> {
    let items = items(ours);
    let rows = items.iter().flat_map(|item| legs::agree(item)).collect();
    for oracle in ORACLES {
        let theirs = items
            .iter()
            .flat_map(|item| reencoded(oracle, item))
            .collect::<Vec<_>>();
        assert_eq!(
            read_back(&theirs).as_deref(),
            Ok(ours),
            "{oracle:?} re-encodes {ours:02x?} as {theirs:02x?}"
        );
    }
    rows
}

/// The items of a CBOR sequence, each whole.
fn items(sequence: &[u8]) -> Vec<&[u8]> {
    core::iter::successors(split(sequence), |&(_, rest)| split(rest))
        .map(|(item, _)| item)
        .collect()
}

/// The first item of `bytes` and what follows it; none when `bytes` is empty.
fn split(bytes: &[u8]) -> Option<(&[u8], &[u8])> {
    (!bytes.is_empty()).then(|| {
        cbor_static::from_slice_partial::<cbor_static::RawCbor>(bytes).map_or_else(
            |error| panic!("not a whole item: {bytes:02x?}: {error:?}"),
            |(item, rest)| (item.as_bytes(), rest),
        )
    })
}

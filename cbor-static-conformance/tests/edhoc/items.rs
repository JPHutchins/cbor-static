//! Every CBOR item of both traces, one case each: it decodes as the type its caption names and
//! re-encodes byte for byte.

use cbor_static::Error;
use cbor_static_conformance::edhoc::Sequence;

use crate::common::reencode;

/// An RFC 9529 caption, camel-cased by `nix/rfc-vectors.nix`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Item {
    A3,
    A4,
    CI,
    CR,
    Context2,
    Context3,
    ContextForKeyUpdate,
    CredI,
    CredR,
    Ead1,
    Ead2,
    Ead3,
    Ead4,
    Error,
    GX,
    GY,
    HMessage1,
    IdCredI,
    IdCredR,
    InfoForIv3,
    InfoForIv4,
    InfoForK3,
    InfoForK4,
    InfoForKeyUpdate,
    InfoForKeystream2,
    InfoForMac2,
    InfoForMac3,
    InfoForOscoreMasterSalt,
    InfoForOscoreMasterSecret,
    InfoForPrkExporter,
    InfoForPrkOut,
    InfoForSalt3e2m,
    InfoForSalt4e3m,
    InputToCalculateTh2,
    InputToCalculateTh3,
    InputToCalculateTh4,
    Mac2,
    Mac3,
    Message1,
    Message2,
    Message3,
    Message4,
    MessageToBeSignedInMessage2,
    MessageToBeSignedInMessage3,
    Method,
    Plaintext2,
    Plaintext3,
    Plaintext4,
    SignatureOrMac2,
    SignatureOrMac3,
    SuitesI,
    Th2,
    Th3,
    Th4,
}

/// A group decoded from the whole of `input`, and written back.
pub fn resequenced<'a, Group: Sequence<'a>>(input: &'a [u8]) -> Result<Vec<u8>, Error> {
    let mut out = vec![0u8; 4096];
    Group::from_sequence(input)
        .and_then(|group| group.to_sequence(&mut out))
        .map(|len| out[..len].to_vec())
}

/// The item, decoded as its caption's type and re-encoded.
pub fn reencoded(item: Item, input: &[u8]) -> Result<Vec<u8>, Error> {
    use cbor_static_conformance::cose::{EncStructure, SigStructure1};
    use cbor_static_conformance::edhoc::{
        ConnectionId, Context2, Context3, Cred, Eads, ErrorMessage, IdCred, Info, Message1,
        Message2, Message3, Message4, Plaintext2, Plaintext3, Plaintext4, Suites, Th2Input,
        TranscriptInput,
    };

    match item {
        Item::Method => reencode::<i64>(input),
        Item::SuitesI => reencode::<Suites>(input),
        Item::GX
        | Item::GY
        | Item::HMessage1
        | Item::Th2
        | Item::Th3
        | Item::Th4
        | Item::Mac2
        | Item::Mac3
        | Item::SignatureOrMac2
        | Item::SignatureOrMac3
        | Item::ContextForKeyUpdate => reencode::<&[u8]>(input),
        Item::CI | Item::CR => reencode::<ConnectionId>(input),
        Item::IdCredI | Item::IdCredR => reencode::<IdCred>(input),
        Item::CredI | Item::CredR => reencode::<Cred>(input),
        Item::Ead1 | Item::Ead2 | Item::Ead3 | Item::Ead4 => resequenced::<Eads>(input),
        Item::Message1 => resequenced::<Message1>(input),
        Item::Message2 => reencode::<Message2>(input),
        Item::Message3 => reencode::<Message3>(input),
        Item::Message4 => reencode::<Message4>(input),
        Item::Plaintext2 => resequenced::<Plaintext2>(input),
        Item::Plaintext3 => resequenced::<Plaintext3>(input),
        Item::Plaintext4 => resequenced::<Plaintext4>(input),
        Item::Error => resequenced::<ErrorMessage>(input),
        Item::Context2 => resequenced::<Context2>(input),
        Item::Context3 => resequenced::<Context3>(input),
        Item::InputToCalculateTh2 => resequenced::<Th2Input>(input),
        Item::InputToCalculateTh3 => resequenced::<TranscriptInput<Plaintext2>>(input),
        Item::InputToCalculateTh4 => resequenced::<TranscriptInput<Plaintext3>>(input),
        Item::InfoForIv3
        | Item::InfoForIv4
        | Item::InfoForK3
        | Item::InfoForK4
        | Item::InfoForKeyUpdate
        | Item::InfoForKeystream2
        | Item::InfoForMac2
        | Item::InfoForMac3
        | Item::InfoForOscoreMasterSalt
        | Item::InfoForOscoreMasterSecret
        | Item::InfoForPrkExporter
        | Item::InfoForPrkOut
        | Item::InfoForSalt3e2m
        | Item::InfoForSalt4e3m => resequenced::<Info>(input),
        Item::MessageToBeSignedInMessage2 | Item::MessageToBeSignedInMessage3 => {
            reencode::<SigStructure1>(input)
        }
        Item::A3 | Item::A4 => reencode::<EncStructure>(input),
    }
}

macro_rules! rfc9529_cases {
    ($(($test:ident, $section:literal, $item:ident)),* $(,)?) => {
        $(
            #[test]
            fn $test() {
                assert_eq!(
                    reencoded(Item::$item, rfc9529!($test)).as_deref(),
                    Ok(rfc9529!($test))
                );
            }
        )*
    };
}

include!(concat!(env!("RFC_VECTORS"), "/rfc9529_cases.rs"));

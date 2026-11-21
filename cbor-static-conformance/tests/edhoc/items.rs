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

/// What an item is: the type it decodes as, and the EAD group's number.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Method,
    Suites,
    Bytes,
    ConnectionId,
    IdCred,
    Cred,
    Eads(u8),
    Message1,
    Message2,
    Message3,
    Message4,
    Plaintext2,
    Plaintext3,
    Plaintext4,
    Error,
    Context2,
    Context3,
    Th2Input,
    Th3Input,
    Th4Input,
    Info,
    SigStructure,
    EncStructure,
}

pub const fn kind(item: Item) -> Kind {
    match item {
        Item::Method => Kind::Method,
        Item::SuitesI => Kind::Suites,
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
        | Item::ContextForKeyUpdate => Kind::Bytes,
        Item::CI | Item::CR => Kind::ConnectionId,
        Item::IdCredI | Item::IdCredR => Kind::IdCred,
        Item::CredI | Item::CredR => Kind::Cred,
        Item::Ead1 => Kind::Eads(1),
        Item::Ead2 => Kind::Eads(2),
        Item::Ead3 => Kind::Eads(3),
        Item::Ead4 => Kind::Eads(4),
        Item::Message1 => Kind::Message1,
        Item::Message2 => Kind::Message2,
        Item::Message3 => Kind::Message3,
        Item::Message4 => Kind::Message4,
        Item::Plaintext2 => Kind::Plaintext2,
        Item::Plaintext3 => Kind::Plaintext3,
        Item::Plaintext4 => Kind::Plaintext4,
        Item::Error => Kind::Error,
        Item::Context2 => Kind::Context2,
        Item::Context3 => Kind::Context3,
        Item::InputToCalculateTh2 => Kind::Th2Input,
        Item::InputToCalculateTh3 => Kind::Th3Input,
        Item::InputToCalculateTh4 => Kind::Th4Input,
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
        | Item::InfoForSalt4e3m => Kind::Info,
        Item::MessageToBeSignedInMessage2 | Item::MessageToBeSignedInMessage3 => Kind::SigStructure,
        Item::A3 | Item::A4 => Kind::EncStructure,
    }
}

/// The item, decoded as its kind's type and re-encoded.
pub fn reencoded(item: Item, input: &[u8]) -> Result<Vec<u8>, Error> {
    use cbor_static_conformance::cose::{EncStructure, SigStructure1};
    use cbor_static_conformance::edhoc::{
        ConnectionId, Context2, Context3, Cred, Eads, ErrorMessage, IdCred, Info, Message1,
        Message2, Message3, Message4, Plaintext2, Plaintext3, Plaintext4, Suites, Th2Input,
        TranscriptInput,
    };

    match kind(item) {
        Kind::Method => reencode::<i64>(input),
        Kind::Suites => reencode::<Suites>(input),
        Kind::Bytes => reencode::<&[u8]>(input),
        Kind::ConnectionId => reencode::<ConnectionId>(input),
        Kind::IdCred => reencode::<IdCred>(input),
        Kind::Cred => reencode::<Cred>(input),
        Kind::Eads(_) => resequenced::<Eads>(input),
        Kind::Message1 => resequenced::<Message1>(input),
        Kind::Message2 => reencode::<Message2>(input),
        Kind::Message3 => reencode::<Message3>(input),
        Kind::Message4 => reencode::<Message4>(input),
        Kind::Plaintext2 => resequenced::<Plaintext2>(input),
        Kind::Plaintext3 => resequenced::<Plaintext3>(input),
        Kind::Plaintext4 => resequenced::<Plaintext4>(input),
        Kind::Error => resequenced::<ErrorMessage>(input),
        Kind::Context2 => resequenced::<Context2>(input),
        Kind::Context3 => resequenced::<Context3>(input),
        Kind::Th2Input => resequenced::<Th2Input>(input),
        Kind::Th3Input => resequenced::<TranscriptInput<Plaintext2>>(input),
        Kind::Th4Input => resequenced::<TranscriptInput<Plaintext3>>(input),
        Kind::Info => resequenced::<Info>(input),
        Kind::SigStructure => reencode::<SigStructure1>(input),
        Kind::EncStructure => reencode::<EncStructure>(input),
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

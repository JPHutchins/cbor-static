//! Examples decoded into the message types, and their layers walked.

use cbor_static::{Error, RawCbor};
use cbor_static_conformance::cose::{
    BytesOrNil, Encrypt, Encrypt0, HeaderMap, Mac, Mac0, Message, Protected, Recipient, Sign, Sign1,
};

use crate::common::encode;
use crate::common::vectors::{Failure, Kind, Vector};

/// The example's message: tagged, unless its generator removed the tag, when its input's kind
/// says which bare struct it is.
pub fn message(vector: &Vector<Kind>) -> Result<Message<'static>, Error> {
    if vector.expect.ops().contains(&Failure::RemoveCborTag) {
        untagged(vector.kind, vector.output)
    } else {
        cbor_static::from_slice(vector.output)
    }
}

fn untagged(kind: Kind, input: &'static [u8]) -> Result<Message<'static>, Error> {
    match kind {
        Kind::Sign => cbor_static::from_slice::<Sign>(input).map(Message::Sign),
        Kind::Sign1 => cbor_static::from_slice::<Sign1>(input).map(Message::Sign1),
        Kind::Encrypt => cbor_static::from_slice::<Encrypt>(input).map(Message::Encrypt),
        Kind::Encrypt0 => cbor_static::from_slice::<Encrypt0>(input).map(Message::Encrypt0),
        Kind::Mac => cbor_static::from_slice::<Mac>(input).map(Message::Mac),
        Kind::Mac0 => cbor_static::from_slice::<Mac0>(input).map(Message::Mac0),
    }
}

/// The example's message, or `None` where the generator changed its tag; any other failure to
/// decode panics.
pub fn intact(vector: &Vector<Kind>) -> Option<Message<'static>> {
    (!vector.expect.ops().contains(&Failure::ChangeCborTag)).then(|| {
        message(vector).unwrap_or_else(|error| panic!("{name}: {error:?}", name = vector.name))
    })
}

/// The message encoded as the example wrote it: tagged, or as its bare struct.
pub fn reencoded(vector: &Vector<Kind>) -> Result<Vec<u8>, Error> {
    let decoded = message(vector)?;
    if vector.expect.ops().contains(&Failure::RemoveCborTag) {
        match &decoded {
            Message::Sign(m) => encode(m),
            Message::Sign1(m) => encode(m),
            Message::Encrypt(m) => encode(m),
            Message::Encrypt0(m) => encode(m),
            Message::Mac(m) => encode(m),
            Message::Mac0(m) => encode(m),
        }
    } else {
        encode(&decoded)
    }
}

pub const fn kind(message: &Message) -> Kind {
    match message {
        Message::Sign(_) => Kind::Sign,
        Message::Sign1(_) => Kind::Sign1,
        Message::Encrypt(_) => Kind::Encrypt,
        Message::Encrypt0(_) => Kind::Encrypt0,
        Message::Mac(_) => Kind::Mac,
        Message::Mac0(_) => Kind::Mac0,
    }
}

/// One layer of a message: the message itself, a signer or a recipient, at the dotted path the
/// examples' intermediates use. `content` is what a countersignature of the layer signs.
pub struct Layer {
    pub at: String,
    pub protected: Protected<'static>,
    pub unprotected: HeaderMap<'static>,
    pub content: &'static [u8],
}

/// Every layer, the message first.
pub fn layers(message: &Message<'static>) -> Vec<Layer> {
    let (protected, unprotected, content) = match message {
        Message::Sign(m) => (m.protected, &m.unprotected, bytes(m.payload)),
        Message::Sign1(m) => (m.protected, &m.unprotected, bytes(m.payload)),
        Message::Encrypt(m) => (m.protected, &m.unprotected, bytes(m.ciphertext)),
        Message::Encrypt0(m) => (m.protected, &m.unprotected, bytes(m.ciphertext)),
        Message::Mac(m) => (m.protected, &m.unprotected, bytes(m.payload)),
        Message::Mac0(m) => (m.protected, &m.unprotected, bytes(m.payload)),
    };
    [Layer {
        at: String::new(),
        protected,
        unprotected: unprotected.clone(),
        content,
    }]
    .into_iter()
    .chain(match message {
        Message::Sign(m) => m
            .signatures
            .iter()
            .enumerate()
            .map(|(index, signer)| Layer {
                at: format!("signers.{index}"),
                protected: signer.protected,
                unprotected: signer.unprotected.clone(),
                content: signer.signature,
            })
            .collect(),
        Message::Sign1(_)
        | Message::Encrypt(_)
        | Message::Encrypt0(_)
        | Message::Mac(_)
        | Message::Mac0(_) => Vec::new(),
    })
    .chain(match message {
        Message::Encrypt(m) => recipient_layers("recipients", &m.recipients),
        Message::Mac(m) => recipient_layers("recipients", &m.recipients),
        Message::Sign(_) | Message::Sign1(_) | Message::Encrypt0(_) | Message::Mac0(_) => {
            Vec::new()
        }
    })
    .collect()
}

fn recipient_layers(at: &str, recipients: &[Recipient<'static, Recipient<'static>>]) -> Vec<Layer> {
    recipients
        .iter()
        .enumerate()
        .flat_map(|(index, recipient)| {
            let path = format!("{at}.{index}");
            let inner = recipient
                .recipients
                .as_deref()
                .unwrap_or_default()
                .iter()
                .enumerate()
                .map(|(inner_index, inner)| {
                    assert_eq!(
                        inner.recipients.as_deref().map(<[RawCbor]>::len),
                        None,
                        "{path}: recipients nest deeper than the types"
                    );
                    recipient_layer(format!("{path}.recipients.{inner_index}"), inner)
                })
                .collect::<Vec<_>>();
            [recipient_layer(path.clone(), recipient)]
                .into_iter()
                .chain(inner)
        })
        .collect()
}

fn recipient_layer<Inner>(at: String, recipient: &Recipient<'static, Inner>) -> Layer {
    Layer {
        at,
        protected: recipient.protected,
        unprotected: recipient.unprotected.clone(),
        content: bytes(recipient.ciphertext),
    }
}

const fn bytes(item: BytesOrNil<'static>) -> &'static [u8] {
    match item {
        BytesOrNil::Bytes(bytes) => bytes,
        BytesOrNil::Nil(()) => &[],
    }
}

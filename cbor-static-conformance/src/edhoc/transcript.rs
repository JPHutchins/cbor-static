use cbor_static::Error;

use super::identifier::{ConnectionId, Cred, IdCred};
use super::sequence::{Eads, Sequence, group, item, last_item, placed, put};

/// RFC 9528 §4.1.2's `info`, the input of `EDHOC_Expand`.
///
/// ```
/// use cbor_static_conformance::edhoc::{Info, Sequence};
///
/// let written = include_bytes!(concat!(env!("RFC_VECTORS"), "/rfc9529/section_3_9_info_for_keyupdate.cbor"));
/// let info = Info::from_sequence(written)?;
/// assert_eq!((info.label, info.context.len(), info.length), (11, 16, 32));
/// # Ok::<(), cbor_static::Error>(())
/// ```
#[derive(Debug, PartialEq, Eq)]
pub struct Info<'a> {
    pub label: i64,
    pub context: &'a [u8],
    pub length: u64,
}

impl<'a> Sequence<'a> for Info<'a> {
    fn decode_front(input: &'a [u8]) -> Result<(Self, &'a [u8]), Error> {
        let (label, rest) = item(input, input)?;
        let (context, rest) = item(input, rest)?;
        let (length, rest) = item(input, rest)?;
        Ok((
            Self {
                label,
                context,
                length,
            },
            rest,
        ))
    }

    fn encode_at(&self, out: &mut [u8], at: usize) -> Result<usize, Error> {
        put(&self.label, out, at)
            .and_then(|at| put(&self.context, out, at))
            .and_then(|at| put(&self.length, out, at))
    }
}

/// RFC 9528 §5.3.2's `context_2`, which `MAC_2` is computed over.
///
/// ```
/// use cbor_static_conformance::edhoc::{Context2, Cred, Sequence};
///
/// let written = include_bytes!(concat!(env!("RFC_VECTORS"), "/rfc9529/section_3_4_context_2.cbor"));
/// let context = Context2::from_sequence(written)?;
/// assert!(matches!(context.cred_r, Cred::Ccs(_)));
/// let mut out = [0u8; 256];
/// let len = context.to_sequence(&mut out)?;
/// assert_eq!(&out[..len], written);
/// # Ok::<(), cbor_static::Error>(())
/// ```
#[derive(Debug, PartialEq)]
pub struct Context2<'a> {
    pub c_r: ConnectionId<'a>,
    pub id_cred_r: IdCred<'a>,
    pub th_2: &'a [u8],
    pub cred_r: Cred<'a>,
    pub ead_2: Eads<'a>,
}

impl<'a> Sequence<'a> for Context2<'a> {
    fn decode_front(input: &'a [u8]) -> Result<(Self, &'a [u8]), Error> {
        let (c_r, rest) = item(input, input)?;
        let (id_cred_r, rest) = item(input, rest)?;
        let (th_2, rest) = item(input, rest)?;
        let (cred_r, rest) = item(input, rest)?;
        let (ead_2, rest) = group::<Eads>(input, rest)?;
        Ok((
            Self {
                c_r,
                id_cred_r,
                th_2,
                cred_r,
                ead_2,
            },
            rest,
        ))
    }

    fn encode_at(&self, out: &mut [u8], at: usize) -> Result<usize, Error> {
        put(&self.c_r, out, at)
            .and_then(|at| put(&self.id_cred_r, out, at))
            .and_then(|at| put(&self.th_2, out, at))
            .and_then(|at| put(&self.cred_r, out, at))
            .and_then(|at| self.ead_2.encode_at(out, at))
    }
}

/// RFC 9528 §5.4.2's `context_3`, which `MAC_3` is computed over.
///
/// ```
/// use cbor_static_conformance::edhoc::{Context3, Sequence};
///
/// let written = include_bytes!(concat!(env!("RFC_VECTORS"), "/rfc9529/section_3_5_context_3.cbor"));
/// let mut out = [0u8; 256];
/// let len = Context3::from_sequence(written)?.to_sequence(&mut out)?;
/// assert_eq!(&out[..len], written);
/// # Ok::<(), cbor_static::Error>(())
/// ```
#[derive(Debug, PartialEq)]
pub struct Context3<'a> {
    pub id_cred_i: IdCred<'a>,
    pub th_3: &'a [u8],
    pub cred_i: Cred<'a>,
    pub ead_3: Eads<'a>,
}

impl<'a> Sequence<'a> for Context3<'a> {
    fn decode_front(input: &'a [u8]) -> Result<(Self, &'a [u8]), Error> {
        let (id_cred_i, rest) = item(input, input)?;
        let (th_3, rest) = item(input, rest)?;
        let (cred_i, rest) = item(input, rest)?;
        let (ead_3, rest) = group::<Eads>(input, rest)?;
        Ok((
            Self {
                id_cred_i,
                th_3,
                cred_i,
                ead_3,
            },
            rest,
        ))
    }

    fn encode_at(&self, out: &mut [u8], at: usize) -> Result<usize, Error> {
        put(&self.id_cred_i, out, at)
            .and_then(|at| put(&self.th_3, out, at))
            .and_then(|at| put(&self.cred_i, out, at))
            .and_then(|at| self.ead_3.encode_at(out, at))
    }
}

/// RFC 9528 §5.3.2's input to `TH_2`: `G_Y` and `H(message_1)`.
///
/// ```
/// use cbor_static_conformance::edhoc::{Sequence, Th2Input};
///
/// let input = Th2Input::from_sequence(b"\x41\x01\x41\x02")?;
/// assert_eq!((input.g_y, input.hash_message_1), (b"\x01".as_slice(), b"\x02".as_slice()));
/// # Ok::<(), cbor_static::Error>(())
/// ```
#[derive(Debug, PartialEq, Eq)]
pub struct Th2Input<'a> {
    pub g_y: &'a [u8],
    pub hash_message_1: &'a [u8],
}

impl<'a> Sequence<'a> for Th2Input<'a> {
    fn decode_front(input: &'a [u8]) -> Result<(Self, &'a [u8]), Error> {
        let (g_y, rest) = item(input, input)?;
        let (hash_message_1, rest) = item(input, rest)?;
        Ok((
            Self {
                g_y,
                hash_message_1,
            },
            rest,
        ))
    }

    fn encode_at(&self, out: &mut [u8], at: usize) -> Result<usize, Error> {
        put(&self.g_y, out, at).and_then(|at| put(&self.hash_message_1, out, at))
    }
}

/// RFC 9528 §5.3.2 and §5.4.2's input to `TH_3` and `TH_4`: the previous transcript hash, the
/// previous plaintext's items, and the sender's credential. A plaintext's last EAD item may lack
/// its value, and a credential may be a byte string, so the credential is read as the last item
/// and the plaintext as everything before it; the group runs to the end of its input.
///
/// ```
/// use cbor_static_conformance::edhoc::{Cred, Ead, Plaintext3, Sequence, TranscriptInput};
///
/// let written = include_bytes!(concat!(env!("RFC_VECTORS"), "/rfc9529/section_3_5_input_to_calculate_th_4.cbor"));
/// let input = TranscriptInput::<Plaintext3>::from_sequence(written)?;
/// assert_eq!(input.th.len(), 32);
///
/// let valueless_ead_then_certificate = b"\x41\x00\x32\x41\x00\x01\x41\x30";
/// let input = TranscriptInput::<Plaintext3>::from_sequence(valueless_ead_then_certificate)?;
/// assert_eq!(
///     (input.plaintext.ead_3.as_slice(), input.cred),
///     ([Ead { label: 1, value: None }].as_slice(), Cred::Certificate(b"\x30"))
/// );
/// # Ok::<(), cbor_static::Error>(())
/// ```
#[derive(Debug, PartialEq)]
pub struct TranscriptInput<'a, Plaintext> {
    pub th: &'a [u8],
    pub plaintext: Plaintext,
    pub cred: Cred<'a>,
}

impl<'a, Plaintext: Sequence<'a>> Sequence<'a> for TranscriptInput<'a, Plaintext> {
    fn decode_front(input: &'a [u8]) -> Result<(Self, &'a [u8]), Error> {
        let (th, rest) = item(input, input)?;
        let (plaintext_items, cred_item) = rest.split_at(rest.len() - last_item(rest).len());
        let plaintext =
            Plaintext::from_sequence(plaintext_items).map_err(|e| placed(e, input, rest))?;
        let (cred, rest) = item(input, cred_item)?;
        Ok((
            Self {
                th,
                plaintext,
                cred,
            },
            rest,
        ))
    }

    fn encode_at(&self, out: &mut [u8], at: usize) -> Result<usize, Error> {
        put(&self.th, out, at)
            .and_then(|at| self.plaintext.encode_at(out, at))
            .and_then(|at| put(&self.cred, out, at))
    }
}

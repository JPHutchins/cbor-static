use cbor_static::{Cbor, Error, RawCbor};

use super::identifier::{ConnectionId, IdCred, Suites};
use super::sequence::{Eads, Sequence, group, item, put};

/// RFC 9528 §5.2.1's `message_1`.
///
/// ```
/// use cbor_static_conformance::edhoc::{ConnectionId, Message1, Sequence, Suites};
///
/// let written = include_bytes!(concat!(env!("RFC_VECTORS"), "/rfc9529/section_2_1_message_1.cbor"));
/// let message = Message1::from_sequence(written)?;
/// assert_eq!((message.method, &message.suites_i, message.c_i), (0, &Suites::One(0), ConnectionId::Int(-14)));
/// let mut out = [0u8; 64];
/// let len = message.to_sequence(&mut out)?;
/// assert_eq!(&out[..len], written);
/// # Ok::<(), cbor_static::Error>(())
/// ```
#[derive(Debug, PartialEq)]
pub struct Message1<'a> {
    pub method: i64,
    pub suites_i: Suites,
    pub g_x: &'a [u8],
    pub c_i: ConnectionId<'a>,
    pub ead_1: Eads<'a>,
}

impl<'a> Sequence<'a> for Message1<'a> {
    fn decode_front(input: &'a [u8]) -> Result<(Self, &'a [u8]), Error> {
        let (method, rest) = item(input, input)?;
        let (suites_i, rest) = item(input, rest)?;
        let (g_x, rest) = item(input, rest)?;
        let (c_i, rest) = item(input, rest)?;
        let (ead_1, rest) = group::<Eads>(input, rest)?;
        Ok((
            Self {
                method,
                suites_i,
                g_x,
                c_i,
                ead_1,
            },
            rest,
        ))
    }

    fn encode_at(&self, out: &mut [u8], at: usize) -> Result<usize, Error> {
        put(&self.method, out, at)
            .and_then(|at| put(&self.suites_i, out, at))
            .and_then(|at| put(&self.g_x, out, at))
            .and_then(|at| put(&self.c_i, out, at))
            .and_then(|at| self.ead_1.encode_at(out, at))
    }
}

/// RFC 9528 §5.3.1's `message_2`: `G_Y_CIPHERTEXT_2`, one byte string.
///
/// ```
/// use cbor_static_conformance::edhoc::Message2;
///
/// assert_eq!(cbor_static::from_slice::<Message2>(b"\x42\x01\x02")?, Message2(b"\x01\x02"));
/// # Ok::<(), cbor_static::Error>(())
/// ```
#[derive(Cbor, Debug, PartialEq, Eq)]
pub struct Message2<'a>(pub &'a [u8]);

/// RFC 9528 §5.4.1's `message_3`: `CIPHERTEXT_3`, one byte string.
///
/// ```
/// use cbor_static_conformance::edhoc::Message3;
///
/// assert_eq!(cbor_static::from_slice::<Message3>(b"\x41\x01")?, Message3(b"\x01"));
/// # Ok::<(), cbor_static::Error>(())
/// ```
#[derive(Cbor, Debug, PartialEq, Eq)]
pub struct Message3<'a>(pub &'a [u8]);

/// RFC 9528 §5.5.1's `message_4`: `CIPHERTEXT_4`, one byte string.
///
/// ```
/// use cbor_static_conformance::edhoc::Message4;
///
/// assert_eq!(cbor_static::from_slice::<Message4>(b"\x40")?, Message4(b""));
/// # Ok::<(), cbor_static::Error>(())
/// ```
#[derive(Cbor, Debug, PartialEq, Eq)]
pub struct Message4<'a>(pub &'a [u8]);

/// RFC 9528 §5.3.2's `PLAINTEXT_2`.
///
/// ```
/// use cbor_static_conformance::edhoc::{ConnectionId, IdCred, Plaintext2, Sequence};
///
/// let plaintext = Plaintext2::from_sequence(b"\x27\x32\x41\x00")?;
/// assert_eq!((plaintext.c_r, plaintext.id_cred_r), (ConnectionId::Int(-8), IdCred::Int(-19)));
/// assert!(plaintext.ead_2.is_empty());
/// # Ok::<(), cbor_static::Error>(())
/// ```
#[derive(Debug, PartialEq)]
pub struct Plaintext2<'a> {
    pub c_r: ConnectionId<'a>,
    pub id_cred_r: IdCred<'a>,
    pub signature_or_mac_2: &'a [u8],
    pub ead_2: Eads<'a>,
}

impl<'a> Sequence<'a> for Plaintext2<'a> {
    fn decode_front(input: &'a [u8]) -> Result<(Self, &'a [u8]), Error> {
        let (c_r, rest) = item(input, input)?;
        let (id_cred_r, rest) = item(input, rest)?;
        let (signature_or_mac_2, rest) = item(input, rest)?;
        let (ead_2, rest) = group::<Eads>(input, rest)?;
        Ok((
            Self {
                c_r,
                id_cred_r,
                signature_or_mac_2,
                ead_2,
            },
            rest,
        ))
    }

    fn encode_at(&self, out: &mut [u8], at: usize) -> Result<usize, Error> {
        put(&self.c_r, out, at)
            .and_then(|at| put(&self.id_cred_r, out, at))
            .and_then(|at| put(&self.signature_or_mac_2, out, at))
            .and_then(|at| self.ead_2.encode_at(out, at))
    }
}

/// RFC 9528 §5.4.2's `PLAINTEXT_3`.
///
/// ```
/// use cbor_static_conformance::edhoc::{IdCred, Plaintext3, Sequence};
///
/// let plaintext = Plaintext3::from_sequence(b"\x2b\x41\x00")?;
/// assert_eq!((plaintext.id_cred_i, plaintext.signature_or_mac_3), (IdCred::Int(-12), b"\x00".as_slice()));
/// # Ok::<(), cbor_static::Error>(())
/// ```
#[derive(Debug, PartialEq)]
pub struct Plaintext3<'a> {
    pub id_cred_i: IdCred<'a>,
    pub signature_or_mac_3: &'a [u8],
    pub ead_3: Eads<'a>,
}

impl<'a> Sequence<'a> for Plaintext3<'a> {
    fn decode_front(input: &'a [u8]) -> Result<(Self, &'a [u8]), Error> {
        let (id_cred_i, rest) = item(input, input)?;
        let (signature_or_mac_3, rest) = item(input, rest)?;
        let (ead_3, rest) = group::<Eads>(input, rest)?;
        Ok((
            Self {
                id_cred_i,
                signature_or_mac_3,
                ead_3,
            },
            rest,
        ))
    }

    fn encode_at(&self, out: &mut [u8], at: usize) -> Result<usize, Error> {
        put(&self.id_cred_i, out, at)
            .and_then(|at| put(&self.signature_or_mac_3, out, at))
            .and_then(|at| self.ead_3.encode_at(out, at))
    }
}

/// RFC 9528 §5.5.2's `PLAINTEXT_4`: its EAD items, if any.
///
/// ```
/// use cbor_static_conformance::edhoc::{Plaintext4, Sequence};
///
/// assert!(Plaintext4::from_sequence(b"")?.ead_4.is_empty());
/// # Ok::<(), cbor_static::Error>(())
/// ```
#[derive(Debug, PartialEq)]
pub struct Plaintext4<'a> {
    pub ead_4: Eads<'a>,
}

impl<'a> Sequence<'a> for Plaintext4<'a> {
    fn decode_front(input: &'a [u8]) -> Result<(Self, &'a [u8]), Error> {
        group::<Eads>(input, input).map(|(ead_4, rest)| (Self { ead_4 }, rest))
    }

    fn encode_at(&self, out: &mut [u8], at: usize) -> Result<usize, Error> {
        self.ead_4.encode_at(out, at)
    }
}

/// RFC 9528 §6's `error`: a code, and information whose type the code sets.
///
/// ```
/// use cbor_static_conformance::edhoc::{ErrorMessage, Sequence};
///
/// let error = ErrorMessage::from_sequence(b"\x02\x02")?;
/// assert_eq!((error.err_code, error.err_info.as_bytes()), (2, b"\x02".as_slice()));
/// # Ok::<(), cbor_static::Error>(())
/// ```
#[derive(Debug, PartialEq)]
pub struct ErrorMessage<'a> {
    pub err_code: i64,
    pub err_info: RawCbor<'a>,
}

impl<'a> Sequence<'a> for ErrorMessage<'a> {
    fn decode_front(input: &'a [u8]) -> Result<(Self, &'a [u8]), Error> {
        let (err_code, rest) = item(input, input)?;
        let (err_info, rest) = item(input, rest)?;
        Ok((Self { err_code, err_info }, rest))
    }

    fn encode_at(&self, out: &mut [u8], at: usize) -> Result<usize, Error> {
        put(&self.err_code, out, at).and_then(|at| put(&self.err_info, out, at))
    }
}

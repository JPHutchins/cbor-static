#![forbid(unsafe_code)]
//! RFC 8949 §3 initial bytes and arguments.

pub mod float;

use crate::error::ErrorKind;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum Major {
    Uint,
    Nint,
    Bytes,
    Text,
    Array,
    Map,
    Tag,
    Simple,
}

pub const INDEFINITE: u8 = 31;
pub const FALSE: u8 = 20;
pub const TRUE: u8 = 21;
pub const NULL: u8 = 22;
pub const UNDEFINED: u8 = 23;
/// The self-describe tag (RFC 8949 §3.4.6): `d9 d9f7`.
pub const SELF_DESCRIBE: u64 = 55_799;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Header {
    pub major: Major,
    pub info: u8,
    pub arg: u64,
}

/// An encoded header: at most 9 bytes.
#[derive(Clone, Copy)]
pub struct Head {
    bytes: [u8; 9],
    len: u8,
}

impl Major {
    const fn of(initial: u8) -> Self {
        match initial >> 5 {
            0 => Self::Uint,
            1 => Self::Nint,
            2 => Self::Bytes,
            3 => Self::Text,
            4 => Self::Array,
            5 => Self::Map,
            6 => Self::Tag,
            _ => Self::Simple,
        }
    }
}

impl Header {
    pub const fn indefinite(self) -> bool {
        self.info == INDEFINITE
    }

    pub const fn is_break(self) -> bool {
        matches!(self.major, Major::Simple) && self.indefinite()
    }

    /// The initial byte of the head it was read from.
    pub const fn initial(self) -> u8 {
        (self.major as u8) << 5 | self.info
    }

    /// The length of the head it was read from, which its additional information fixes.
    pub const fn head_len(self) -> Result<usize, ErrorKind> {
        header_len(self.initial())
    }
}

impl Head {
    /// Preferred serialization (RFC 8949 §4.1) of `major` with argument `arg`.
    pub const fn new(major: Major, arg: u64) -> Self {
        let initial = (major as u8) << 5;
        let be = arg.to_be_bytes();
        match arg {
            0..24 => Self::from([initial | be[7], 0, 0, 0, 0, 0, 0, 0, 0], 1),
            24..0x100 => Self::from([initial | 0x18, be[7], 0, 0, 0, 0, 0, 0, 0], 2),
            0x100..0x1_0000 => Self::from([initial | 0x19, be[6], be[7], 0, 0, 0, 0, 0, 0], 3),
            0x1_0000..0x1_0000_0000 => {
                Self::from([initial | 0x1a, be[4], be[5], be[6], be[7], 0, 0, 0, 0], 5)
            }
            _ => Self::from(
                [
                    initial | 0x1b,
                    be[0],
                    be[1],
                    be[2],
                    be[3],
                    be[4],
                    be[5],
                    be[6],
                    be[7],
                ],
                9,
            ),
        }
    }

    pub const fn float((info, bits): (u8, u64)) -> Self {
        let be = bits.to_be_bytes();
        let initial = (Major::Simple as u8) << 5 | info;
        match info {
            25 => Self::from([initial, be[6], be[7], 0, 0, 0, 0, 0, 0], 3),
            26 => Self::from([initial, be[4], be[5], be[6], be[7], 0, 0, 0, 0], 5),
            _ => Self::from(
                [
                    initial, be[0], be[1], be[2], be[3], be[4], be[5], be[6], be[7],
                ],
                9,
            ),
        }
    }

    const fn from(bytes: [u8; 9], len: u8) -> Self {
        Self { bytes, len }
    }

    pub const fn as_bytes(&self) -> &[u8] {
        match self.bytes.split_at_checked(self.len as usize) {
            Some((head, _)) => head,
            None => &[],
        }
    }
}

/// A wire length or count as `usize`. Under `--cfg cbor_static_usize16` it is capped at
/// `u16::MAX`, to test a 16-bit target's limit on the host.
///
/// # Errors
/// [`ErrorKind::LimitExceeded`] when `len` is beyond that limit (RFC 8949 §5.1).
pub fn length(len: u64) -> Result<usize, ErrorKind> {
    if cfg!(cbor_static_usize16) {
        u16::try_from(len).map(usize::from)
    } else {
        usize::try_from(len)
    }
    .map_err(|_| ErrorKind::LimitExceeded)
}

/// Length of the header that starts with `initial`.
///
/// # Errors
/// [`ErrorKind::NotWellFormed`] for the reserved additional information 28–30.
pub const fn header_len(initial: u8) -> Result<usize, ErrorKind> {
    match initial & 0x1f {
        0..24 | INDEFINITE => Ok(1),
        24 => Ok(2),
        25 => Ok(3),
        26 => Ok(5),
        27 => Ok(9),
        _ => Err(ErrorKind::NotWellFormed),
    }
}

/// Parses a complete header of exactly [`header_len`] bytes, enforcing RFC 8949 Appendix F.
///
/// # Errors
/// [`ErrorKind::NotWellFormed`] for reserved or misplaced additional information, and for
/// two-byte simple values below 32.
pub fn parse(bytes: &[u8]) -> Result<Header, ErrorKind> {
    let (&initial, rest) = bytes.split_first().ok_or(ErrorKind::UnexpectedEnd)?;
    let major = Major::of(initial);
    let info = initial & 0x1f;
    let arg = match info {
        0..24 => u64::from(info),
        INDEFINITE => 0,
        _ => rest.iter().fold(0, |acc, &b| acc << 8 | u64::from(b)),
    };
    match (major, info) {
        (_, 28..31) | (Major::Uint | Major::Nint | Major::Tag, INDEFINITE) => {
            Err(ErrorKind::NotWellFormed)
        }
        (Major::Simple, 24) if arg < 32 => Err(ErrorKind::NotWellFormed),
        _ => Ok(Header { major, info, arg }),
    }
}

#[cfg(test)]
mod tests {
    use super::{Head, Header, Major, header_len, length, parse};
    use crate::error::ErrorKind;

    #[test]
    fn heads_use_the_shortest_argument() {
        let cases: [(Major, u64, &[u8]); 7] = [
            (Major::Uint, 0, &[0x00]),
            (Major::Uint, 23, &[0x17]),
            (Major::Uint, 24, &[0x18, 0x18]),
            (Major::Nint, 0xff, &[0x38, 0xff]),
            (Major::Text, 0x100, &[0x79, 0x01, 0x00]),
            (Major::Map, 0x1_0000, &[0xba, 0, 1, 0, 0]),
            (
                Major::Uint,
                u64::MAX,
                &[0x1b, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff],
            ),
        ];
        for (major, arg, expected) in cases {
            assert_eq!(Head::new(major, arg).as_bytes(), expected);
            assert_eq!(
                parse(expected),
                Ok(Header {
                    major,
                    info: expected[0] & 0x1f,
                    arg
                })
            );
        }
    }

    #[test]
    fn appendix_f_headers_are_rejected() {
        for bad in [
            [0x1c].as_slice(),
            &[0x3d],
            &[0xfe],
            &[0x1f],
            &[0x3f],
            &[0xdf],
            &[0xf8, 0x1f],
        ] {
            let parsed = header_len(bad[0]).and_then(|_| parse(bad));
            assert_eq!(parsed, Err(ErrorKind::NotWellFormed), "{bad:x?}");
        }
        assert_eq!(parse(&[0xf8, 0x20]).map(|h| h.arg), Ok(32));
    }

    #[test]
    fn lengths_beyond_the_target_are_a_resource_limit() {
        assert_eq!(length(0), Ok(0));
        assert_eq!(length(65_535), Ok(65_535));
        assert_eq!(
            length(65_536),
            if cfg!(cbor_static_usize16) {
                Err(ErrorKind::LimitExceeded)
            } else {
                Ok(65_536)
            }
        );
        assert_eq!(
            length(u64::MAX).is_ok(),
            !cfg!(cbor_static_usize16) && usize::BITS == u64::BITS
        );
    }
}

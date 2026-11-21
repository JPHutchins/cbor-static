//! EDHOC (RFC 9528): its messages, plaintexts, error message, credentials, and the sequences its
//! keys and MACs are derived from. A group of several items is a [`Sequence`]; a message of one
//! byte string is a newtype. No cryptography and no protocol state.
//!
//! ```
//! use cbor_static_conformance::edhoc::{Cred, Plaintext2, Sequence, TranscriptInput};
//!
//! let written = include_bytes!(concat!(env!("RFC_VECTORS"), "/rfc9529/section_2_3_input_to_calculate_th_3.cbor"));
//! let input = TranscriptInput::<Plaintext2>::from_sequence(written)?;
//! assert!(matches!(input.cred, Cred::Certificate(_)));
//! let mut out = [0u8; 512];
//! let len = input.to_sequence(&mut out)?;
//! assert_eq!(&out[..len], written);
//! # Ok::<(), cbor_static::Error>(())
//! ```

mod identifier;
mod message;
mod sequence;
mod transcript;

pub use identifier::{ConnectionId, Cred, IdCred, Suites};
pub use message::{
    ErrorMessage, Message1, Message2, Message3, Message4, Plaintext2, Plaintext3, Plaintext4,
};
pub use sequence::{Ead, Eads, Sequence};
pub use transcript::{Context2, Context3, Info, Th2Input, TranscriptInput};

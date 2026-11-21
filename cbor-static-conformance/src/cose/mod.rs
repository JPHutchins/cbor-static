//! COSE (RFC 9052, RFC 9053): the six messages, their headers, keys, and the structures their
//! signatures, MACs and keys are computed over. No cryptography.
//!
//! A tagged message decodes through [`Message`]; an untagged one, whose type the application
//! knows, as its bare struct.
//!
//! ```
//! use cbor_static_conformance::cose::{Message, Sign1};
//!
//! let tagged = include_bytes!(concat!(env!("COSE_WG_VECTORS"), "/RFC8152/Appendix_C_2_1/output.cbor"));
//! let message: Message = cbor_static::from_slice(tagged)?;
//! let Message::Sign1(sign1) = message else { panic!("RFC 9052 C.2.1 is a COSE_Sign1") };
//!
//! let untagged: Sign1 = cbor_static::from_slice(&tagged[1..])?;
//! assert_eq!(sign1, untagged);
//! # Ok::<(), cbor_static::Error>(())
//! ```

mod header;
mod key;
mod message;
mod structure;

pub use header::{ContentType, HeaderMap, Headers, Label, Protected};
pub use key::{Key, KeySet};
pub use message::{
    BytesOrNil, Encrypt, Encrypt0, Mac, Mac0, Message, Recipient, Sign, Sign1, Signature,
};
pub use structure::{
    EncContext, EncStructure, KdfContext, MacContext, MacStructure, Nonce, PartyInfo, SigContext,
    SigStructure, SigStructure1, SuppPubInfo,
};

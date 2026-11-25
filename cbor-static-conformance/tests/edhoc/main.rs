//! EDHOC against RFC 9529's traces (<https://www.rfc-editor.org/rfc/rfc9529>): section 2's
//! signature trace, section 3's static Diffie-Hellman trace, and section 4's invalid items, each
//! CBOR item's hex extracted from the RFC's XML by `nix/rfc-vectors.nix`. The RFC is published
//! under the IETF Trust's Legal Provisions (<https://trustee.ietf.org/license-info>).

macro_rules! rfc9529 {
    ($item:ident) => {
        include_bytes!(concat!(
            env!("RFC_VECTORS"),
            "/rfc9529/",
            stringify!($item),
            ".cbor"
        ))
        .as_slice()
    };
}

#[path = "../common/mod.rs"]
mod common;

mod constructs;
#[cfg(not(miri))]
mod differential;
mod invalid;
mod items;
#[cfg(not(miri))]
mod oracle;
mod traces;

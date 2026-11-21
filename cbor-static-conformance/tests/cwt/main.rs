//! CWT against RFC 8392 Appendix A (<https://www.rfc-editor.org/rfc/rfc8392#appendix-A>): each
//! example's hex, extracted from the RFC's text by `nix/rfc-vectors.nix`. The RFC is published
//! under the IETF Trust's Legal Provisions (<https://trustee.ietf.org/license-info>).

macro_rules! rfc8392 {
    ($example:ident) => {
        include_bytes!(concat!(
            env!("RFC_VECTORS"),
            "/rfc8392/",
            stringify!($example),
            ".cbor"
        ))
        .as_slice()
    };
}

#[path = "../common/mod.rs"]
mod common;

mod constructs;
mod examples;
mod keys;
#[cfg(not(miri))]
mod oracle;

#![no_std]
#![forbid(unsafe_code)]
//! Protocol types that hold cbor-static to the published vectors of the protocols it claims:
//! their messages decode and re-encode byte for byte, and their CDDL accepts what they
//! write.

pub mod cose;
pub mod cwt;
pub mod edhoc;

#![no_std]
#![forbid(unsafe_code)]
//! Protocol types that hold cbor-static to the published vectors of the protocols it claims:
//! their messages decode and re-encode byte for byte. The RFCs' CDDL accepts what the COSE, CWT
//! and EDHOC types write, and JPHutchins/smp reads what the SMP types write.

pub mod cose;
pub mod cwt;
pub mod edhoc;
pub mod smp;

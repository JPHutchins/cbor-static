//! SMP's OS and Image groups against JPHutchins/smp's binary regression records, Zephyr's `MCUmgr`
//! firmware responses, and hand-built vectors, as `nix/smp-vectors.nix` extracts them. The
//! sources, revisions and licenses are cited on `cbor_static_conformance::smp`.

macro_rules! twin {
    ($path:literal) => {
        include_bytes!(concat!(env!("SMP_VECTORS"), "/", $path, ".cbor")).as_slice()
    };
}

#[path = "../common/mod.rs"]
mod common;

mod constructs;
#[cfg(not(miri))]
mod differential;
mod hand_built;
mod records;
mod smp_only;
mod zephyr;

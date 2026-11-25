#![allow(dead_code)]

#[cfg(not(miri))]
pub mod differential;
pub mod vectors;

use cbor_static::{Decode, Encode, Error};

/// Room for the largest vector, an SMP image upload chunk.
pub const ENCODED_CAPACITY: usize = 64 * 1024;

pub fn reencode<'a, T: Decode<'a> + Encode>(input: &'a [u8]) -> Result<std::vec::Vec<u8>, Error> {
    encode(&cbor_static::from_slice::<T>(input)?)
}

pub fn encode<T: Encode>(value: &T) -> Result<std::vec::Vec<u8>, Error> {
    let mut out = std::vec![0u8; ENCODED_CAPACITY];
    let len = cbor_static::to_slice(value, &mut out)?;
    out.truncate(len);
    Ok(out)
}

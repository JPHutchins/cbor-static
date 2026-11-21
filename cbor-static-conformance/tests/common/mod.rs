#![allow(dead_code)]

pub mod vectors;

use cbor_static::{Decode, Encode, Error};

pub fn reencode<'a, T: Decode<'a> + Encode>(input: &'a [u8]) -> Result<std::vec::Vec<u8>, Error> {
    encode(&cbor_static::from_slice::<T>(input)?)
}

pub fn encode<T: Encode>(value: &T) -> Result<std::vec::Vec<u8>, Error> {
    let mut out = std::vec![0u8; 4096];
    let len = cbor_static::to_slice(value, &mut out)?;
    out.truncate(len);
    Ok(out)
}

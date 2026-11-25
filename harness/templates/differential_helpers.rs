#![allow(dead_code)]

pub fn stat<T: cbor_static::Encode>(value: &T) -> Vec<u8> {
    let mut buf = [0u8; 1024];
    let len = cbor_static::to_slice(value, &mut buf).expect("cbor-static encodes");
    buf[..len].to_vec()
}

pub fn serde_layer<T: serde::Serialize>(value: &T) -> Vec<u8> {
    let mut buf = [0u8; 1024];
    let len = cbor_static::serde::to_slice(value, &mut buf).expect("the serde layer encodes");
    buf[..len].to_vec()
}

pub fn ciborium_bytes<T: serde::Serialize>(value: &T) -> Vec<u8> {
    let mut written = Vec::new();
    ciborium::into_writer(value, &mut written).expect("ciborium serializes");
    written
}

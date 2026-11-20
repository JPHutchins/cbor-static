#[derive(cbor_static::Cbor, serde::Serialize, serde::Deserialize, Debug, PartialEq)]
struct Frame<'a> {
    #[serde(with = "serde_bytes")]
    id: [u8; 2],
    #[serde(with = "serde_bytes")]
    payload: &'a [u8],
    #[serde(with = "serde_bytes")]
    owned: Vec<u8>,
}

fn main() {
    let frame = Frame {
        id: [1, 2],
        payload: b"ab",
        owned: vec![3],
    };
    let native = cbor_static::to_vec(&frame).unwrap();
    assert_eq!(cbor_static::serde::to_vec(&frame).unwrap(), native);
    assert_eq!(
        native,
        b"\xa3\x62id\x42\x01\x02\x67payload\x42ab\x65owned\x41\x03"
    );
    assert_eq!(cbor_static::serde::from_slice::<Frame>(&native).unwrap(), frame);
}

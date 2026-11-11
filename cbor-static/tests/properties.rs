//! R-101: preferred serialization (RFC 8949 §4.1). Encoding then decoding is the identity, heads
//! take the shortest argument, and floats the narrowest exact width down to binary16.

mod common;

use std::collections::HashMap;
use std::mem::MaybeUninit;
use std::sync::LazyLock;

use cbor_static::{
    Decode, Encode, ErrorKind, Token, TokenWriter, Tokens, decode_into, encode_into,
};
use common::{float_bits, float_encoded, halves};
use proptest::prelude::*;

static HALVES: LazyLock<HashMap<u64, u16>> = LazyLock::new(halves);

fn config() -> ProptestConfig {
    ProptestConfig {
        cases: if cfg!(miri) { 8 } else { 1024 },
        failure_persistence: None,
        ..ProptestConfig::default()
    }
}

/// Binary64 bits that are often exactly binary16 or binary32, NaNs included.
fn float_bits_strategy() -> impl Strategy<Value = u64> {
    prop_oneof![
        any::<u64>(),
        any::<u32>().prop_map(|single| f64::from(f32::from_bits(single)).to_bits()),
        any::<u16>().prop_map(common::ieee_binary16),
    ]
}

/// The length RFC 8949 §3.1 gives the head of argument `arg`.
fn head_len(arg: u64) -> usize {
    match arg {
        0..24 => 1,
        24..0x100 => 2,
        0x100..0x1_0000 => 3,
        0x1_0000..0x1_0000_0000 => 5,
        _ => 9,
    }
}

/// The width an exact float of binary64 `bits` takes: 3 for binary16, 5 for binary32, else 9.
#[expect(
    clippy::cast_possible_truncation,
    reason = "the host's own narrowing, checked exact by widening back"
)]
fn float_len(bits: u64) -> usize {
    let single_exact = if f64::from_bits(bits).is_nan() {
        bits & ((1 << 29) - 1) == 0
    } else {
        f64::from(f64::from_bits(bits) as f32).to_bits() == bits
    };
    match (HALVES.contains_key(&bits), single_exact) {
        (true, _) => 3,
        (false, true) => 5,
        (false, false) => 9,
    }
}

fn written(token: Token) -> Vec<u8> {
    let mut out = [0u8; 9];
    let mut writer = TokenWriter::new(&mut out);
    writer.write(token).expect("fits");
    let len = writer.written();
    out[..len].to_vec()
}

fn tokens(bytes: &[u8]) -> Vec<Token<'_>> {
    Tokens::new(bytes)
        .collect::<Result<_, _>>()
        .expect("well-formed")
}

fn round_trip<T: Encode + for<'de> Decode<'de> + Copy>(value: T) -> (Vec<u8>, T) {
    let mut out = [0u8; 160];
    let len = encode_into(&value, &mut out).expect("encodes");
    let mut slot = MaybeUninit::uninit();
    let decoded = *decode_into::<T>(&out[..len], &mut slot).expect("decodes");
    (out[..len].to_vec(), decoded)
}

proptest! {
    #![proptest_config(config())]

    #[test]
    fn heads_take_the_shortest_argument(arg in any::<u64>()) {
        for token in [
            Token::Unsigned(arg),
            Token::Negative(arg),
            Token::Tag(arg),
            Token::Array(Some(arg)),
            Token::Map(Some(arg)),
        ] {
            let encoded = written(token);
            prop_assert_eq!(encoded.len(), head_len(arg));
            prop_assert_eq!(
                Tokens::new(&encoded).next().map(|t| t.map_err(|e| e.kind)),
                Some(if matches!(token, Token::Map(_)) && arg > u64::MAX / 2 {
                    Err(ErrorKind::LimitExceeded)
                } else {
                    Ok(token)
                })
            );
        }
    }

    #[test]
    fn floats_round_trip_bitwise_at_their_narrowest_width(bits in float_bits_strategy()) {
        let (out, len) = float_encoded(bits);
        prop_assert_eq!(len, float_len(bits));
        prop_assert_eq!(float_bits(&out[..len]), bits);
    }

    #[test]
    fn strings_round_trip_with_a_shortest_length(text in ".{0,300}", bytes in proptest::collection::vec(any::<u8>(), 0..300)) {
        let encoded = written_all(&[Token::Text(&text), Token::Bytes(&bytes)]);
        prop_assert_eq!(tokens(&encoded), [Token::Text(&text), Token::Bytes(&bytes)]);
        prop_assert_eq!(encoded.len(), head_len(text.len() as u64) + text.len() + head_len(bytes.len() as u64) + bytes.len());
    }

    #[test]
    fn typed_integers_round_trip_in_their_preferred_encoding(u in any::<u64>(), i in any::<i32>(), wide in any::<u128>(), signed in any::<i128>()) {
        let (encoded, decoded) = round_trip(u);
        prop_assert_eq!(decoded, u);
        prop_assert_eq!(encoded, written(Token::Unsigned(u)));
        let (encoded, decoded) = round_trip(i);
        prop_assert_eq!(decoded, i);
        prop_assert_eq!(encoded.len(), head_len(u64::try_from(if i < 0 { !i } else { i }).expect("non-negative")));
        prop_assert_eq!(round_trip(wide).1, wide);
        prop_assert_eq!(round_trip(signed).1, signed);
    }

    #[test]
    fn typed_floats_round_trip_bitwise_at_their_narrowest_width(bits in float_bits_strategy(), single in any::<u32>()) {
        let (encoded, decoded) = round_trip(f64::from_bits(bits));
        prop_assert_eq!(decoded.to_bits(), bits);
        prop_assert_eq!(encoded.len(), float_len(bits));
        let (encoded, decoded) = round_trip(f32::from_bits(single));
        prop_assert_eq!(decoded.to_bits(), single);
        prop_assert_eq!(encoded.len(), float_len(float_bits(&encoded)));
    }
}

fn written_all(tokens: &[Token]) -> Vec<u8> {
    let mut out = vec![0u8; 1024];
    let mut writer = TokenWriter::new(&mut out);
    for &token in tokens {
        writer.write(token).expect("fits");
    }
    let len = writer.written();
    out.truncate(len);
    out
}

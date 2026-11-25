//! Lowercase hex: the oracle child's wire form of an item, and the tests' way of writing one.

use serde::{Deserialize, Deserializer, Serializer};

pub fn encoded(bytes: &[u8]) -> String {
    use std::fmt::Write;

    bytes.iter().fold(String::new(), |mut hex, byte| {
        write!(hex, "{byte:02x}").expect("a String takes any write");
        hex
    })
}

pub fn decoded(text: &str) -> Option<Vec<u8>> {
    (0..text.len())
        .step_by(2)
        .map(|at| {
            text.get(at..at + 2)
                .and_then(|pair| u8::from_str_radix(pair, 16).ok())
        })
        .collect()
}

pub fn serialize<S: Serializer>(bytes: &[u8], serializer: S) -> Result<S::Ok, S::Error> {
    serializer.serialize_str(&encoded(bytes))
}

pub fn deserialize<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Vec<u8>, D::Error> {
    let text = <&str>::deserialize(deserializer)?;
    decoded(text).ok_or_else(|| serde::de::Error::custom(format!("not hex: {text:?}")))
}

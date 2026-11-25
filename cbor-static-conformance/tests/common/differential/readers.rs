//! Each reader's view of one item as `Data`: cbor-static's `Tokens` and minicbor's Tokenizer
//! through one tree builder, and ciborium's `Value`.

use super::{Data, Float, Length};

/// A head, or a whole string, as a token reader yields it.
enum Head {
    Integer(i128),
    Bytes(Vec<u8>),
    Text(String),
    BytesStart,
    TextStart,
    Array(Option<u64>),
    Map(Option<u64>),
    Tag(u64),
    Simple(u8),
    Bool(bool),
    Null,
    Undefined,
    Float(Float),
    Break,
}

/// The one item `bytes` holds, as cbor-static's `Tokens` reads it.
pub fn tokens(bytes: &[u8]) -> Result<Data, String> {
    use cbor_static::{Token, Tokens};

    item_of(Tokens::new(bytes).map(|token| {
        token
            .map(|token| match token {
                Token::Unsigned(value) => Head::Integer(value.into()),
                Token::Negative(value) => Head::Integer(-1 - i128::from(value)),
                Token::Bytes(bytes) => Head::Bytes(bytes.to_vec()),
                Token::Text(text) => Head::Text(text.into()),
                Token::BytesIndefinite => Head::BytesStart,
                Token::TextIndefinite => Head::TextStart,
                Token::Array(len) => Head::Array(len),
                Token::Map(len) => Head::Map(len),
                Token::Tag(tag) => Head::Tag(tag),
                Token::Simple(value) => Head::Simple(value),
                Token::Bool(value) => Head::Bool(value),
                Token::Null => Head::Null,
                Token::Undefined => Head::Undefined,
                Token::Float(value) => Head::Float(Float::read(value)),
                Token::Break => Head::Break,
            })
            .map_err(|error| format!("{error:?}"))
    }))
}

/// The one item `bytes` holds, as minicbor's Tokenizer reads it. Its binary16 NaNs are any NaN:
/// `half` quiets them on the way to binary32.
pub fn minicbor(bytes: &[u8]) -> Result<Data, String> {
    use minicbor::data::Token;

    item_of(minicbor::decode::Tokenizer::new(bytes).map(|token| {
        token
            .map(|token| match token {
                Token::Bool(value) => Head::Bool(value),
                Token::U8(value) => Head::Integer(value.into()),
                Token::U16(value) => Head::Integer(value.into()),
                Token::U32(value) => Head::Integer(value.into()),
                Token::U64(value) => Head::Integer(value.into()),
                Token::I8(value) => Head::Integer(value.into()),
                Token::I16(value) => Head::Integer(value.into()),
                Token::I32(value) => Head::Integer(value.into()),
                Token::I64(value) => Head::Integer(value.into()),
                Token::Int(value) => Head::Integer(value.into()),
                Token::F16(value) => Head::Float(Float::lossy_nan(value.into())),
                Token::F32(value) => Head::Float(widened(value)),
                Token::F64(value) => Head::Float(Float::read(value)),
                Token::Bytes(bytes) => Head::Bytes(bytes.to_vec()),
                Token::String(text) => Head::Text(text.into()),
                Token::Array(len) => Head::Array(Some(len)),
                Token::Map(len) => Head::Map(Some(len)),
                Token::Tag(tag) => Head::Tag(tag.into()),
                Token::Simple(value) => Head::Simple(value),
                Token::Break => Head::Break,
                Token::Null => Head::Null,
                Token::Undefined => Head::Undefined,
                Token::BeginBytes => Head::BytesStart,
                Token::BeginString => Head::TextStart,
                Token::BeginArray => Head::Array(None),
                Token::BeginMap => Head::Map(None),
            })
            .map_err(|error| error.to_string())
    }))
}

/// The one item `bytes` holds, as ciborium's `Value`. Its NaNs are any NaN: ciborium widens
/// through the FPU, which quiets a signalling NaN; that is ciborium's behaviour, not a finding.
pub fn ciborium(bytes: &[u8]) -> Result<Data, String> {
    ciborium::from_reader::<ciborium::Value, _>(bytes)
        .map(from_value)
        .map_err(|error| error.to_string())
}

fn from_value(value: ciborium::Value) -> Data {
    use ciborium::Value;

    match value {
        Value::Integer(value) => Data::Integer(value.into()),
        Value::Bytes(bytes) => Data::Bytes(bytes),
        Value::Float(value) => Data::Float(Float::lossy_nan(value)),
        Value::Text(text) => Data::Text(text),
        Value::Bool(value) => Data::Bool(value),
        Value::Null => Data::Null,
        Value::Tag(tag, content) => Data::Tag(tag, Box::new(from_value(*content))),
        Value::Array(items) => Data::Array(
            items.into_iter().map(from_value).collect(),
            Length::Definite,
        ),
        Value::Map(entries) => Data::Map(
            entries
                .into_iter()
                .map(|(key, value)| (from_value(key), from_value(value)))
                .collect(),
            Length::Definite,
        ),
        other => panic!("ciborium::Value has a variant this reader does not know: {other:?}"),
    }
}

/// A binary32 widened to binary64 bit for bit, NaN payload and all, as `Tokens` widens it.
fn widened(value: f32) -> Float {
    if value.is_nan() {
        let bits = u64::from(value.to_bits());
        Float::Bits((bits & 0x8000_0000) << 32 | 0x7ff0_0000_0000_0000 | (bits & 0x007f_ffff) << 29)
    } else {
        Float::read(value.into())
    }
}

/// The one item a token stream holds, and nothing after it.
fn item_of(mut heads: impl Iterator<Item = Result<Head, String>>) -> Result<Data, String> {
    item(&mut heads).and_then(|item| {
        heads
            .next()
            .map_or(Ok(item), |_| Err("input after the item".into()))
    })
}

fn next(heads: &mut impl Iterator<Item = Result<Head, String>>) -> Result<Head, String> {
    heads
        .next()
        .unwrap_or_else(|| Err("the input ends inside the item".into()))
}

fn item(heads: &mut impl Iterator<Item = Result<Head, String>>) -> Result<Data, String> {
    rest_of(next(heads)?, heads)
}

/// The item that starts with `head`; `Break` is refused, as only the reader of an indefinite
/// container takes it.
fn rest_of(
    head: Head,
    heads: &mut impl Iterator<Item = Result<Head, String>>,
) -> Result<Data, String> {
    match head {
        Head::Integer(value) => Ok(Data::Integer(value)),
        Head::Bytes(bytes) => Ok(Data::Bytes(bytes)),
        Head::Text(text) => Ok(Data::Text(text)),
        Head::BytesStart => until_break(heads, |head, _| match head {
            Head::Bytes(bytes) => Ok(bytes),
            _ => Err("a chunk of a byte string that is not a definite byte string".into()),
        })
        .map(Data::BytesChunks),
        Head::TextStart => until_break(heads, |head, _| match head {
            Head::Text(text) => Ok(text),
            _ => Err("a chunk of a text string that is not a definite text string".into()),
        })
        .map(Data::TextChunks),
        Head::Array(Some(len)) => (0..len)
            .map(|_| item(heads))
            .collect::<Result<_, _>>()
            .map(|items| Data::Array(items, Length::Definite)),
        Head::Array(None) => {
            until_break(heads, rest_of).map(|items| Data::Array(items, Length::Indefinite))
        }
        Head::Map(Some(len)) => (0..len)
            .map(|_| Ok((item(heads)?, item(heads)?)))
            .collect::<Result<_, String>>()
            .map(|entries| Data::Map(entries, Length::Definite)),
        Head::Map(None) => until_break(heads, |head, heads| {
            Ok((rest_of(head, heads)?, item(heads)?))
        })
        .map(|entries| Data::Map(entries, Length::Indefinite)),
        Head::Tag(tag) => Ok(Data::Tag(tag, Box::new(item(heads)?))),
        Head::Simple(value) => Ok(Data::Simple(value)),
        Head::Bool(value) => Ok(Data::Bool(value)),
        Head::Null => Ok(Data::Null),
        Head::Undefined => Ok(Data::Undefined),
        Head::Float(value) => Ok(Data::Float(value)),
        Head::Break => Err("a break outside an indefinite item".into()),
    }
}

/// The members of an indefinite item, each read from its first head, up to its break.
fn until_break<I: Iterator<Item = Result<Head, String>>, T>(
    heads: &mut I,
    mut member: impl FnMut(Head, &mut I) -> Result<T, String>,
) -> Result<Vec<T>, String> {
    std::iter::from_fn(|| match next(heads) {
        Ok(Head::Break) => None,
        Ok(head) => Some(member(head, heads)),
        Err(error) => Some(Err(error)),
    })
    .collect()
}

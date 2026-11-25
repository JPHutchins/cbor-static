#[derive(Serialize, Deserialize)]
pub struct Unit;

#[derive(Serialize, Deserialize)]
pub struct Newtype(u32);

#[derive(Serialize, Deserialize)]
pub struct Tuple(u32, bool);

#[derive(Serialize, Deserialize)]
pub struct Fields {
    a: u32,
    b: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    c: Option<u32>,
}

#[derive(Serialize, Deserialize)]
pub enum External {
    Unit,
    Newtype(u32),
    Tuple(u32, bool),
    Struct { x: u32 },
}

#[derive(Serialize, Deserialize)]
#[serde(tag = "t")]
pub enum Internal {
    Unit,
    Struct { x: u32 },
    Newtype(Fields),
}

#[derive(Serialize, Deserialize)]
#[serde(tag = "t", content = "c")]
pub enum Adjacent {
    Unit,
    Newtype(u32),
    Struct { x: u32 },
}

#[derive(Serialize, Deserialize)]
#[serde(untagged)]
pub enum Untagged {
    Int(u32),
    Text(String),
    Pair(u32, u32),
}

/// A sequence serde cannot count ahead: its iterator gives no exact size, so `collect_seq` calls
/// `serialize_seq(None)`.
#[derive(Deserialize)]
pub struct UnknownLength(pub Vec<u32>);

impl Serialize for UnknownLength {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_seq(self.0.iter().filter(|_| true))
    }
}

#[derive(Serialize, Deserialize)]
pub struct Flattened {
    a: u32,
    #[serde(flatten)]
    rest: Fields,
}

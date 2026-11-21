use cbor_static::{Cbor, Error, RawCbor, RawEntry};
use heapless::{LinearMap, Vec};

/// The capacity of the COSE types' maps and lists.
pub(crate) const ENTRIES: usize = 8;

/// RFC 9052 `label = int / tstr`, also every other `int / tstr` the RFC names.
///
/// ```
/// use cbor_static_conformance::cose::Label;
///
/// assert_eq!(cbor_static::from_slice::<Label>(b"\x26")?, Label::Int(-7));
/// assert_eq!(cbor_static::from_slice::<Label>(b"\x61a")?, Label::Text("a"));
/// # Ok::<(), cbor_static::Error>(())
/// ```
#[derive(Cbor, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(untagged)]
pub enum Label<'a> {
    Int(i64),
    Text(&'a str),
}

/// A header map as written: each label with its value's bytes, in input order, so that it
/// re-encodes byte for byte whatever order its writer chose.
///
/// ```
/// use cbor_static_conformance::cose::{HeaderMap, Label};
///
/// let written = b"\xa2\x20\x01\x04\x41\x31";
/// let map: HeaderMap = cbor_static::from_slice(written)?;
/// assert_eq!(map.keys().copied().collect::<Vec<_>>(), [Label::Int(-1), Label::Int(4)]);
///
/// let mut out = [0u8; 8];
/// let len = cbor_static::to_slice(&map, &mut out)?;
/// assert_eq!(&out[..len], written);
/// # Ok::<(), cbor_static::Error>(())
/// ```
pub type HeaderMap<'a> = LinearMap<Label<'a>, RawCbor<'a>, ENTRIES>;

/// RFC 9052 `empty_or_serialized_map`: the byte string as written, whose content is decoded on
/// demand, because the signed bytes are the written ones.
///
/// ```
/// use cbor_static_conformance::cose::{Label, Protected};
///
/// let protected: Protected = cbor_static::from_slice(b"\x43\xa1\x01\x26")?;
/// assert_eq!(protected.0, b"\xa1\x01\x26");
/// assert_eq!(protected.headers()?.keys().copied().collect::<Vec<_>>(), [Label::Int(1)]);
/// assert!(Protected(b"").headers()?.is_empty());
/// assert!(Protected(b"\xa0").headers()?.is_empty());
/// # Ok::<(), cbor_static::Error>(())
/// ```
#[derive(Cbor, Clone, Copy, Debug, PartialEq, Eq)]
pub struct Protected<'a>(pub &'a [u8]);

impl<'a> Protected<'a> {
    /// The headers inside, none when the byte string is empty.
    ///
    /// ```
    /// use cbor_static_conformance::cose::Protected;
    ///
    /// assert_eq!(Protected(b"\xa1\x04\x41\x31").headers()?.len(), 1);
    /// assert!(Protected(b"\x80").headers().is_err());
    /// # Ok::<(), cbor_static::Error>(())
    /// ```
    ///
    /// # Errors
    ///
    /// When the content is not one header map.
    pub fn headers(&self) -> Result<HeaderMap<'a>, Error> {
        if self.0.is_empty() {
            Ok(HeaderMap::new())
        } else {
            cbor_static::from_slice(self.0)
        }
    }

    /// The headers inside, typed.
    ///
    /// ```
    /// use cbor_static_conformance::cose::{Label, Protected};
    ///
    /// assert_eq!(Protected(b"\xa1\x01\x26").typed()?.alg, Some(Label::Int(-7)));
    /// assert_eq!(Protected(b"").typed()?.alg, None);
    /// # Ok::<(), cbor_static::Error>(())
    /// ```
    ///
    /// # Errors
    ///
    /// When the content is not one header map, or a known header has the wrong type.
    pub fn typed(&self) -> Result<Headers<'a>, Error> {
        cbor_static::from_slice(if self.0.is_empty() { b"\xa0" } else { self.0 })
    }
}

/// RFC 9052 §3.1's `Generic_Headers`, and every other header as written.
///
/// ```
/// use cbor_static_conformance::cose::{Headers, Label};
///
/// let headers: Headers = cbor_static::from_slice(b"\xa2\x01\x26\x18\x21\x80")?;
/// assert_eq!(headers.alg, Some(Label::Int(-7)));
/// assert_eq!(headers.other.len(), 1);
/// # Ok::<(), cbor_static::Error>(())
/// ```
#[derive(Cbor, Debug, PartialEq)]
pub struct Headers<'a> {
    #[cbor(key = 1, default, skip_serializing_if = "Option::is_none")]
    #[serde(borrow)]
    pub alg: Option<Label<'a>>,
    #[cbor(key = 2, default, skip_serializing_if = "Option::is_none")]
    pub crit: Option<Vec<Label<'a>, ENTRIES>>,
    #[cbor(key = 3, default, skip_serializing_if = "Option::is_none")]
    pub content_type: Option<ContentType<'a>>,
    #[cbor(key = 4, default, skip_serializing_if = "Option::is_none")]
    pub kid: Option<&'a [u8]>,
    #[cbor(key = 5, default, skip_serializing_if = "Option::is_none")]
    pub iv: Option<&'a [u8]>,
    #[cbor(key = 6, default, skip_serializing_if = "Option::is_none")]
    pub partial_iv: Option<&'a [u8]>,
    #[cbor(other)]
    pub other: Vec<RawEntry<'a>, ENTRIES>,
}

/// RFC 9052 §3.1's content type, `uint / tstr`.
///
/// ```
/// use cbor_static_conformance::cose::ContentType;
///
/// assert_eq!(cbor_static::from_slice::<ContentType>(b"\x00")?, ContentType::Format(0));
/// assert!(cbor_static::from_slice::<ContentType>(b"\x20").is_err());
/// # Ok::<(), cbor_static::Error>(())
/// ```
#[derive(Cbor, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(untagged)]
pub enum ContentType<'a> {
    Format(u64),
    Media(&'a str),
}

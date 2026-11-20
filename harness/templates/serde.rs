#![no_std]
#![no_main]

use core::hint::black_box;

#[panic_handler]
fn panic(_: &core::panic::PanicInfo) -> ! {
    loop {}
}

#[derive(serde::Serialize, serde::Deserialize)]
pub enum Shape {
    Unit,
    Newtype(u32),
    Tuple(u8, i8),
    Struct { letter: char },
}

#[derive(serde::Serialize, serde::Deserialize)]
pub struct Everything<'a> {
    flag: bool,
    small: u8,
    signed: i64,
    wide: u128,
    wide_signed: i128,
    single: f32,
    double: f64,
    letter: char,
    text: &'a str,
    #[serde(with = "serde_bytes")]
    bytes: &'a [u8],
    maybe: Option<u32>,
    unit: (),
    list: [u16; 2],
    shapes: [Shape; 2],
}

/// Its items, as a sequence of unknown length.
pub struct Unsized<'a>(&'a [u16]);

impl serde::Serialize for Unsized<'_> {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_seq(self.0.iter().filter(|_| true))
    }
}

/// Its items by their index, as a map of unknown length.
pub struct UnsizedMap<'a>(&'a [u16]);

impl serde::Serialize for UnsizedMap<'_> {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_map(self.0.iter().enumerate().filter(|_| true))
    }
}

/// Whatever item comes, read through `deserialize_any`.
pub struct Anything;

impl<'de> serde::Deserialize<'de> for Anything {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        deserializer.deserialize_any(serde::de::IgnoredAny).map(|_| Self)
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn _start() -> ! {
    let input: &[u8] = unsafe { core::slice::from_raw_parts(black_box(0x2000_0000usize) as *const u8, black_box(256)) };
    let out: &mut [u8] = unsafe { core::slice::from_raw_parts_mut(black_box(0x2000_1000usize) as *mut u8, black_box(256)) };
    black_box(
        cbor_static::serde::from_slice::<Everything>(input)
            .ok()
            .and_then(|value| {
                cbor_static::serde::to_slice(&value, &mut *out).ok()?;
                cbor_static::serde::to_slice(&Unsized(&value.list), &mut *out).ok()?;
                cbor_static::serde::to_slice(&UnsizedMap(&value.list), out).ok()
            }),
    );
    black_box(cbor_static::serde::from_slice::<Anything>(input).is_ok());
    black_box(
        cbor_static::serde::Deserializer::with_depth_limit(input, black_box(4))
            .decode_partial::<Shape>()
            .is_ok(),
    );
    loop {}
}

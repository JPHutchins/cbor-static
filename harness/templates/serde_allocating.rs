#![no_std]
#![no_main]

use core::hint::black_box;

$allocator

#[panic_handler]
fn panic(_: &core::panic::PanicInfo) -> ! {
    loop {}
}

#[derive(serde::Serialize, serde::Deserialize)]
pub struct Inner {
    x: u16,
    y: bool,
}

#[derive(serde::Serialize, serde::Deserialize)]
#[serde(untagged)]
pub enum Id {
    Number(u64),
    Name(alloc::string::String),
}

#[derive(serde::Serialize, serde::Deserialize)]
pub struct Owned {
    name: alloc::string::String,
    #[serde(with = "serde_bytes")]
    blob: alloc::vec::Vec<u8>,
    values: alloc::vec::Vec<u32>,
    id: Id,
    #[serde(flatten)]
    inner: Inner,
}

#[unsafe(no_mangle)]
pub extern "C" fn _start() -> ! {
    let input: &[u8] = unsafe { core::slice::from_raw_parts(black_box(0x2000_0000usize) as *const u8, black_box(256)) };
    if let Ok(owned) = cbor_static::serde::from_slice::<Owned>(input) {
        black_box(cbor_static::serde::to_vec(&owned).map_or(0, |encoded| encoded.len()));
    }
    loop {}
}

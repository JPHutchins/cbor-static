#![no_std]
#![no_main]

use core::hint::black_box;

#[panic_handler]
fn panic(_: &core::panic::PanicInfo) -> ! {
    loop {}
}

#[derive(serde::Deserialize)]
pub struct Signed<'a> {
    alg: i8,
    #[serde(with = "serde_bytes")]
    payload: &'a [u8],
    wide: u128,
}

#[unsafe(no_mangle)]
pub extern "C" fn _start() -> ! {
    let input: &[u8] = unsafe { core::slice::from_raw_parts(black_box(0x2000_0000usize) as *const u8, black_box(256)) };
    black_box(cbor_static::serde::Deserializer::new_strict(input).decode::<Signed>().is_ok());
    loop {}
}

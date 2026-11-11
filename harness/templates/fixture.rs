#![no_std]
#![no_main]

use core::hint::black_box;

#[panic_handler]
fn panic(_: &core::panic::PanicInfo) -> ! {
    loop {}
}

#[derive(cbor_static::Cbor)]
pub struct Fixture {
$fields
}

#[unsafe(no_mangle)]
pub extern "C" fn _start() -> ! {
    let input: &[u8] = unsafe { core::slice::from_raw_parts(black_box(0x2000_0000usize) as *const u8, black_box(64)) };
    let out: &mut [u8] = unsafe { core::slice::from_raw_parts_mut(black_box(0x2000_1000usize) as *mut u8, black_box(64)) };
    let mut slot = core::mem::MaybeUninit::<Fixture>::uninit();
    if let Ok(value) = cbor_static::decode_into(input, &mut slot) {
        black_box(cbor_static::encode_into(&*value, out).ok());
    }
    loop {}
}

#![no_std]
#![no_main]

use core::hint::black_box;

#[panic_handler]
fn panic(_: &core::panic::PanicInfo) -> ! {
    loop {}
}

#[unsafe(no_mangle)]
pub extern "C" fn _start() -> ! {
    let input: &[u8] = unsafe { core::slice::from_raw_parts(black_box(0x2000_0000usize) as *const u8, black_box(64)) };
    let out: &mut [u8] = unsafe { core::slice::from_raw_parts_mut(black_box(0x2000_1000usize) as *mut u8, black_box(64)) };
    let mut writer = cbor_static::TokenWriter::new(out);
    let copied = cbor_static::Tokens::new(input).try_for_each(|token| writer.write(token?).map(|_| ()));
    black_box((copied.is_ok(), writer.written()));
    loop {}
}

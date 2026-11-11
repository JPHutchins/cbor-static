#![no_std]
#![no_main]
#![allow(dead_code)]

use core::hint::black_box;

$prelude
#[panic_handler]
fn panic(_: &core::panic::PanicInfo) -> ! {
    loop {}
}

$items
#[unsafe(no_mangle)]
pub extern "C" fn _start() -> ! {
    let input: &[u8] = unsafe { core::slice::from_raw_parts(black_box(0x2000_0000usize) as *const u8, black_box(256)) };
    let out: &mut [u8] = unsafe { core::slice::from_raw_parts_mut(black_box(0x2000_1000usize) as *mut u8, black_box(256)) };
$calls
    loop {}
}

#![no_std]
#![no_main]

use core::hint::black_box;

$allocator

#[panic_handler]
fn panic(_: &core::panic::PanicInfo) -> ! {
    loop {}
}

#[unsafe(no_mangle)]
pub extern "C" fn _start() -> ! {
    let mut values = alloc::vec::Vec::new();
    values.push(black_box(1u8));
    black_box(values);
    loop {}
}

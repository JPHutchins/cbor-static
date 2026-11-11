#![no_std]
#![no_main]
#![allow(dead_code)]

use core::mem::size_of;

#[panic_handler]
fn panic(_: &core::panic::PanicInfo) -> ! {
    loop {}
}

$items

#[used]
#[unsafe(no_mangle)]
pub static SIZES: [[u32; 4]; $count] = [
$rows
];

#[unsafe(no_mangle)]
pub extern "C" fn _start() -> ! {
    loop {}
}

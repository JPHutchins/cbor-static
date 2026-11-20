#![no_std]
#![no_main]

use core::hint::black_box;

#[panic_handler]
fn panic(_: &core::panic::PanicInfo) -> ! {
    loop {}
}

struct Sink;

impl core::fmt::Write for Sink {
    fn write_str(&mut self, text: &str) -> core::fmt::Result {
        black_box(text);
        Ok(())
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn _start() -> ! {
    black_box(core::fmt::write(&mut Sink, format_args!("{seven}", seven = black_box(7u32))).is_ok());
    loop {}
}

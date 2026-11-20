#![no_std]
#![no_main]

use core::hint::black_box;

#[panic_handler]
fn panic(_: &core::panic::PanicInfo) -> ! {
    loop {}
}

/// A number written as text, through `collect_str`.
pub struct Shown(u32);

impl serde::Serialize for Shown {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_str(&self.0)
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn _start() -> ! {
    let out: &mut [u8] = unsafe { core::slice::from_raw_parts_mut(black_box(0x2000_1000usize) as *mut u8, black_box(256)) };
    black_box(cbor_static::serde::to_slice(&Shown(black_box(70_000)), out).is_ok());
    loop {}
}

#![no_std]
#![no_main]

use core::hint::black_box;

$allocator

#[panic_handler]
fn panic(_: &core::panic::PanicInfo) -> ! {
    loop {}
}

#[derive(cbor_static::Cbor)]
pub struct Every<'a> {
    pub values: alloc::vec::Vec<u32>,
    pub name: alloc::string::String,
    #[cbor(bytes)]
    pub blob: alloc::vec::Vec<u8>,
    pub boxed: alloc::boxed::Box<u32>,
    pub bounded: cbor_static::Bounded<alloc::string::String, 8>,
    #[cbor(other)]
    pub rest: alloc::vec::Vec<cbor_static::RawEntry<'a>>,
}

#[unsafe(no_mangle)]
pub extern "C" fn _start() -> ! {
    let input: &[u8] = unsafe { core::slice::from_raw_parts(black_box(0x2000_0000usize) as *const u8, black_box(256)) };
    if let Ok(every) = cbor_static::from_slice::<Every>(input) {
        black_box(cbor_static::to_vec(&every).map_or(0, |encoded| encoded.len()));
        black_box(cbor_static::to_vec_in(&every, alloc::alloc::Global).map_or(0, |encoded| encoded.len()));
    }
    loop {}
}

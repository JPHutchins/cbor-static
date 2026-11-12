#![no_std]
#![no_main]

use core::hint::black_box;

#[panic_handler]
fn panic(_: &core::panic::PanicInfo) -> ! {
    loop {}
}

#[derive($derive)]
pub struct Inner {
    pub id: u32,
    pub name: heapless::String<8>,
}

#[derive($derive)]
pub enum Kind {
    A,
    B(u32),
    C { x: i16 },
}

#[derive($derive)]
pub struct Meters(pub u32);

#[derive($derive)]
pub struct Message<'a> {
    pub a: u8,
    pub b: i64,
    pub c: Option<u32>,
    pub d: f32,
    pub e: cbor_static::F16,
    pub f: char,
    pub g: &'a str,
    #[cbor(bytes)]
    pub h: [u8; 4],
    pub i: heapless::Vec<Inner, 2>,
    #[cbor(bytes)]
    pub j: heapless::Vec<u8, 8>,
    pub k: heapless::LinearMap<u8, u32, 2>,
    pub l: cbor_static::Tagged<1, u64>,
    pub m: cbor_static::SelfDescribed<Kind>,
    pub n: cbor_static::RawCbor<'a>,
    pub o: cbor_static::ArrayRef<'a, u32>,
    pub p: cbor_static::MapRef<'a, u8, u32>,
    pub q: (u32, bool),
    pub r: cbor_static::Chunks<'a, str>,
    pub s: u128,
    pub t: Meters,
    pub u: [Kind; 2],
}

#[unsafe(no_mangle)]
pub extern "C" fn _start() -> ! {
    let out: &mut [u8] = unsafe { core::slice::from_raw_parts_mut(black_box(0x2000_1000usize) as *mut u8, black_box(256)) };
$calls
    loop {}
}

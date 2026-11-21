//! Every distinct record input, one case each: its payload decodes to the value its kwargs name,
//! and re-encodes byte for byte. Lines with the same payload and kwargs are the same input, so
//! only the first has a case; `nix/smp_vectors/` pins how many each file holds.

use core::fmt::Debug;

use cbor_static::{Decode, Encode};
use heapless::{LinearMap, Vec};

pub fn check<'a, T: Decode<'a> + Encode + PartialEq + Debug>(twin: &'a [u8], expected: &T) {
    use crate::common::reencode;

    assert_eq!(cbor_static::from_slice::<T>(twin).as_ref(), Ok(expected));
    assert_eq!(reencode::<T>(twin).as_deref(), Ok(twin));
}

pub fn vec<T, const N: usize, const M: usize>(items: [T; M]) -> Vec<T, N> {
    items.into_iter().collect()
}

pub fn map<K: Eq, V, const N: usize, const M: usize>(entries: [(K, V); M]) -> LinearMap<K, V, N> {
    entries.into_iter().collect()
}

macro_rules! records {
    ($($command:ident: $type:ty { $($(#[$attribute:meta])* ($line:ident, $twin:literal, $expected:expr)),* $(,)? })*) => {
        $(
            mod $command {
                use super::*;
                use crate::records::check;

                $(
                    #[test]
                    $(#[$attribute])*
                    fn $line() {
                        check::<$type>(twin!($twin), &$expected);
                    }
                )*
            }
        )*
    };
}

mod image;
mod os;

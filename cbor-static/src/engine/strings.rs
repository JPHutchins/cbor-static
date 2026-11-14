//! The per-type side of owned strings: how the decoder's copy form fills a `char`, a byte array
//! or, with `heapless`, a `String` or byte `Vec`, and the leaves that encode each one.
#![expect(
    clippy::cast_ptr_alignment,
    reason = "every field pointer is a descriptor offset taken with offset_of!, so it is aligned"
)]

use super::{Pending, StrictOps, TEXT_OWNED};
use crate::error::ErrorKind;
use crate::schema::{DecKind, EncKind};
use crate::wire::Major;

pub static ENC_CHAR: EncKind = EncKind::Leaf {
    encode: enc_char,
    absent: None,
};

/// A `char` collects the UTF-8 of its one character in its own four bytes, then becomes it.
pub static DEC_CHAR: DecKind = DecKind::Owned {
    ops: &TEXT_OWNED,
    begin: nothing,
    drop: None,
    append: within::<4>,
    finish: one_char,
};

unsafe fn enc_char(src: *const u8, out: &mut Pending) {
    let mut utf8 = [0; 4];
    let text = unsafe { src.cast::<char>().read() }.encode_utf8(&mut utf8);
    out.head(Major::Text, u64::try_from(text.len()).unwrap_or(4));
    out.bytes(text.as_bytes());
}

/// A byte string of `T`'s bytes.
pub unsafe fn enc_view<T: AsRef<[u8]>>(src: *const u8, out: &mut Pending) {
    write(out, Major::Bytes, unsafe { &*src.cast::<T>() }.as_ref());
}

fn write(out: &mut Pending, major: Major, payload: &[u8]) {
    out.head(major, u64::try_from(payload.len()).unwrap_or(u64::MAX));
    out.tail(payload);
}

pub const fn nothing(_: *mut u8) {}

/// Copies `segment` to `at` bytes into an `N`-byte buffer, which it must fit.
pub unsafe fn within<const N: usize>(
    dst: *mut u8,
    at: usize,
    segment: &[u8],
) -> Result<(), ErrorKind> {
    match at.checked_add(segment.len()) {
        Some(end) if end <= N => {
            unsafe { core::slice::from_raw_parts_mut(dst.add(at), segment.len()) }
                .iter_mut()
                .zip(segment)
                .for_each(|(d, s)| *d = *s);
            Ok(())
        }
        _ => Err(ErrorKind::TypeMismatch),
    }
}

pub const fn exactly<const N: usize>(
    _: *mut u8,
    total: usize,
    _: Option<&'static StrictOps>,
) -> Result<(), ErrorKind> {
    if total == N {
        Ok(())
    } else {
        Err(ErrorKind::TypeMismatch)
    }
}

/// The text's one character, which the copy form has already validated as UTF-8.
unsafe fn one_char(
    dst: *mut u8,
    total: usize,
    _: Option<&'static StrictOps>,
) -> Result<(), ErrorKind> {
    let mut chars =
        unsafe { core::str::from_utf8_unchecked(core::slice::from_raw_parts(dst, total)) }.chars();
    match (chars.next(), chars.next()) {
        (Some(one), None) => {
            unsafe { dst.cast::<char>().write(one) };
            Ok(())
        }
        _ => Err(ErrorKind::TypeMismatch),
    }
}

#[cfg(feature = "heapless")]
pub use bounded::{any_length, append_string, append_vec, empty, enc_string};

#[cfg(feature = "heapless")]
mod bounded {
    use heapless::{LenType, String, Vec};

    use super::{Major, Pending, StrictOps, write};
    use crate::error::ErrorKind;

    pub unsafe fn enc_string<const N: usize, L: LenType>(src: *const u8, out: &mut Pending) {
        write(
            out,
            Major::Text,
            unsafe { &*src.cast::<String<N, L>>() }.as_bytes(),
        );
    }

    pub unsafe fn empty<T: Default>(dst: *mut u8) {
        unsafe { dst.cast::<T>().write(T::default()) };
    }

    /// Copies the next segment of text, which may end inside a character until the copy form
    /// finishes: nothing reads the `String` before then.
    pub unsafe fn append_string<const N: usize, L: LenType>(
        dst: *mut u8,
        _: usize,
        segment: &[u8],
    ) -> Result<(), ErrorKind> {
        unsafe { (*dst.cast::<String<N, L>>()).as_mut_vec() }
            .extend_from_slice(segment)
            .map_err(|_| ErrorKind::OutOfRange)
    }

    pub unsafe fn append_vec<const N: usize, L: LenType>(
        dst: *mut u8,
        _: usize,
        segment: &[u8],
    ) -> Result<(), ErrorKind> {
        unsafe { &mut *dst.cast::<Vec<u8, N, L>>() }
            .extend_from_slice(segment)
            .map_err(|_| ErrorKind::OutOfRange)
    }

    #[expect(clippy::unnecessary_wraps, reason = "a FinishFn can fail")]
    pub const fn any_length(
        _: *mut u8,
        _: usize,
        _: Option<&'static StrictOps>,
    ) -> Result<(), ErrorKind> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::{exactly, one_char, within};
    use crate::error::ErrorKind;

    #[test]
    fn within_copies_at_its_offset_only_what_fits() {
        let mut buffer = [0u8; 4];
        let dst = buffer.as_mut_ptr();
        assert_eq!(unsafe { within::<4>(dst, 0, b"ab") }, Ok(()));
        assert_eq!(unsafe { within::<4>(dst, 2, b"cd") }, Ok(()));
        assert_eq!(
            unsafe { within::<4>(dst, 3, b"ef") },
            Err(ErrorKind::TypeMismatch)
        );
        assert_eq!(
            unsafe { within::<4>(dst, usize::MAX, b"g") },
            Err(ErrorKind::TypeMismatch)
        );
        assert_eq!(&buffer, b"abcd");
        assert_eq!(exactly::<4>(dst, 4, None), Ok(()));
        assert_eq!(exactly::<4>(dst, 3, None), Err(ErrorKind::TypeMismatch));
    }

    #[test]
    fn one_char_takes_exactly_one_character() {
        for (utf8, expected) in [
            ("é".as_bytes(), Ok('é')),
            (b"", Err(ErrorKind::TypeMismatch)),
            (b"ab", Err(ErrorKind::TypeMismatch)),
        ] {
            let mut slot = [0u32; 1];
            let dst = slot.as_mut_ptr().cast::<u8>();
            assert_eq!(unsafe { within::<4>(dst, 0, utf8) }, Ok(()));
            assert_eq!(
                unsafe { one_char(dst, utf8.len(), None) }
                    .map(|()| unsafe { dst.cast::<char>().read() }),
                expected,
                "{utf8:x?}"
            );
        }
    }
}

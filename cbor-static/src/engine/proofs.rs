use core::mem::MaybeUninit;

use crate::error::ErrorKind;
use crate::schema::{Decode, Encode};
use crate::stack::{Depth, Frame, MAX_FRAMES};
use crate::{Skipped, Tagged};

macro_rules! each_stack_holds_its_frames {
    ($($(#[$attr:meta])* $name:ident: $ty:ty as $schema:path;)*) => {$(
        $(#[$attr])*
        #[kani::proof]
        fn $name() {
            holds_exactly_its_frames::<<$ty as $schema>::Stack>(<$ty as $schema>::MAX_DEPTH);
        }
    )*};
}

each_stack_holds_its_frames! {
    no_frames: u8 as Encode;
    one_struct: Leaf as Encode;
    uneven_fields_encoded: Uneven as Encode;
    uneven_fields_decoded: Uneven as Decode<'static>;
    positional_fields: Positional as Decode<'static>;
    an_enum_encoded: Choice as Encode;
    an_enum_decoded: Choice as Decode<'static>;
    a_flattened_field_encoded: Flattening as Encode;
    a_flattened_field_decoded: Flattening as Decode<'static>;
    other_entries_encoded: Unclaimed as Encode;
    other_entries_decoded: Unclaimed as Decode<'static>;
    nested_options: Option<Option<Leaf>> as Decode<'static>;
    a_tag_encoded: Tagged<1, Leaf> as Encode;
    a_tag_decoded: Tagged<1, Leaf> as Decode<'static>;
    a_tuple: (u8, Leaf, [Leaf; 2]) as Decode<'static>;
    eight_arrays: By8<u8> as Encode;
    arrays_32_deep: By32<u8> as Decode<'static>;
    #[cfg(feature = "heapless")]
    a_bounded_vec: heapless::Vec<Leaf, 2> as Decode<'static>;
    #[cfg(feature = "heapless")]
    a_bounded_map_encoded: heapless::LinearMap<Leaf, Uneven, 2> as Encode;
    #[cfg(feature = "heapless")]
    a_bounded_map_decoded: heapless::LinearMap<Leaf, Uneven, 2> as Decode<'static>;
}

#[kani::proof]
fn a_push_fills_the_next_frame_or_is_limit_exceeded() {
    use super::push;

    let old = any_frame();
    let mut buffer = [MaybeUninit::new(old); MAX_FRAMES];
    let len: usize = kani::any();
    kani::assume(len <= MAX_FRAMES);
    let before: Depth = kani::any();
    kani::assume(usize::from(before) <= len);
    let mut depth = before;
    let new = any_frame();
    match push(&mut buffer[..len], &mut depth, new) {
        Ok(()) => {
            assert!(usize::from(before) < len && depth == before + 1);
            assert!(same(
                unsafe { buffer[usize::from(before)].assume_init_ref() },
                &new
            ));
        }
        Err(kind) => {
            assert!(kind == ErrorKind::LimitExceeded && usize::from(before) == len);
            assert_eq!(depth, before);
        }
    }
    let other: usize = kani::any();
    kani::assume(other < MAX_FRAMES && other != usize::from(before));
    assert!(same(unsafe { buffer[other].assume_init_ref() }, &old));
}

#[kani::proof]
fn a_frame_index_is_in_bounds_or_unsupported() {
    use super::frame_mut;

    let mut buffer = [MaybeUninit::new(any_frame()); MAX_FRAMES];
    let len: usize = kani::any();
    kani::assume(len <= MAX_FRAMES);
    let index: Depth = kani::any();
    match frame_mut(&mut buffer[..len], index) {
        Ok(_) => assert!(usize::from(index) < len),
        Err(kind) => assert!(kind == ErrorKind::Unsupported && usize::from(index) >= len),
    }
}

fn holds_exactly_its_frames<S>(max_depth: usize) {
    use super::frames;
    use crate::stack::{depth_of, fits};

    let depth: usize = kani::any();
    assert_eq!(fits::<S>(depth), depth == max_depth);
    assert_eq!(depth_of::<S>(), max_depth);

    let mut stack = MaybeUninit::<S>::uninit();
    let start = stack.as_ptr().addr();
    let slots = frames(&mut stack, max_depth);
    assert_eq!(slots.len(), max_depth);
    let index: usize = kani::any();
    kani::assume(index < max_depth);
    let at = slots[index].as_ptr().addr();
    assert!(start <= at && at + size_of::<Frame>() <= start + size_of::<S>());
    let frame = any_frame();
    assert!(same(slots[index].write(frame), &frame));
}

fn same(a: &Frame, b: &Frame) -> bool {
    (a.remaining, a.seen, a.field, a.phase) == (b.remaining, b.seen, b.field, b.phase)
        && core::ptr::eq(a.desc, b.desc)
        && core::ptr::eq(a.base, b.base)
}

fn any_frame() -> Frame {
    use crate::stack::Phase;

    Frame {
        remaining: kani::any(),
        seen: kani::any(),
        desc: core::ptr::null(),
        base: core::ptr::null_mut(),
        field: kani::any(),
        phase: Phase::Key,
    }
}

type By2<T> = [[T; 1]; 1];
type By4<T> = By2<By2<T>>;
type By8<T> = By4<By4<T>>;
type By16<T> = By8<By8<T>>;
type By32<T> = By16<By16<T>>;

#[derive(crate::Encode, crate::Decode, PartialEq, Eq)]
struct Leaf {
    a: u8,
}

#[derive(crate::Encode, crate::Decode)]
struct Uneven {
    a: u8,
    b: Leaf,
    c: [Leaf; 1],
    d: Option<Leaf>,
    e: (u8, By2<Leaf>),
}

#[derive(crate::Decode)]
#[cbor(array)]
struct Positional {
    a: Leaf,
    b: By2<u8>,
}

#[derive(crate::Encode, crate::Decode)]
enum Choice {
    Unit,
    Name(u8),
    Payload(Uneven),
}

#[derive(crate::Encode, crate::Decode)]
struct Flattening {
    a: u8,
    #[cbor(flatten)]
    inner: Uneven,
}

#[derive(crate::Encode, crate::Decode)]
struct Unclaimed {
    a: Leaf,
    #[cbor(other)]
    rest: Skipped,
}

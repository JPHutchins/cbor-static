#![forbid(unsafe_code)]
//! What `flatten` concatenates at compile time (R-127, Q34): a flattened struct's entries join its
//! parent's layout and kinds. Their values come from the child's own descriptors, which the codec
//! traits vouch for; this module gives only the shapes of the arrays they are copied into, so that
//! a generic child needs no length the compiler cannot name.

use crate::engine::{filled, offset, set};
use crate::schema::{FieldLayout, Layout};

/// Storage of `LEN` values of `X`, laid out as `[X; LEN]`: an array, or two storages side by side.
/// Sealed, since the engine reads every one as that array.
pub trait Storage<X>: Copy + sealed::Storage {
    const LEN: usize;
}

impl<X: Copy, const N: usize> Storage<X> for [X; N] {
    const LEN: usize = N;
}

/// Two storages side by side, which `repr(C)` lays out as one array.
#[derive(Clone, Copy)]
#[repr(C)]
pub struct Concat<A, B>(A, B);

impl<X, A: Storage<X>, B: Storage<X>> Storage<X> for Concat<A, B> {
    const LEN: usize = A::LEN + B::LEN;
}

/// A map-form struct's entries, which a struct flattening it takes as its own: the storage of one
/// value per entry, its flattened children's included, and of its pre-encoded names. The derive
/// implements it in each direction it derives.
pub trait Flat<Direction> {
    type Entries<X: Copy + 'static>: Storage<X> + 'static;
    type Names: Storage<u8> + 'static;
}

/// A run of a flattening struct's entries, in declaration order: one of its own, placed in its own
/// names, or a flattened field's, whose struct is at this offset and whose names follow.
#[derive(Clone, Copy)]
pub enum Run {
    Own(FieldLayout),
    Flattened(&'static Layout, usize),
}

/// The entries of `runs`, a flattened field's names placed after `own_names` bytes of the struct's
/// own and the names of the flattened fields before it.
///
/// # Panics
/// At compile time, when the runs do not fill `S`, or the names outgrow 64 KiB.
#[must_use]
pub const fn entries<S: Storage<FieldLayout>>(own_names: usize, runs: &[Run]) -> S {
    let mut out: S = filled(FieldLayout {
        offset: 0,
        name_at: 0,
        name_len: 0,
        omit_if_absent: false,
    });
    let (mut run, mut at, mut names) = (0, 0, own_names);
    while run < runs.len() {
        match runs[run] {
            Run::Own(place) => {
                set(&mut out, at, place);
                at += 1;
            }
            Run::Flattened(layout, base) => {
                let mut each = 0;
                while each < layout.fields.len() {
                    let place = layout.fields[each];
                    let name_at = place.name_at as usize + names;
                    assert!(
                        name_at <= u16::MAX as usize,
                        "cbor-static: names exceed 64 KiB"
                    );
                    set(
                        &mut out,
                        at,
                        FieldLayout {
                            offset: offset(place.offset as usize + base),
                            #[expect(clippy::cast_possible_truncation, reason = "asserted above")]
                            name_at: name_at as u16,
                            ..place
                        },
                    );
                    (each, at) = (each + 1, at + 1);
                }
                names += layout.names.len();
            }
        }
        run += 1;
    }
    assert!(
        at == S::LEN,
        "cbor-static: fewer entries than their storage"
    );
    out
}

/// A flattened field's layout: its type's, which `Flat` already requires to be a map-form struct.
///
/// # Panics
/// At compile time, for a descriptor without a layout.
#[must_use]
pub const fn flattened(layout: Option<&'static Layout>) -> &'static Layout {
    match layout {
        Some(layout) => layout,
        None => panic!("cbor-static: a flattened field must be a map-form struct"),
    }
}

mod sealed {
    pub trait Storage {}

    impl<X, const N: usize> Storage for [X; N] {}
    impl<A, B> Storage for super::Concat<A, B> {}
}

#[cfg(test)]
mod tests {
    use super::{Run, entries};
    use crate::engine::gathered;
    use crate::keys::{Key, field, names, names_len};
    use crate::schema::{FieldLayout, Layout};

    const CHILD_KEYS: &[Key] = &[Key::Text("x"), Key::Int(-1)];
    const CHILD_NAMES: [u8; names_len(CHILD_KEYS)] = names(CHILD_KEYS);
    const CHILD: Layout = Layout {
        names: &CHILD_NAMES,
        fields: &[
            field(CHILD_KEYS, 0, 2, false),
            field(CHILD_KEYS, 1, 6, true),
        ],
    };
    const OWN_KEYS: &[Key] = &[Key::Text("ab"), Key::Text("c")];
    const OWN_NAMES: [u8; names_len(OWN_KEYS)] = names(OWN_KEYS);

    const BUILT: [FieldLayout; 4] = entries(
        OWN_NAMES.len(),
        &[
            Run::Own(field(OWN_KEYS, 0, 0, false)),
            Run::Flattened(&CHILD, 8),
            Run::Own(field(OWN_KEYS, 1, 20, false)),
        ],
    );
    const NAMES: [u8; OWN_NAMES.len() + CHILD_NAMES.len()] = gathered(&[&OWN_NAMES, &CHILD_NAMES]);
    const BUILT_LAYOUT: Layout = Layout {
        names: &NAMES,
        fields: &BUILT,
    };

    #[test]
    fn a_flattened_layout_follows_its_struct_in_names_and_in_offsets() {
        let items: [&[u8]; 4] = [b"\x62ab", b"\x61x", b"\x20", b"\x61c"];
        for (at, item) in (0..).zip(items) {
            assert_eq!(BUILT_LAYOUT.name_item(at), item);
        }
        assert_eq!(
            BUILT.map(|place| (place.offset, place.omit_if_absent)),
            [(0, false), (10, false), (14, true), (20, false)]
        );
    }
}

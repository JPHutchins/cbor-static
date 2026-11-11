use core::mem::MaybeUninit;
use core::ptr;

use super::pending::Pending;
use super::{Progress, frame_mut, push, top_field};
use crate::error::{Error, ErrorKind};
use crate::schema::{EncKind, FieldPtrFn, Layout};
use crate::stack::{Frame, Phase};
use crate::wire::Major;

const BEFORE_FIRST: u8 = u8::MAX;

pub struct State {
    root: &'static EncKind,
    value: *const u8,
    depth: u8,
    started: bool,
    pending: Pending,
    offset: usize,
}

impl State {
    #[inline(never)]
    pub(crate) const fn new(root: &'static EncKind, value: *const u8) -> Self {
        Self {
            root,
            value,
            depth: 0,
            started: false,
            pending: Pending::new(),
            offset: 0,
        }
    }
}

#[inline(never)]
pub fn resume(
    state: &mut State,
    frames: &mut [MaybeUninit<Frame>],
    out: &mut [u8],
) -> Result<Progress, Error> {
    let mut written = 0;
    loop {
        let drained = state
            .pending
            .drain(out.get_mut(written..).unwrap_or(&mut []));
        written += drained;
        state.offset += drained;
        if !state.pending.is_empty() {
            return Ok(Progress::NeedSpace(written));
        }
        if !advance(state, frames)
            .map_err(|kind| Error::new(kind, top_field(frames, state.depth), state.offset))?
        {
            return Ok(Progress::Done(written));
        }
    }
}

fn advance(state: &mut State, frames: &mut [MaybeUninit<Frame>]) -> Result<bool, ErrorKind> {
    if !state.started {
        state.started = true;
        return emit(state, frames, state.root, state.value).map(|()| true);
    }
    while let Some(top) = state.depth.checked_sub(1) {
        let frame = frame_mut(frames, top)?;
        let base = frame.base.cast_const();
        match (unsafe { &*frame.desc.cast::<EncKind>() }, frame.phase) {
            (
                EncKind::Struct {
                    layout,
                    kinds,
                    field_ptr,
                },
                Phase::Key,
            ) => {
                match (frame.field.wrapping_add(1)..u8::MAX)
                    .take_while(|&f| usize::from(f) < kinds.len())
                    .find(|&f| present(layout, kinds, *field_ptr, base, f))
                {
                    Some(field) => {
                        frame.field = field;
                        frame.phase = Phase::Value;
                        state.pending.tail(layout.name_item(field));
                        return Ok(true);
                    }
                    None => state.depth = top,
                }
            }
            (
                EncKind::Struct {
                    layout,
                    kinds,
                    field_ptr,
                },
                _,
            ) => {
                let field = frame.field;
                frame.phase = Phase::Key;
                let kind = kinds
                    .get(usize::from(field))
                    .ok_or(ErrorKind::Unsupported)?;
                return emit(
                    state,
                    frames,
                    kind,
                    field_addr(layout, *field_ptr, base, field),
                )
                .map(|()| true);
            }
            (
                EncKind::Enum {
                    layout,
                    payloads,
                    field_ptr,
                    ..
                },
                Phase::EnumPayload,
            ) => {
                frame.phase = Phase::EnumDone;
                let variant = frame.field;
                let payload = payloads
                    .get(usize::from(variant))
                    .copied()
                    .flatten()
                    .ok_or(ErrorKind::Unsupported)?;
                return emit(
                    state,
                    frames,
                    payload,
                    match (field_ptr, payload) {
                        (
                            Some(_),
                            EncKind::Struct {
                                field_ptr: Some(_), ..
                            },
                        ) => base,
                        (Some(glue), _) => unsafe { glue(base, 0) },
                        (None, _) => field_addr(layout, None, base, variant),
                    },
                )
                .map(|()| true);
            }
            _ => state.depth = top,
        }
    }
    Ok(false)
}

fn emit(
    state: &mut State,
    frames: &mut [MaybeUninit<Frame>],
    kind: &'static EncKind,
    at: *const u8,
) -> Result<(), ErrorKind> {
    let opened = |field, phase| Frame {
        remaining: 0,
        seen: 0,
        desc: ptr::from_ref(kind).cast(),
        base: at.cast_mut(),
        field,
        phase,
    };
    match kind {
        EncKind::Leaf { encode, .. } => {
            unsafe { encode(at, &mut state.pending) };
            Ok(())
        }
        EncKind::Struct {
            layout,
            kinds,
            field_ptr,
        } => {
            state.pending.head(
                Major::Map,
                u64::try_from(
                    (0..kinds.len())
                        .filter_map(|f| u8::try_from(f).ok())
                        .filter(|&f| present(layout, kinds, *field_ptr, at, f))
                        .count(),
                )
                .unwrap_or(u64::MAX),
            );
            push(frames, &mut state.depth, opened(BEFORE_FIRST, Phase::Key))
        }
        EncKind::Enum {
            layout,
            payloads,
            index,
            ..
        } => {
            let variant = unsafe { index(at) };
            match payloads.get(usize::from(variant)) {
                Some(None) => {
                    state.pending.tail(layout.name_item(variant));
                    Ok(())
                }
                Some(Some(_)) => {
                    state.pending.head(Major::Map, 1);
                    state.pending.tail(layout.name_item(variant));
                    push(
                        frames,
                        &mut state.depth,
                        opened(variant, Phase::EnumPayload),
                    )
                }
                None => Err(ErrorKind::UnknownVariant),
            }
        }
    }
}

fn present(
    layout: &Layout,
    kinds: &[&EncKind],
    field_ptr: Option<FieldPtrFn>,
    base: *const u8,
    field: u8,
) -> bool {
    match (
        layout.fields.get(usize::from(field)),
        kinds.get(usize::from(field)),
    ) {
        (
            Some(place),
            Some(EncKind::Leaf {
                absent: Some(absent),
                ..
            }),
        ) if place.omit_if_absent => !unsafe { absent(field_addr(layout, field_ptr, base, field)) },
        _ => true,
    }
}

fn field_addr(
    layout: &Layout,
    field_ptr: Option<FieldPtrFn>,
    base: *const u8,
    field: u8,
) -> *const u8 {
    match field_ptr {
        Some(glue) => unsafe { glue(base, field) },
        None => base.wrapping_add(
            layout
                .fields
                .get(usize::from(field))
                .map_or(0, |place| usize::try_from(place.offset).unwrap_or(0)),
        ),
    }
}

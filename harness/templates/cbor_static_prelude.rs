fn roundtrip<'a, T: cbor_static::Decode<'a> + cbor_static::Encode>(input: &'a [u8], out: &mut [u8]) -> usize {
    exchange::<T, _>(input, out, |slot| cbor_static::Decoder::new(slot))
}

fn roundtrip_strict<'a, T: cbor_static::Decode<'a> + cbor_static::Encode>(input: &'a [u8], out: &mut [u8]) -> usize {
    exchange::<T, _>(input, out, |slot| cbor_static::Decoder::new_strict(slot))
}

fn exchange<'a, T, F>(input: &'a [u8], out: &mut [u8], decoder: F) -> usize
where
    T: cbor_static::Decode<'a> + cbor_static::Encode,
    F: for<'s> FnOnce(&'s mut core::mem::MaybeUninit<T>) -> cbor_static::Decoder<'a, 's, T>,
{
    let chunk = black_box(64usize).max(1);
    let mut slot = core::mem::MaybeUninit::<T>::uninit();
    let mut decoder = decoder(&mut slot);
    let _ = input.chunks(chunk).try_for_each(|piece| match decoder.feed(piece) {
        Ok(cbor_static::Fed::NeedInput) => Ok(()),
        _ => Err(()),
    });
    let Ok(value) = decoder.finish() else { return 0 };
    let mut encoder = cbor_static::Encoder::new(&*value);
    out.chunks_mut(chunk)
        .try_fold(0, |written, piece| match encoder.resume(piece) {
            Ok(cbor_static::Progress::NeedSpace(n)) => Ok(written + n),
            Ok(cbor_static::Progress::Done(n)) => Err(written + n),
            Err(_) => Err(0),
        })
        .unwrap_or_else(|written| written)
}

fn decode_only<'a, T: cbor_static::Decode<'a>>(input: &'a [u8], _: &mut [u8]) -> usize {
    let chunk = black_box(64usize).max(1);
    let mut slot = core::mem::MaybeUninit::<T>::uninit();
    let mut decoder = cbor_static::Decoder::new(&mut slot);
    let _ = input.chunks(chunk).try_for_each(|piece| match decoder.feed(piece) {
        Ok(cbor_static::Fed::NeedInput) => Ok(()),
        _ => Err(()),
    });
    decoder.finish().map_or(0, |value| {
        black_box(value);
        1
    })
}

//! `Tokens` walks a CBOR sequence exactly as far as skipping it item by item does.

/// `Tokens` reads every token of `input` exactly when `RawCbor` takes it apart into whole items,
/// and reads every token of each of those items alone.
///
/// # Panics
///
/// When the two disagree.
pub fn agree(input: &[u8]) {
    use cbor_static::Tokens;

    let walked = Tokens::new(input).try_for_each(|token| token.map(drop));
    let skipped = items(input);
    assert_eq!(
        walked.is_ok(),
        skipped.is_ok(),
        "Tokens {walked:?} but RawCbor {skipped:?} on {input:02x?}"
    );
    for item in skipped.unwrap_or_default() {
        assert_eq!(
            Tokens::new(item).try_for_each(|token| token.map(drop)),
            Ok(()),
            "Tokens refuses the item {item:02x?} that RawCbor takes from {input:02x?}"
        );
    }
}

/// The whole items of a CBOR sequence, as `RawCbor` takes them one after another.
fn items(sequence: &[u8]) -> Result<Vec<&[u8]>, cbor_static::Error> {
    core::iter::successors(Some(Ok((&[][..], sequence))), |taken| match taken {
        Ok((_, rest)) if !rest.is_empty() => Some(
            cbor_static::from_slice_partial::<cbor_static::RawCbor>(rest)
                .map(|(item, rest)| (item.as_bytes(), rest)),
        ),
        Ok(_) | Err(_) => None,
    })
    .skip(1)
    .map(|taken| taken.map(|(item, _)| item))
    .collect()
}

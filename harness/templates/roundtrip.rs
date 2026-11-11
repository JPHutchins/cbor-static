fn $fn_name(input: &[u8], out: &mut [u8]) -> usize {
    $decode
        .ok()
        .and_then(|m| $encode)
        .unwrap_or(0)
}

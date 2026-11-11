//! RFC 8949 well-formedness written the obvious way: recursion-free, with an explicit stack of
//! open items. A reference for the D-14a skip state, not an implementation of it.

use std::vec::Vec;

use super::Class;

enum Open {
    Definite(u64),
    Indefinite { map: bool, items: u64 },
    Chunks { text: bool },
}

/// The length of the first item of `bytes`.
pub fn item(bytes: &[u8]) -> Result<usize, Class> {
    let mut stack = Vec::new();
    let mut at = 0;
    loop {
        let initial = *bytes.get(at).ok_or(Class::Truncated)?;
        let (major, info) = (initial >> 5, initial & 0x1f);
        let width = match info {
            0..24 | 31 => 0,
            24 => 1,
            25 => 2,
            26 => 4,
            27 => 8,
            _ => return Err(Class::Malformed),
        };
        let arg = match info {
            0..24 => u64::from(info),
            _ => bytes
                .get(at + 1..at + 1 + width)
                .ok_or(Class::Truncated)?
                .iter()
                .fold(0, |n, &b| n << 8 | u64::from(b)),
        };
        at += 1 + width;
        let indefinite = info == 31;
        if let Some(Open::Chunks { text }) = stack.last() {
            match (major, indefinite) {
                (7, true) => {
                    stack.pop();
                }
                (2, false) if !text => {
                    at += payload(bytes, at, arg, false)?;
                    continue;
                }
                (3, false) if *text => {
                    at += payload(bytes, at, arg, true)?;
                    continue;
                }
                _ => return Err(Class::Malformed),
            }
        } else {
            match (major, indefinite) {
                (0 | 1 | 6, true) => return Err(Class::Malformed),
                (2 | 3, false) => at += payload(bytes, at, arg, major == 3)?,
                (2 | 3, true) => {
                    stack.push(Open::Chunks { text: major == 3 });
                    continue;
                }
                (4 | 5, true) => {
                    stack.push(Open::Indefinite {
                        map: major == 5,
                        items: 0,
                    });
                    continue;
                }
                (4 | 5, false) => {
                    let items = arg
                        .checked_mul(if major == 5 { 2 } else { 1 })
                        .ok_or(Class::Truncated)?;
                    if items > 0 {
                        stack.push(Open::Definite(items));
                        continue;
                    }
                }
                (6, false) => {
                    stack.push(Open::Definite(1));
                    continue;
                }
                (7, true) => match stack.pop() {
                    Some(Open::Indefinite { map, items }) if !map || items % 2 == 0 => {}
                    _ => return Err(Class::Malformed),
                },
                (7, false) if info == 24 && arg < 32 => return Err(Class::Malformed),
                _ => {}
            }
        }
        loop {
            match stack.last_mut() {
                None => return Ok(at),
                Some(Open::Definite(left)) => {
                    *left -= 1;
                    if *left > 0 {
                        break;
                    }
                    stack.pop();
                }
                Some(Open::Indefinite { items, .. }) => {
                    *items += 1;
                    break;
                }
                Some(Open::Chunks { .. }) => return Err(Class::Malformed),
            }
        }
    }
}

/// Every item of `bytes`, which must end at an item boundary.
pub fn sequence(bytes: &[u8]) -> Result<(), Class> {
    let mut at = 0;
    while at < bytes.len() {
        at += item(&bytes[at..])?;
    }
    Ok(())
}

fn payload(bytes: &[u8], at: usize, len: u64, text: bool) -> Result<usize, Class> {
    let payload = usize::try_from(len)
        .ok()
        .and_then(|len| bytes.get(at..)?.get(..len))
        .ok_or(Class::Truncated)?;
    if text && core::str::from_utf8(payload).is_err() {
        return Err(Class::InvalidUtf8);
    }
    Ok(payload.len())
}

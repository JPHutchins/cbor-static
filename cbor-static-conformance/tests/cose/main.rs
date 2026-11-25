//! COSE against cose-wg/Examples: every message, header map, key and intermediate structure.

#[path = "../common/mod.rs"]
mod common;

mod constructs;
mod decoded;
#[cfg(not(miri))]
mod differential;
mod headers;
mod intermediates;
mod keys;
mod messages;
#[cfg(not(miri))]
mod oracle;
mod rfc9052;

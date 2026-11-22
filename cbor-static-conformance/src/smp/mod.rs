//! `MCUmgr`'s SMP: the CBOR payloads of the OS (group 0) and Image (group 1) management commands,
//! one request and one response type per command, without the 8-byte SMP header. Maps are
//! written in RFC 7049's length-first order, as JPHutchins/smp writes them.
//!
//! The types follow Zephyr's encoder, citing `subsys/mgmt/mcumgr` source lines at the revision
//! `nix/smp-vectors.nix` pins. Where JPHutchins/smp writes another shape, that shape is in
//! [`smp_only`], under smp's class name.
//!
//! The vectors that hold these types to the wire, which `nix/smp_vectors/` extracts at the
//! revisions `nix/smp-vectors.nix` and `flake.nix` pin:
//! - JPHutchins/smp's binary regression records, `tests/binary_regressions/records/` (Apache-2.0);
//! - Zephyr's `MCUmgr` tests `tests/subsys/mgmt/mcumgr/{os_mgmt_echo,os_mgmt_info,smp_version}`
//!   (Apache-2.0): firmware's frames, in indefinite-length maps;
//! - hand-built: values for the commands neither source covers, written from Zephyr's
//!   `doc/services/device_mgmt/smp_groups/smp_group_1.rst` (Apache-2.0) and encoded by smp. Their
//!   bytes are smp's, not a device's.
//!
//! ```
//! use cbor_static_conformance::smp::os::EchoWriteResponse;
//!
//! let firmware = include_bytes!(concat!(env!("SMP_VECTORS"), "/zephyr/os_mgmt_echo/expected_response.cbor"));
//! let response: EchoWriteResponse = cbor_static::from_slice(firmware)?;
//! let mut out = [0u8; 64];
//! let len = cbor_static::to_slice(&response, &mut out)?;
//! assert_eq!((firmware[0], out[0]), (0xbf, 0xa1));
//! assert_eq!(cbor_static::from_slice::<EchoWriteResponse>(&out[..len])?, response);
//! # Ok::<(), cbor_static::Error>(())
//! ```

pub mod error;
pub mod image;
pub mod os;
pub mod smp_only;

use cbor_static::Cbor;

pub use error::{ErrorV1, ErrorV2};

/// The empty map that a command without fields sends, such as a read of task statistics, or
/// the success response of a reset.
///
/// ```
/// use cbor_static_conformance::smp::Empty;
///
/// let written = include_bytes!(concat!(env!("SMP_VECTORS"), "/os/reset_write_response/1.cbor"));
/// assert_eq!(cbor_static::from_slice::<Empty>(written)?, Empty {});
/// # Ok::<(), cbor_static::Error>(())
/// ```
#[derive(Cbor, Clone, Copy, Debug, Default, PartialEq, Eq)]
#[cbor(length_first, deny_unknown_fields)]
pub struct Empty {}

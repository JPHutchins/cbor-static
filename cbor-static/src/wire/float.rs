#![forbid(unsafe_code)]
//! Shortest value-preserving float widths (RFC 8949 §4.1), by moving bits only (D-22a): no float
//! arithmetic, comparison or cast, so no soft-float routine is linked.

pub const HALF: u8 = 25;
pub const SINGLE: u8 = 26;
pub const DOUBLE: u8 = 27;
/// The one NaN a deterministic encoder writes: `f9 7e00` (RFC 8949 §4.2.2).
pub const CANONICAL_NAN: u64 = 0x7e00;

const BINARY64_MANTISSA: u32 = 52;
const BINARY64_FRACTION: u64 = (1 << BINARY64_MANTISSA) - 1;
const BINARY64_SPECIAL: u64 = 0x7ff;
const BINARY64_BIAS: u64 = 1023;

/// An IEEE 754 binary interchange format narrower than binary64, by the widths of its fields.
#[derive(Clone, Copy)]
struct Format {
    exponent: u32,
    mantissa: u32,
}

const BINARY16: Format = Format {
    exponent: 5,
    mantissa: 10,
};
const BINARY32: Format = Format {
    exponent: 8,
    mantissa: 23,
};

/// The additional information and argument of the narrowest encoding of the binary64 `bits`
/// that is exact, NaN payloads included.
pub fn shortest(bits: u64) -> (u8, u64) {
    BINARY16
        .narrow(bits)
        .map(|half| (HALF, half))
        .or_else(|| BINARY32.narrow(bits).map(|single| (SINGLE, single)))
        .unwrap_or((DOUBLE, bits))
}

/// The binary64 bits of a float header's argument.
pub fn value(info: u8, arg: u64) -> Option<u64> {
    match info {
        HALF => Some(BINARY16.widen(arg)),
        SINGLE => Some(BINARY32.widen(arg)),
        DOUBLE => Some(arg),
        _ => None,
    }
}

/// Whether a float head is the one a deterministic encoder writes for its value: the shortest exact
/// width, and for every NaN, [`CANONICAL_NAN`].
pub fn deterministic(info: u8, arg: u64) -> bool {
    value(info, arg).is_some_and(|bits| {
        if nan(bits) {
            (info, arg) == (HALF, CANONICAL_NAN)
        } else {
            shortest(bits) == (info, arg)
        }
    })
}

/// Whether a float head, as [`shortest`] gives it, is a NaN.
pub fn nan_head((info, arg): (u8, u64)) -> bool {
    value(info, arg).is_some_and(nan)
}

const fn nan(bits: u64) -> bool {
    bits >> BINARY64_MANTISSA & BINARY64_SPECIAL == BINARY64_SPECIAL
        && bits & BINARY64_FRACTION != 0
}

/// The binary64 bits of the binary32 `bits`.
pub fn from_single(bits: u32) -> u64 {
    BINARY32.widen(bits.into())
}

/// The binary32 bits of the binary64 `bits`, when exact.
pub fn to_single(bits: u64) -> Option<u32> {
    BINARY32
        .narrow(bits)
        .and_then(|single| u32::try_from(single).ok())
}

/// The binary16 bits of the binary64 `bits`, when exact.
pub fn to_half(bits: u64) -> Option<u16> {
    BINARY16
        .narrow(bits)
        .and_then(|half| u16::try_from(half).ok())
}

/// The binary32 bits of the binary16 `bits`. Binary32 holds every binary16 value, so the fallback,
/// a NaN, is never taken.
pub fn half_to_single(bits: u16) -> u32 {
    to_single(BINARY16.widen(bits.into())).unwrap_or(u32::MAX)
}

impl Format {
    const fn bias(self) -> u64 {
        (1 << (self.exponent - 1)) - 1
    }

    const fn special(self) -> u64 {
        (1 << self.exponent) - 1
    }

    /// The low binary64 mantissa bits this format has no room for.
    const fn dropped(self) -> u32 {
        BINARY64_MANTISSA - self.mantissa
    }

    /// The binary64 biased exponent of this format's smallest normal.
    const fn lowest_normal(self) -> u64 {
        BINARY64_BIAS + 1 - self.bias()
    }

    /// `bits` of this format as binary64 bits, exact for every value, NaN payloads included.
    fn widen(self, bits: u64) -> u64 {
        let exponent = bits >> self.mantissa & self.special();
        let mantissa = bits & ((1 << self.mantissa) - 1);
        (bits >> (self.exponent + self.mantissa) & 1) << 63
            | match (exponent, mantissa.checked_ilog2()) {
                (0, None) => 0,
                (0, Some(top)) => {
                    (self.lowest_normal() + u64::from(top) - u64::from(self.mantissa))
                        << BINARY64_MANTISSA
                        | mantissa << (BINARY64_MANTISSA - top) & BINARY64_FRACTION
                }
                (special, _) if special == self.special() => {
                    BINARY64_SPECIAL << BINARY64_MANTISSA | mantissa << self.dropped()
                }
                (normal, _) => {
                    (normal + self.lowest_normal() - 1) << BINARY64_MANTISSA
                        | mantissa << self.dropped()
                }
            }
    }

    /// The binary64 `bits` in this format, when that is exact, NaN payloads included.
    fn narrow(self, bits: u64) -> Option<u64> {
        let sign = bits >> 63 << (self.exponent + self.mantissa);
        let mantissa = bits & BINARY64_FRACTION;
        let fits = mantissa.trailing_zeros() >= self.dropped();
        match bits >> BINARY64_MANTISSA & BINARY64_SPECIAL {
            BINARY64_SPECIAL => {
                fits.then(|| sign | self.special() << self.mantissa | mantissa >> self.dropped())
            }
            0 => (mantissa == 0).then_some(sign),
            normal if normal >= self.lowest_normal() => {
                (fits && normal - self.lowest_normal() + 1 < self.special()).then(|| {
                    sign | (normal - self.lowest_normal() + 1) << self.mantissa
                        | mantissa >> self.dropped()
                })
            }
            subnormal => {
                let shift = u32::try_from(self.lowest_normal() - subnormal).ok()? + self.dropped();
                let significand = 1 << BINARY64_MANTISSA | mantissa;
                (significand.trailing_zeros() >= shift).then(|| sign | significand >> shift)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        BINARY16, BINARY32, CANONICAL_NAN, DOUBLE, HALF, SINGLE, deterministic, from_single,
        half_to_single, shortest, to_half, to_single, value,
    };

    #[test]
    fn rfc8949_appendix_a_floats_take_their_shortest_width() {
        let cases: [(f64, u8, u64); 14] = [
            (0.0, HALF, 0x0000),
            (-0.0, HALF, 0x8000),
            (1.0, HALF, 0x3c00),
            (1.5, HALF, 0x3e00),
            (65504.0, HALF, 0x7bff),
            (5.960_464_477_539_063e-8, HALF, 0x0001),
            (6.103_515_625e-5, HALF, 0x0400),
            (-4.0, HALF, 0xc400),
            (f64::INFINITY, HALF, 0x7c00),
            (f64::NEG_INFINITY, HALF, 0xfc00),
            (100_000.0, SINGLE, 0x47c3_5000),
            (3.402_823_466_385_288_6e38, SINGLE, 0x7f7f_ffff),
            (1.1, DOUBLE, 0x3ff1_9999_9999_999a),
            (-4.1, DOUBLE, 0xc010_6666_6666_6666),
        ];
        for (number, info, bits) in cases {
            assert_eq!(shortest(number.to_bits()), (info, bits), "{number}");
            assert_eq!(value(info, bits), Some(number.to_bits()), "{number}");
        }
    }

    #[test]
    fn nan_payloads_survive_narrowing_only_when_exact() {
        assert_eq!(shortest(0x7ff8_0000_0000_0000), (HALF, 0x7e00));
        assert_eq!(shortest(0x7ff8_0000_2000_0000), (SINGLE, 0x7fc0_0001));
        assert_eq!(
            shortest(0x7ff8_0000_0000_0001),
            (DOUBLE, 0x7ff8_0000_0000_0001)
        );
    }

    #[test]
    fn single_nan_payloads_widen_exactly() {
        let cases: [(u32, u64, (u8, u64)); 4] = [
            (0x7f80_0001, 0x7ff0_0000_2000_0000, (SINGLE, 0x7f80_0001)),
            (0xff80_0001, 0xfff0_0000_2000_0000, (SINGLE, 0xff80_0001)),
            (0x7fff_ffff, 0x7fff_ffff_e000_0000, (SINGLE, 0x7fff_ffff)),
            (0x7fc0_0000, 0x7ff8_0000_0000_0000, (HALF, 0x7e00)),
        ];
        for (single, double, narrowest) in cases {
            assert_eq!(from_single(single), double, "{single:x}");
            assert_eq!(shortest(double), narrowest, "{single:x}");
            assert_eq!(to_single(double), Some(single), "{single:x}");
        }
    }

    #[test]
    fn every_binary16_narrows_back_and_is_shortest() {
        for half in (0..=u16::MAX).step_by(if cfg!(miri) { 257 } else { 1 }) {
            let widened = BINARY16.widen(half.into());
            assert_eq!(BINARY16.narrow(widened), Some(half.into()), "{half:04x}");
            assert_eq!(shortest(widened), (HALF, half.into()), "{half:04x}");
        }
    }

    #[test]
    fn every_binary16_is_exactly_a_binary32_and_narrows_back() {
        for half in (0..=u16::MAX).step_by(if cfg!(miri) { 257 } else { 1 }) {
            let widened = BINARY16.widen(half.into());
            assert_eq!(
                to_single(widened).map(from_single),
                Some(widened),
                "{half:04x}"
            );
            assert_eq!(
                half_to_single(half),
                to_single(widened).unwrap_or(0),
                "{half:04x}"
            );
            assert_eq!(to_half(widened), Some(half), "{half:04x}");
        }
        assert_eq!(to_half(from_single(0x3dcc_cccd)), None);
    }

    #[test]
    fn every_binary16_is_deterministic_but_the_other_nans() {
        for half in (0..=u16::MAX).step_by(if cfg!(miri) { 257 } else { 1 }) {
            let nan = half & 0x7c00 == 0x7c00 && half & 0x3ff != 0;
            assert_eq!(
                deterministic(HALF, half.into()),
                !nan || u64::from(half) == CANONICAL_NAN,
                "{half:04x}"
            );
            let widened = BINARY16.widen(half.into());
            assert!(!deterministic(DOUBLE, widened), "{half:04x}");
            assert!(
                BINARY32
                    .narrow(widened)
                    .is_none_or(|single| !deterministic(SINGLE, single)),
                "{half:04x}"
            );
        }
    }

    #[test]
    fn a_wider_float_is_deterministic_only_when_no_narrower_width_is_exact() {
        let cases: [(u8, u64, bool); 8] = [
            (SINGLE, 0x47c3_5000, true),
            (SINGLE, 0x3f80_0000, false),
            (SINGLE, 0x7fc0_0001, false),
            (DOUBLE, 0x3ff1_9999_9999_999a, true),
            (DOUBLE, 0x40f8_6a00_0000_0000, false),
            (DOUBLE, 0x7ff8_0000_0000_0001, false),
            (HALF, 0xfe00, false),
            (HALF, 0x7c00, true),
        ];
        for (info, arg, expected) in cases {
            assert_eq!(deterministic(info, arg), expected, "{info} {arg:x}");
        }
        assert!(!deterministic(24, 0));
    }

    #[test]
    fn single_boundaries_widen_to_their_ieee_value_and_narrow_back() {
        for single in [
            0x0000_0001_u32,
            0x0000_0003,
            0x007f_ffff,
            0x0080_0000,
            0x0080_0001,
            0x3380_0000,
            0x3380_0001,
            0x387f_c000,
            0x3880_0000,
            0x3f80_0000,
            0x477f_e000,
            0x477f_f000,
            0x4780_0000,
            0x7f7f_ffff,
            0x7f80_0000,
            0x8000_0000,
            0x8000_0001,
            0xff7f_ffff,
        ] {
            assert_eq!(
                from_single(single),
                f64::from(f32::from_bits(single)).to_bits(),
                "{single:08x}"
            );
            assert_eq!(to_single(from_single(single)), Some(single), "{single:08x}");
        }
    }

    #[test]
    fn doubles_between_the_formats_stay_double() {
        for double in [
            0x0000_0000_0000_0001_u64,
            0x000f_ffff_ffff_ffff,
            0x3690_0000_0000_0000,
            0x3e70_0000_0000_0001,
            0x47f0_0000_0000_0000,
            0x7fef_ffff_ffff_ffff,
            0x7ff0_0000_0000_0001,
        ] {
            assert_eq!(shortest(double), (DOUBLE, double), "{double:016x}");
            assert_eq!(to_single(double), None, "{double:016x}");
        }
        assert_eq!(shortest(0x36a0_0000_0000_0000), (SINGLE, 0x0000_0001));
    }
}

#[cfg(kani)]
mod proofs {
    use super::{BINARY16, BINARY32, shortest, value};

    #[kani::proof]
    fn every_binary16_round_trips() {
        let half: u16 = kani::any();
        assert_eq!(
            BINARY16.narrow(BINARY16.widen(half.into())),
            Some(half.into())
        );
    }

    #[kani::proof]
    fn every_binary32_round_trips() {
        let single: u32 = kani::any();
        assert_eq!(
            BINARY32.narrow(BINARY32.widen(single.into())),
            Some(single.into())
        );
    }

    #[kani::proof]
    fn narrowing_happens_only_when_exact() {
        let bits: u64 = kani::any();
        if let Some(half) = BINARY16.narrow(bits) {
            assert_eq!(BINARY16.widen(half), bits);
        }
        if let Some(single) = BINARY32.narrow(bits) {
            assert_eq!(BINARY32.widen(single), bits);
        }
    }

    #[kani::proof]
    fn the_shortest_encoding_decodes_to_the_same_bits() {
        let bits: u64 = kani::any();
        let (info, arg) = shortest(bits);
        assert_eq!(value(info, arg), Some(bits));
    }
}

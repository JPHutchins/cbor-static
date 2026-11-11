#![forbid(unsafe_code)]
//! Shortest value-preserving float widths (RFC 8949 §4.1), with in-crate binary16 conversion.

pub const HALF: u8 = 25;
pub const SINGLE: u8 = 26;
pub const DOUBLE: u8 = 27;

const F64_MANTISSA: u64 = (1 << 52) - 1;
const F64_TO_F16_DROPPED: u64 = (1 << 42) - 1;
const F64_TO_F32_DROPPED: u64 = (1 << 29) - 1;

/// The additional information and bits of the narrowest encoding that preserves `value`,
/// NaN payloads included.
pub fn shortest(value: f64) -> (u8, u64) {
    to_f16(value).map_or_else(
        || {
            to_f32(value).map_or((DOUBLE, value.to_bits()), |single| {
                (SINGLE, u64::from(single.to_bits()))
            })
        },
        |half| (HALF, u64::from(half)),
    )
}

/// The value of a float header's argument.
pub fn value(info: u8, bits: u64) -> Option<f64> {
    match info {
        HALF => u16::try_from(bits).ok().map(from_f16),
        SINGLE => u32::try_from(bits)
            .ok()
            .map(|b| f64::from(f32::from_bits(b))),
        DOUBLE => Some(f64::from_bits(bits)),
        _ => None,
    }
}

/// `value` as `f32` when that loses nothing, NaN payloads included.
pub fn to_f32(value: f64) -> Option<f32> {
    let bits = value.to_bits();
    if value.is_nan() {
        (bits & F64_TO_F32_DROPPED == 0).then(|| {
            let sign = u32::from(bits >> 63 == 1) << 31;
            let payload = u32::try_from((bits & F64_MANTISSA) >> 29).unwrap_or(0);
            f32::from_bits(sign | 0x7f80_0000 | payload)
        })
    } else {
        #[expect(
            clippy::cast_possible_truncation,
            reason = "checked exact by the round trip"
        )]
        let single = value as f32;
        (f64::from(single).to_bits() == bits).then_some(single)
    }
}

fn to_f16(value: f64) -> Option<u16> {
    let bits = value.to_bits();
    let sign = u16::from(bits >> 63 == 1) << 15;
    let exponent = i32::try_from((bits >> 52) & 0x7ff).unwrap_or(0);
    let mantissa = bits & F64_MANTISSA;
    let high_mantissa = u16::try_from(mantissa >> 42).unwrap_or(0);
    match exponent {
        0x7ff => (mantissa & F64_TO_F16_DROPPED == 0).then_some(sign | 0x7c00 | high_mantissa),
        0 => (mantissa == 0).then_some(sign),
        _ => match exponent - 1023 {
            unbiased @ -14..=15 => (mantissa & F64_TO_F16_DROPPED == 0)
                .then(|| sign | u16::try_from(unbiased + 15).unwrap_or(0) << 10 | high_mantissa),
            unbiased @ -24..-14 => {
                let shift = u32::try_from(28 - unbiased).unwrap_or(64);
                let significand = (1u64 << 52) | mantissa;
                (significand.trailing_zeros() >= shift)
                    .then(|| sign | u16::try_from(significand >> shift).unwrap_or(0))
            }
            _ => None,
        },
    }
}

fn from_f16(half: u16) -> f64 {
    let magnitude = match (half >> 10) & 0x1f {
        0 => f64::from(half & 0x3ff) * (1.0 / 16_777_216.0),
        exponent => f64::from_bits(
            match exponent {
                0x1f => 0x7ff << 52,
                _ => (u64::from(exponent) + 1008) << 52,
            } | u64::from(half & 0x3ff) << 42,
        ),
    };
    if half >> 15 == 1 {
        -magnitude
    } else {
        magnitude
    }
}

#[cfg(test)]
mod tests {
    use super::{DOUBLE, HALF, SINGLE, shortest, value};

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
            assert_eq!(shortest(number), (info, bits), "{number}");
            assert_eq!(
                value(info, bits).map(f64::to_bits),
                Some(number.to_bits()),
                "{number}"
            );
        }
    }

    #[test]
    fn nan_payloads_survive_narrowing_only_when_exact() {
        assert_eq!(
            shortest(f64::from_bits(0x7ff8_0000_0000_0000)),
            (HALF, 0x7e00)
        );
        assert_eq!(
            shortest(f64::from_bits(0x7ff8_0000_2000_0000)),
            (SINGLE, 0x7fc0_0001)
        );
        assert_eq!(
            shortest(f64::from_bits(0x7ff8_0000_0000_0001)),
            (DOUBLE, 0x7ff8_0000_0000_0001)
        );
    }

    #[test]
    fn half_subnormals_round_trip_exactly() {
        for half in [0x0001_u16, 0x0003, 0x03ff, 0x8200] {
            let number = value(HALF, u64::from(half)).unwrap_or(f64::NAN);
            assert_eq!(shortest(number), (HALF, u64::from(half)));
        }
        assert_eq!(shortest(1.5 * 5.960_464_477_539_063e-8).0, SINGLE);
    }
}

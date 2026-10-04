//! Rating-control geometry shared by saved dialog sizes and their consumers.

/// Width of Qt's counter rectangle for a height and an integral rating.
pub fn counter_width(height: f64, value: i64) -> f64 {
    let digits = f64::from(value.max(1).unsigned_abs().ilog10() + 1);
    (height * 2.0
        + if digits > 3.0 {
            (height - 1.0) * (digits - (2.0 + digits / 3.0))
        } else {
            0.0
        })
    .trunc()
}

/// Round a finite rating pixel size in 1–255 to two decimals as Qt does.
///
/// Multiplying the binary input by 100 in floating point can erase which side
/// of a decimal half it lies on. Scale its exact mantissa instead, then round
/// halves away from zero. The validated positive bounds keep the integer
/// numerator and denominator within u64.
pub fn round_hundredths(value: f64) -> f64 {
    debug_assert!((1.0..=255.0).contains(&value));
    let bits = value.to_bits();
    let mantissa = (bits & ((1_u64 << 52) - 1)) | (1_u64 << 52);
    let exponent = (bits >> 52) & 0x7ff;
    let denominator = 1_u64 << (1075 - exponent);
    let numerator = mantissa * 100;
    ((numerator + denominator / 2) / denominator) as f64 / 100.0
}

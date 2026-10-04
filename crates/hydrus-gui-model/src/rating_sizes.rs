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

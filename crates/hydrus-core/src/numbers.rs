//! Numbers as the reference writes them in human-readable text.

/// An integer with thousands separators: `1234567` -> `1,234,567`
/// (the reference's `HydrusNumbers.ToHumanInt`).
pub fn human_int(n: u64) -> String {
    let digits = n.to_string();
    let mut out = String::with_capacity(digits.len() + digits.len() / 3);
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(c);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn groups_thousands() {
        assert_eq!(human_int(0), "0");
        assert_eq!(human_int(999), "999");
        assert_eq!(human_int(1000), "1,000");
        assert_eq!(human_int(1_234_567), "1,234,567");
    }
}

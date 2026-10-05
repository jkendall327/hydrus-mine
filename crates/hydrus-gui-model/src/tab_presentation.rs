//! Middle name elision. Font measurement and fitting belong to each native tab.
/// Retain both ends around Qt's single-character middle ellipsis.
#[must_use]
pub fn middle_name(text: &str, kept: usize) -> String {
    let chars = text.chars().collect::<Vec<_>>();
    if kept >= chars.len() {
        return text.to_owned();
    }
    let prefix = kept.div_ceil(2);
    let suffix = kept / 2;
    chars[..prefix]
        .iter()
        .chain(std::iter::once(&'…'))
        .chain(chars[chars.len() - suffix..].iter())
        .collect()
}

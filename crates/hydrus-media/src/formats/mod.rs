//! Format-specific parsing: containers, headers and metadata that are not
//! plain image decoding.

pub(crate) mod apng;
pub(crate) mod archive;
pub(crate) mod clip;
pub(crate) mod flash;
pub(crate) mod gif;
pub(crate) mod isobmff;
pub(crate) mod ole;
pub(crate) mod pdf;
pub(crate) mod pdn;
pub(crate) mod psd;
pub(crate) mod svg;
pub(crate) mod update;
pub(crate) mod webp;

/// Run a third-party parser or renderer on untrusted input without letting a
/// panic inside it take the caller down.
pub(crate) fn guarded<T>(f: impl FnOnce() -> Option<T>) -> Option<T> {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(f))
        .ok()
        .flatten()
}

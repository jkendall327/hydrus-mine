//! Format-specific parsing: containers, headers and metadata that are not
//! plain image decoding.

pub(crate) mod apng;
pub(crate) mod archive;
pub(crate) mod clip;
pub(crate) mod flash;
pub(crate) mod gif;
pub(crate) mod isobmff;
pub(crate) mod ole;
pub(crate) mod pdn;
pub(crate) mod psd;
pub(crate) mod svg;
pub(crate) mod webp;

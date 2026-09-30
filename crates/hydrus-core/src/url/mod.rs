//! URLs as hydrus sees them.
//!
//! The reference defines URL handling in terms of Python's `urllib.parse`
//! ([`pyurl`] reproduces the parts it uses) plus its own re-encoding rules
//! ([`functions`]). Stored URLs are whatever those produced, so looking a
//! URL up needs them exactly.
//!
//! [`UrlClass`]es recognise the URLs of known sites and say how to normalise
//! them; [`UrlClasses`] holds a client's classes and answers questions about
//! any URL.

pub mod class;
pub mod functions;
pub mod psl;
pub mod pyurl;
pub mod registry;
pub mod strings;

pub use class::{DomainMask, UrlClass, UrlClassError, UrlParameter, UrlType};
pub use functions::{UrlError, ensure_url_is_encoded, search_urls, url_domain};
pub use registry::{ParseCapability, UrlClassSettings, UrlClasses};
pub use strings::{StringConverter, StringMatch, StringProcessor};

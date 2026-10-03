//! The downloader's parsing engine: what the reference's parsers do to HTML
//! and JSON documents, as pure functions (no network, no database).

pub mod content;
pub mod dom;
pub mod downloaders;
pub mod folders;
pub mod formula;
pub mod sidecar;
pub mod simple;
pub mod text;

pub use content::{
    ContentKind, ContentParser, PageParser, ParseFailure, ParsedContent, ParsedPost,
};
pub use downloaders::{Downloaders, Unconverted};
pub use formula::{Formula, FormulaKind, ParseError, ParsingContext};

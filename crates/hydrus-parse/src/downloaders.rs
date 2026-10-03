//! A client's downloader definitions: its gallery URL generators and page
//! parsers. (Its URL classes, which link URLs to parsers, are kept with the
//! rest of URL handling in `hydrus_core::url`.)

use serde::{Deserialize, Serialize};

use hydrus_core::url::Gugs;

use crate::content::PageParser;

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Downloaders {
    pub gugs: Gugs,
    /// In the user's order.
    pub parsers: Vec<PageParser>,
    /// Definitions migration could not convert. They stay in the legacy
    /// copy of the database until support for them is added.
    pub unconverted: Vec<Unconverted>,
}

/// A definition that could not be converted.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Unconverted {
    /// `gug` or `parser`.
    pub kind: String,
    pub name: String,
    pub reason: String,
}

impl Downloaders {
    /// The parser with this key (hex).
    pub fn parser(&self, key: &str) -> Option<&PageParser> {
        self.parsers.iter().find(|p| p.key == key)
    }

    /// Every namespace the parsers parse, sorted (the reference's
    /// `GetParserNamespaces`, offered by the tag filter editor).
    pub fn parser_namespaces(&self) -> Vec<String> {
        let mut all = std::collections::BTreeSet::new();
        for parser in &self.parsers {
            all.extend(parser.namespaces());
        }
        all.into_iter().collect()
    }
}

//! The simple downloader's parsing formulae
//! (`SimpleDownloaderParsingFormula`): a name and a formula, run on a page
//! to find the URLs of files to download.

use serde::{Deserialize, Serialize};

use crate::formula::{Formula, ParseError, ParsingContext};

/// A named formula for the simple downloader.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SimpleFormula {
    pub name: String,
    pub formula: Formula,
}

impl SimpleFormula {
    /// `_WorkOnGallery`'s parse of a page: what the formula finds in it
    /// (with the page's URL as the parsing context, newlines collapsed),
    /// each joined to the page's URL.
    pub fn file_urls(&self, page_url: &str, text: &str) -> Result<Vec<String>, ParseError> {
        let context = ParsingContext::from([("url".to_owned(), page_url.to_owned())]);
        Ok(self
            .formula
            .parse(&context, text, true)?
            .iter()
            .map(|found| hydrus_core::url::pyurl::urljoin(page_url, found))
            .collect())
    }
}

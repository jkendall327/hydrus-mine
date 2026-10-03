//! Gallery searches: a downloader page's searches (`GalleryImport`), each
//! reading a downloader's result pages for one query.

/// A gallery search's state (`GalleryImport`), kept as its queue's extra.
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct GallerySearch {
    pub query: String,
    /// The downloader's name.
    pub source_name: String,
    /// Stop reading pages after this many new files (`None`: no limit).
    pub file_limit: Option<u64>,
    pub num_new_urls_found: u64,
    pub num_urls_found: u64,
}

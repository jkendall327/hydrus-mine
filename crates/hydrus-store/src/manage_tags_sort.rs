//! Independent opening defaults for the two Manage Tags presentation contexts.
use crate::settings::Setting;
use hydrus_core::tag_sort::TagSort;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Context {
    SearchPage,
    MediaViewer,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Sort {
    pub order: TagSort,
    pub use_siblings: bool,
}
impl Default for Sort {
    fn default() -> Self {
        Self {
            order: TagSort::DEFAULT,
            use_siblings: true,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub search_page: Sort,
    pub media_viewer: Sort,
}
impl Settings {
    pub fn get(self, context: Context) -> Sort {
        match context {
            Context::SearchPage => self.search_page,
            Context::MediaViewer => self.media_viewer,
        }
    }
}
impl Setting for Settings {
    const KEY: &'static str = "manage_tags_sort";
}

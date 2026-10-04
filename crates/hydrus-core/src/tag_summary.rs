//! Tag summaries (the reference's `TagSummaryGenerator`): a line of text
//! made from a file's tags in some namespaces, as the reference draws over
//! its thumbnails ("creator - series - title" across the top, "v3-c10-p330-331"
//! at the bottom right) and over the media viewer.

use serde::{Deserialize, Serialize};

/// One namespace a summary shows: its subtags after `prefix`, joined by
/// `separator`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NamespaceInfo {
    pub namespace: String,
    pub prefix: String,
    pub separator: String,
}

impl NamespaceInfo {
    fn new(namespace: &str, prefix: &str, separator: &str) -> Self {
        Self {
            namespace: namespace.to_owned(),
            prefix: prefix.to_owned(),
            separator: separator.to_owned(),
        }
    }
}

/// How to summarise a file's tags, and the colours to draw it in (RGBA).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TagSummaryGenerator {
    pub background: [u8; 4],
    pub text: [u8; 4],
    pub namespace_info: Vec<NamespaceInfo>,
    /// Between the namespaces' texts.
    pub separator: String,
    /// What the options show it making.
    pub example_tags: Vec<String>,
    /// Whether it makes anything at all.
    pub show: bool,
}

const BACKGROUND: [u8; 4] = [223, 227, 230, 255];
const TEXT: [u8; 4] = [1, 17, 26, 255];

impl TagSummaryGenerator {
    fn made(namespace_info: Vec<NamespaceInfo>, separator: &str, example_tags: &[&str]) -> Self {
        Self {
            background: BACKGROUND,
            text: TEXT,
            namespace_info,
            separator: separator.to_owned(),
            example_tags: example_tags.iter().map(|t| (*t).to_owned()).collect(),
            show: true,
        }
    }

    fn titles() -> Vec<NamespaceInfo> {
        vec![
            NamespaceInfo::new("creator", "", ", "),
            NamespaceInfo::new("series", "", ", "),
            NamespaceInfo::new("title", "", ", "),
        ]
    }

    fn numbers() -> Vec<NamespaceInfo> {
        vec![
            NamespaceInfo::new("volume", "v", "-"),
            NamespaceInfo::new("chapter", "c", "-"),
            NamespaceInfo::new("page", "p", "-"),
        ]
    }

    /// Across the top of a thumbnail, by default: "creator - series -
    /// title".
    pub fn thumbnail_top() -> Self {
        Self::made(
            Self::titles(),
            " - ",
            &["creator:creator", "series:series", "title:title"],
        )
    }

    /// At a thumbnail's bottom right, by default: "v3-c10-p330-331".
    pub fn thumbnail_bottom_right() -> Self {
        Self::made(
            Self::numbers(),
            "-",
            &["volume:3", "chapter:10", "page:330", "page:331"],
        )
    }

    /// Over the media viewer's top, by default: both of those.
    pub fn media_viewer_top() -> Self {
        let mut info = Self::titles();
        info.extend(Self::numbers());
        Self::made(
            info,
            " - ",
            &[
                "creator:creator",
                "series:series",
                "title:title",
                "volume:1",
                "chapter:1",
                "page:1",
            ],
        )
    }

    /// The summary of `tags` (`GenerateSummary`): each namespace's subtags,
    /// shown as `render` shows them, sorted in human order, and a run of
    /// three or more numbers given as its first and last; each namespace's
    /// prefix and subtags joined by its separator, the namespaces joined by
    /// the generator's. Empty if it doesn't show.
    pub fn summary<'a>(
        &self,
        tags: impl IntoIterator<Item = &'a str>,
        render: impl Fn(&str) -> String,
    ) -> String {
        if !self.show {
            return String::new();
        }
        // Duplicate namespace rows repeat their presentation, not their tags.
        let mut subtags: std::collections::BTreeMap<&str, Vec<String>> = self
            .namespace_info
            .iter()
            .map(|i| (i.namespace.as_str(), Vec::new()))
            .collect();
        for tag in tags {
            let (namespace, subtag) = crate::tag::split_tag(tag);
            if let Some(found) = subtags.get_mut(namespace) {
                found.push(render(subtag));
            }
        }
        let mut texts = Vec::new();
        for info in &self.namespace_info {
            let Some(found) = subtags.get(info.namespace.as_str()) else {
                continue;
            };
            if found.is_empty() {
                continue;
            }
            let mut sorted = found.clone();
            crate::sort::human_sort(&mut sorted);
            // (`CollapseMultipleSortedNumericTagsToMinMax`)
            let decimal = |s: &String| crate::tag_presentation::is_decimal(s);
            if sorted.len() > 2 && sorted.iter().all(decimal) {
                sorted = vec![sorted[0].clone(), sorted[sorted.len() - 1].clone()];
            }
            texts.push(format!("{}{}", info.prefix, sorted.join(&info.separator)));
        }
        texts.join(&self.separator)
    }
}

/// The summaries the reference draws: over thumbnails, and over the media
/// viewer (`tag_summary_generators`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct TagSummaries {
    pub thumbnail_top: TagSummaryGenerator,
    pub thumbnail_bottom_right: TagSummaryGenerator,
    pub media_viewer_top: TagSummaryGenerator,
}

impl Default for TagSummaries {
    fn default() -> Self {
        Self {
            thumbnail_top: TagSummaryGenerator::thumbnail_top(),
            thumbnail_bottom_right: TagSummaryGenerator::thumbnail_bottom_right(),
            media_viewer_top: TagSummaryGenerator::media_viewer_top(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn plain(subtag: &str) -> String {
        subtag.to_owned()
    }

    #[test]
    fn the_defaults_summarise_as_their_examples_say() {
        let top = TagSummaryGenerator::thumbnail_top();
        assert_eq!(
            top.summary(top.example_tags.iter().map(String::as_str), plain),
            "creator - series - title"
        );
        let bottom = TagSummaryGenerator::thumbnail_bottom_right();
        assert_eq!(
            bottom.summary(bottom.example_tags.iter().map(String::as_str), plain),
            "v3-c10-p330-331"
        );
        // a run of numbers is its first and last
        assert_eq!(
            bottom.summary(["page:3", "page:1", "page:10", "page:2"], plain),
            "p1-10"
        );
        let mut hidden = top.clone();
        hidden.show = false;
        assert_eq!(hidden.summary(["creator:x"], plain), "");
    }
}

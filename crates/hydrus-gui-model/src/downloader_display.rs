//! Detached downloader selector and media-viewer URL display choices.
use hydrus_core::url::{UrlClassSettings, UrlClasses, functions::url_domain};
use hydrus_parse::Downloaders;
use hydrus_store::{
    Store,
    settings::{self, Setting},
};
use serde::{Deserialize, Serialize};

/// Native URL display preferences. Until explicitly edited, every installed
/// class is displayed. This avoids introducing site-specific default lists.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ViewerUrls {
    pub class_keys_to_display: Option<Vec<String>>,
    pub show_unmatched: bool,
}
impl Default for ViewerUrls {
    fn default() -> Self {
        Self {
            class_keys_to_display: None,
            show_unmatched: true,
        }
    }
}
impl Setting for ViewerUrls {
    const KEY: &'static str = "media_viewer_url_display";
}
impl ViewerUrls {
    pub fn displays(&self, key: &str) -> bool {
        self.class_keys_to_display
            .as_ref()
            .is_none_or(|keys| keys.iter().any(|k| k == key))
    }
}
/// One immutable definition identity and its editable display state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Row {
    pub key: String,
    pub name: String,
    pub url_type: String,
    pub display: bool,
    original: bool,
}
impl Row {
    pub fn cells(&self, classes: bool) -> Vec<String> {
        let mut cells = vec![self.name.clone()];
        if classes {
            cells.push(self.url_type.clone());
        }
        cells.push(if self.display { "yes" } else { "no" }.into());
        cells
    }
}
#[derive(Debug, Clone)]
pub struct Draft {
    pub gugs: Vec<Row>,
    pub classes: Vec<Row>,
    pub show_unmatched: bool,
    original_unmatched: bool,
}
impl Draft {
    pub fn load(store: &Store) -> hydrus_store::Result<Self> {
        store.read(|conn| {
            Ok(Self::new(
                &settings::get(conn)?,
                &settings::get(conn)?,
                &settings::get(conn)?,
            ))
        })
    }
    pub fn new(downloaders: &Downloaders, classes: &UrlClassSettings, viewer: &ViewerUrls) -> Self {
        let row = |key: String, name: String, url_type: String, display| Row {
            key,
            name,
            url_type,
            display,
            original: display,
        };
        let mut gugs: Vec<_> = downloaders
            .gugs
            .gugs
            .iter()
            .map(|g| {
                row(
                    g.key().into(),
                    g.name().into(),
                    String::new(),
                    downloaders
                        .gugs
                        .keys_to_display
                        .iter()
                        .any(|k| k == g.key()),
                )
            })
            .collect();
        let mut classes: Vec<_> = classes
            .url_classes
            .iter()
            .map(|c| {
                let key = hex::encode(&c.key);
                let display = viewer.displays(&key);
                row(
                    key,
                    c.name.clone(),
                    c.url_type.name().unwrap_or("source url").into(),
                    display,
                )
            })
            .collect();
        gugs.sort_by(|a, b| a.name.cmp(&b.name));
        classes.sort_by(|a, b| a.name.cmp(&b.name));
        Self {
            gugs,
            classes,
            show_unmatched: viewer.show_unmatched,
            original_unmatched: viewer.show_unmatched,
        }
    }
    pub fn rows(&self, classes: bool) -> &[Row] {
        if classes { &self.classes } else { &self.gugs }
    }
    /// Only changed identities are merged into current settings; concurrent
    /// parser/definition edits and other display choices survive Apply.
    pub fn save(&self, store: &Store) -> hydrus_store::Result<()> {
        let gugs: Vec<_> = self
            .gugs
            .iter()
            .filter(|r| r.display != r.original)
            .cloned()
            .collect();
        let classes: Vec<_> = self
            .classes
            .iter()
            .filter(|r| r.display != r.original)
            .cloned()
            .collect();
        let unmatched =
            (self.show_unmatched != self.original_unmatched).then_some(self.show_unmatched);
        store.write(move |ctx| {
            let conn = ctx.conn();
            if !gugs.is_empty() {
                let mut current: Downloaders = settings::get(conn)?;
                for row in gugs {
                    if current.gugs.gugs.iter().any(|g| g.key() == row.key) {
                        set_key(&mut current.gugs.keys_to_display, &row.key, row.display);
                    }
                }
                settings::set(conn, &current)?;
            }
            if !classes.is_empty() || unmatched.is_some() {
                let mut viewer: ViewerUrls = settings::get(conn)?;
                if !classes.is_empty() {
                    let current: UrlClassSettings = settings::get(conn)?;
                    let available: Vec<_> = current
                        .url_classes
                        .iter()
                        .map(|c| hex::encode(&c.key))
                        .collect();
                    let keys = viewer
                        .class_keys_to_display
                        .get_or_insert_with(|| available.clone());
                    for row in classes {
                        if available.contains(&row.key) {
                            set_key(keys, &row.key, row.display);
                        }
                    }
                }
                if let Some(show) = unmatched {
                    viewer.show_unmatched = show;
                }
                settings::set(conn, &viewer)?;
            }
            Ok(())
        })
    }
    pub fn answer(&mut self, classes: bool, indices: &[usize], answer: Option<bool>) {
        let Some(show) = answer else {
            return;
        };
        let rows = if classes {
            &mut self.classes
        } else {
            &mut self.gugs
        };
        for &i in indices {
            if let Some(row) = rows.get_mut(i) {
                row.display = show;
            }
        }
    }
}
fn set_key(keys: &mut Vec<String>, key: &str, show: bool) {
    keys.retain(|k| k != key);
    if show {
        keys.push(key.into());
    }
}
/// Exact batch question wording, including the reference's compact list summary.
pub fn question(rows: &[Row], selected: &[usize], classes: bool) -> (String, String) {
    let mut names: Vec<_> = selected
        .iter()
        .filter_map(|&i| rows.get(i).map(|r| r.name.clone()))
        .collect();
    hydrus_core::sort::human_sort(&mut names);
    let summary = if names.len() == 1 {
        format!(" \"{}\" ", names[0])
    } else {
        let mut lines = Vec::new();
        if names.len() <= 4 {
            lines = names;
        } else {
            let mut line = String::new();
            for (i, name) in names.iter().enumerate() {
                if !line.is_empty() && line.chars().count() + name.chars().count() + 2 > 64 {
                    lines.push(std::mem::take(&mut line));
                    if lines.len() >= 24 {
                        lines.push(format!(
                            "and {} others",
                            hydrus_core::numbers::human_int(
                                u64::try_from(names.len() - i).unwrap_or(u64::MAX)
                            )
                        ));
                        break;
                    }
                }
                if !line.is_empty() {
                    line.push_str(", ");
                }
                line.push_str(name);
            }
            if !line.is_empty() {
                lines.push(line);
            }
        }
        format!("\n\n{}\n\n", lines.join("\n"))
    };
    if classes {
        (
            "Show in the media viewer?".into(),
            format!("Show{summary}in the media viewer?"),
        )
    } else {
        (
            "Show in the first list?".into(),
            format!("Show{summary}in the main selector list?"),
        )
    }
}
/// Class links first, unmatched domains second, each sorted by (label, URL).
/// Ten displayed matched URLs end the scan, as the reference does.
pub fn viewer_links(
    urls: &[String],
    classes: &UrlClasses,
    settings: &ViewerUrls,
) -> Vec<(String, String)> {
    let mut matched = Vec::new();
    let mut unmatched = Vec::new();
    for url in urls {
        if let Some(class) = classes.class_for(url) {
            if settings.displays(&hex::encode(&class.key)) {
                let (raw, regex, _) = class.domain_mask.grouping_key();
                let label = if raw.len() == 1 && regex.is_empty() {
                    class.name.clone()
                } else {
                    format!(
                        "{} ({})",
                        class.name,
                        url_domain(url).unwrap_or_else(|_| "unknown".into())
                    )
                };
                matched.push((label, url.clone()));
            }
        } else if settings.show_unmatched {
            unmatched.push((
                url_domain(url).unwrap_or_else(|_| "unknown".into()),
                url.clone(),
            ));
        }
        if matched.len() == 10 {
            break;
        }
    }
    matched.sort();
    unmatched.sort();
    matched.extend(unmatched);
    matched
}

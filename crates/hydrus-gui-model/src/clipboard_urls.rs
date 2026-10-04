//! Changed clipboard text routed through the client's current URL classes.

use hydrus_core::url::functions::{check_full_url, ensure_url_is_encoded};
use hydrus_core::url::{UrlClasses, UrlType};
use hydrus_store::settings::ClipboardUrls;

/// The importer kind that accepts a recognised clipboard URL.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Destination {
    Watchers,
    Urls,
}

/// A URL normalised for its destination importer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Routed {
    pub url: String,
    pub destination: Destination,
}

/// Clipboard change detection. A toggle clears its remembered text, as in Hydrus.
#[derive(Debug, Default)]
pub struct Watcher {
    settings: ClipboardUrls,
    last_text: String,
    failed: bool,
}

impl Watcher {
    /// A successful menu toggle lets the current clipboard be examined again.
    pub fn reset(&mut self) {
        self.last_text.clear();
        self.failed = false;
    }

    /// Refresh switches and report whether a clipboard read is needed.
    pub fn reading(&mut self, settings: ClipboardUrls) -> bool {
        if self.settings != settings {
            self.settings = settings;
            self.reset();
        }
        settings.enabled() && !self.failed
    }

    /// Suspend reads after a clipboard access failure, until a switch changes.
    pub fn failed(&mut self) {
        self.failed = true;
    }

    /// Route a changed clipboard value; unavailable text is treated as empty.
    pub fn changed(&mut self, text: Option<&str>, classes: &UrlClasses) -> Vec<Routed> {
        let text = text.unwrap_or_default();
        if !self.settings.enabled() || self.failed || text == self.last_text {
            return Vec::new();
        }
        text.clone_into(&mut self.last_text);
        text.lines()
            .map(|line| line.trim_matches(|c: char| c.is_whitespace() || c == '\u{feff}'))
            .filter_map(|line| route(line, self.settings, classes))
            .collect()
    }
}

/// Match the reference's automatic import policy: recognised, enabled and parseable.
pub fn route(text: &str, settings: ClipboardUrls, classes: &UrlClasses) -> Option<Routed> {
    if !text.starts_with("http") || check_full_url(text).is_err() {
        return None;
    }
    let encoded = ensure_url_is_encoded(text, false, classes.settings().collapse_leading_slashes);
    let url = classes.normalise(&encoded, true).ok()?;
    let capability = classes.parse_capability(&url);
    if matches!(
        capability.url_type,
        UrlType::Post | UrlType::Gallery | UrlType::Watchable
    ) && capability.parser.is_err()
    {
        return None;
    }
    let destination = match capability.url_type {
        UrlType::Watchable if settings.watchers => Destination::Watchers,
        UrlType::File | UrlType::Post | UrlType::Gallery if settings.other_recognised => {
            Destination::Urls
        }
        _ => return None,
    };
    Some(Routed { url, destination })
}

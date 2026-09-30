//! Gallery URL generators (GUGs): how a downloader turns a search into the
//! URL of its first results page.
//!
//! A GUG fills its URL template with the search's terms; a nested GUG runs
//! several GUGs on the same search. The behaviour is the reference's
//! `ClientNetworkingGUG`, checked on `oracle/fixtures/gugs.json`.

use serde::{Deserialize, Serialize};

use super::functions::{PATH_EXCEPTION_CHARS, ensure_component_is_encoded, ensure_url_is_encoded};

/// A gallery URL generator.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Gug {
    pub name: String,
    /// The reference's key, in hex.
    pub key: String,
    /// The URL, with the replacement phrase where the search terms go.
    pub url_template: String,
    pub replacement_phrase: String,
    /// Put between the search's terms.
    pub separator: String,
    /// What a new search box starts with.
    pub initial_search_text: String,
    pub example_search_text: String,
}

/// A gallery URL generator that runs several others, named by key and name.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NestedGug {
    pub name: String,
    pub key: String,
    pub initial_search_text: String,
    /// `(key in hex, name)` of each GUG it runs.
    pub gugs: Vec<(String, String)>,
}

/// Either kind, as a downloader is chosen.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AnyGug {
    Single(Gug),
    Nested(NestedGug),
}

/// Why a GUG could not make a URL.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum GugError {
    #[error("No replacement phrase!")]
    NoReplacementPhrase,
    #[error("Replacement phrase not in URL template!")]
    PhraseNotInTemplate,
}

/// Client options that change how searches become URLs.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct GugOptions {
    /// Read `%20` in a search as a space (the reference's
    /// `replace_percent_twenty_with_space_in_gug_input`).
    pub percent_twenty_is_space: bool,
    /// Treat every leading `/` of a URL path as one.
    pub collapse_leading_slashes: bool,
}

impl Gug {
    /// The first gallery page's URL for a search: its space-separated terms
    /// encoded (fully if they go into the query, as a path component if
    /// they go into the path), joined with the separator and put in place of
    /// the replacement phrase.
    pub fn gallery_url(&self, query: &str, options: GugOptions) -> Result<String, GugError> {
        if self.replacement_phrase.is_empty() {
            return Err(GugError::NoReplacementPhrase);
        }
        if !self.url_template.contains(&self.replacement_phrase) {
            return Err(GugError::PhraseNotInTemplate);
        }
        let query = if options.percent_twenty_is_space {
            query.replace("%20", " ")
        } else {
            query.to_owned()
        };
        let into_params = self
            .url_template
            .split_once('?')
            .is_some_and(|(_, params)| params.contains(&self.replacement_phrase));
        // a tag like `6+girls` must become `6%2Bgirls` in a query, where `+`
        // would otherwise separate terms
        let safe = if into_params {
            ""
        } else {
            PATH_EXCEPTION_CHARS
        };
        let terms: Vec<String> = query
            .split(' ')
            .map(|term| ensure_component_is_encoded(term, safe))
            .collect();
        let url = self
            .url_template
            .replace(&self.replacement_phrase, &terms.join(&self.separator));
        Ok(ensure_url_is_encoded(
            &url,
            true,
            options.collapse_leading_slashes,
        ))
    }

    pub fn example_url(&self, options: GugOptions) -> Result<String, GugError> {
        self.gallery_url(&self.example_search_text, options)
    }
}

impl AnyGug {
    pub fn name(&self) -> &str {
        match self {
            AnyGug::Single(g) => &g.name,
            AnyGug::Nested(g) => &g.name,
        }
    }

    pub fn key(&self) -> &str {
        match self {
            AnyGug::Single(g) => &g.key,
            AnyGug::Nested(g) => &g.key,
        }
    }

    pub fn initial_search_text(&self) -> &str {
        match self {
            AnyGug::Single(g) => &g.initial_search_text,
            AnyGug::Nested(g) => &g.initial_search_text,
        }
    }
}

/// A client's GUGs, found as the reference finds them: by key, else by name.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Gugs {
    /// In the user's order.
    pub gugs: Vec<AnyGug>,
    /// Keys (hex) of those offered in the downloader list.
    pub keys_to_display: Vec<String>,
}

impl Gugs {
    /// The GUG with this key, or else this name. An empty name finds
    /// nothing (the reference's "no downloader").
    pub fn get(&self, key: &str, name: &str) -> Option<&AnyGug> {
        if name.is_empty() {
            return None;
        }
        // later ones win, as in the reference's lookup dictionaries
        self.gugs
            .iter()
            .rev()
            .find(|g| g.key() == key)
            .or_else(|| self.gugs.iter().rev().find(|g| g.name() == name))
    }

    /// Every gallery URL a search starts from: one for a GUG; one per
    /// GUG a nested GUG names and that still exists.
    pub fn gallery_urls(
        &self,
        gug: &AnyGug,
        query: &str,
        options: GugOptions,
    ) -> Result<Vec<String>, GugError> {
        match gug {
            AnyGug::Single(g) => Ok(vec![g.gallery_url(query, options)?]),
            AnyGug::Nested(n) => n
                .gugs
                .iter()
                .filter_map(|(key, name)| match self.get(key, name) {
                    Some(AnyGug::Single(g)) => Some(g.gallery_url(query, options)),
                    // the reference would recurse into a nested one and fail
                    // on it; nesting is not something its editor allows
                    Some(AnyGug::Nested(_)) | None => None,
                })
                .collect(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn gug(template: &str) -> Gug {
        Gug {
            name: "test".into(),
            key: "00".into(),
            url_template: template.into(),
            replacement_phrase: "%tags%".into(),
            separator: "+".into(),
            initial_search_text: String::new(),
            example_search_text: "blue_eyes".into(),
        }
    }

    #[test]
    fn terms_are_encoded_for_where_they_go() {
        let o = GugOptions::default();
        assert_eq!(
            gug("https://example.com/search?q=%tags%&i=0").gallery_url("6+girls skirt", o),
            Ok("https://example.com/search?q=6%2Bgirls+skirt&i=0".into())
        );
        assert_eq!(
            gug("https://example.com/artist/%tags%/works").gallery_url("a+b c", o),
            Ok("https://example.com/artist/a+b+c/works".into())
        );
        assert_eq!(
            gug("https://example.com/").gallery_url("x", o),
            Err(GugError::PhraseNotInTemplate)
        );
    }
}

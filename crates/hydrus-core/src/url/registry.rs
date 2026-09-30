//! A client's URL classes, and what they say about any URL: which class it
//! is, how to store it, what to fetch for it and whether it can be parsed.

use std::collections::HashSet;

use serde::{Deserialize, Serialize};

use super::class::{DomainMask, UrlClass, UrlClassError, UrlType};
use super::functions::{check_full_url, ensure_url_is_encoded, search_urls};

/// A client's URL class configuration.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct UrlClassSettings {
    /// In the order the user has them (the order matching tries them in,
    /// among equally specific classes).
    pub url_classes: Vec<UrlClass>,
    /// Which parser each URL class is linked to, by keys (hex).
    pub parser_links: Vec<(String, Option<String>)>,
    /// Keys (hex) of the parsers that exist.
    pub parser_keys: Vec<String>,
    /// Treat every leading `/` of a URL path as one (an old reference
    /// behaviour some users keep on).
    pub collapse_leading_slashes: bool,
}

impl UrlClassSettings {
    /// The URL class a search names (`GetURLClassFromName`): the first whose
    /// name matches, case-folded.
    pub fn class_by_name(&self, name: &str) -> Option<&UrlClass> {
        let wanted = crate::casefold::casefold(name);
        self.url_classes
            .iter()
            .find(|class| crate::casefold::casefold(&class.name) == wanted)
    }
}

/// What a client can do with a URL, as `/add_urls/get_url_info` reports.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseCapability {
    pub url_type: UrlType,
    /// The URL class's name, or `unknown url`.
    pub match_name: String,
    /// `Ok` if a parser can read it, else why not.
    pub parser: Result<(), String>,
}

/// URL classes ready for matching.
#[derive(Debug, Clone, Default)]
pub struct UrlClasses {
    settings: UrlClassSettings,
    /// Classes grouped by domain mask; groups and classes within a group
    /// most specific first.
    groups: Vec<(DomainMask, Vec<usize>)>,
    parsers: HashSet<String>,
}

impl UrlClasses {
    pub fn new(settings: UrlClassSettings) -> Self {
        let collapse = settings.collapse_leading_slashes;
        let mut groups: Vec<(DomainMask, Vec<usize>)> = Vec::new();
        for (i, class) in settings.url_classes.iter().enumerate() {
            match groups
                .iter_mut()
                .find(|(mask, _)| mask.grouping_key() == class.domain_mask.grouping_key())
            {
                Some((_, members)) => members.push(i),
                None => groups.push((class.domain_mask.clone(), vec![i])),
            }
        }
        for (_, members) in &mut groups {
            let keys: Vec<[usize; 6]> = members
                .iter()
                .map(|&i| settings.url_classes[i].sorting_key(collapse))
                .collect();
            let mut order: Vec<usize> = (0..members.len()).collect();
            // stable, so equally specific classes keep the user's order
            order.sort_by(|&a, &b| keys[b].cmp(&keys[a]));
            *members = order.into_iter().map(|j| members[j]).collect();
        }
        let parsers = settings.parser_keys.iter().cloned().collect();
        Self {
            settings,
            groups,
            parsers,
        }
    }

    pub fn settings(&self) -> &UrlClassSettings {
        &self.settings
    }

    fn collapse(&self) -> bool {
        self.settings.collapse_leading_slashes
    }

    /// The URL class a URL belongs to, if any.
    pub fn class_for(&self, url: &str) -> Option<&UrlClass> {
        let domain = check_full_url(url).ok()?.netloc;
        let mut masks: Vec<&(DomainMask, Vec<usize>)> = self
            .groups
            .iter()
            .filter(|(mask, _)| mask.matches(&domain))
            .collect();
        masks.sort_by_key(|(mask, _)| std::cmp::Reverse(mask.sorting_complexity()));
        masks
            .into_iter()
            .flat_map(|(_, members)| members)
            .map(|&i| &self.settings.url_classes[i])
            .find(|class| class.matches(url, self.collapse()))
    }

    /// The URL to store for `url` (`for_server`: the URL to request). Texts
    /// that aren't full URLs come back as they are; URLs of no class are
    /// just encoded and lose their fragment.
    pub fn normalise(&self, url: &str, for_server: bool) -> Result<String, UrlClassError> {
        if check_full_url(url).is_err() {
            return Ok(url.to_owned());
        }
        match self.class_for(url) {
            None => Ok(ensure_url_is_encoded(url, false, self.collapse())),
            Some(class) => class.normalise(url, for_server, self.collapse()),
        }
    }

    /// Follow a URL class's API/redirect links to the class and URL that are
    /// actually fetched.
    fn api_class_and_url(&self, url: &str) -> Result<(&UrlClass, String), UrlClassError> {
        self.api_chain(url)
            .map(|(chain, api_url)| (*chain.last().expect("never empty"), api_url))
    }

    /// A URL's class and the classes of each API URL it leads to, and the
    /// last API URL (normalised for requests).
    fn api_chain(&self, url: &str) -> Result<(Vec<&UrlClass>, String), UrlClassError> {
        let Some(mut class) = self.class_for(url) else {
            return Err(UrlClassError(format!(
                "Could not find a URL Class for {url}!"
            )));
        };
        let mut seen: Vec<&[u8]> = vec![&class.key];
        let mut api_url = url.to_owned();
        while class.uses_api_url() {
            api_url = class.api_url(&api_url, self.collapse())?;
            let Some(next) = self.class_for(&api_url) else {
                return Err(UrlClassError(format!(
                    "Could not find an API/Redirect URL Class for {api_url} URL, which originally came from {url}!"
                )));
            };
            if seen.contains(&next.key.as_slice()) {
                return Err(UrlClassError(match seen.len() {
                    1 => format!(
                        "Could not find an API/Redirect URL Class for {url} as the url class API-linked to itself!"
                    ),
                    2 => format!(
                        "Could not find an API/Redirect URL Class for {url} as the url class and its API url class API-linked to each other!"
                    ),
                    n => format!(
                        "Could not find an API/Redirect URL Class for {url} as it and its API url classes linked in a loop of size {n}!"
                    ),
                }));
            }
            seen.push(&next.key);
            class = next;
        }
        let api_url = class.normalise(&api_url, true, self.collapse())?;
        let chain = seen
            .iter()
            .filter_map(|key| {
                self.settings
                    .url_classes
                    .iter()
                    .find(|c| c.key.as_slice() == *key)
            })
            .collect();
        Ok((chain, api_url))
    }

    /// The keys (hex) of a URL's class and its API URL classes, in order
    /// (`GetAPIPertinentURLClassKeysInPreferenceOrder`); none for a URL
    /// without a class.
    pub fn api_class_keys(&self, url: &str) -> Vec<(String, UrlType)> {
        self.api_chain(url)
            .map(|(chain, _)| {
                chain
                    .into_iter()
                    .map(|c| (hex::encode(&c.key), c.url_type))
                    .collect()
            })
            .unwrap_or_default()
    }

    /// The URL to request for a post URL and the key (hex) of the parser
    /// that reads what comes back.
    pub fn url_to_fetch_and_parser(&self, url: &str) -> Result<(String, String), UrlClassError> {
        let (class, api_url) = self
            .api_class_and_url(url)
            .map_err(|e| UrlClassError(format!("Could not find a URL class for {url}!\n\n{e}")))?;
        let key = hex::encode(&class.key);
        self.settings
            .parser_links
            .iter()
            .find(|(class_key, _)| *class_key == key)
            .and_then(|(_, parser)| parser.clone())
            .filter(|parser| self.parsers.contains(parser))
            .map(|parser| (api_url, parser))
            .ok_or_else(|| {
                UrlClassError(format!(
                    "Could not find a parser for {} URL Class!",
                    class.name
                ))
            })
    }

    /// Whether a URL's class says it can lead to several files (a gallery,
    /// a watchable page, a multi-file post).
    pub fn can_refer_to_multiple_files(&self, url: &str) -> bool {
        self.class_for(url).is_some_and(|c| {
            matches!(c.url_type, UrlType::Gallery | UrlType::Watchable)
                || (c.url_type == UrlType::Post && c.can_produce_multiple_files)
        })
    }

    /// Whether a URL's class says it is exactly one file (a file URL or a
    /// single-file post).
    pub fn refers_to_one_file(&self, url: &str) -> bool {
        self.class_for(url).is_some_and(|c| {
            c.url_type == UrlType::File
                || (c.url_type == UrlType::Post && !c.can_produce_multiple_files)
        })
    }

    /// The URL to request for a (normalised) URL: itself, or where its URL
    /// class redirects.
    pub fn url_to_fetch(&self, url: &str) -> Result<String, UrlClassError> {
        if self.class_for(url).is_none() {
            return Ok(url.to_owned());
        }
        self.api_class_and_url(url)
            .map(|(_, api_url)| api_url)
            .map_err(|e| UrlClassError(format!("Could not find a URL class for {url}!\n\n{e}")))
    }

    /// The referral URL to send when requesting `url`, given the page it
    /// was found on (if known).
    pub fn referral_url(&self, url: &str, given: Option<&str>) -> Option<String> {
        match self.class_for(url) {
            None => given.map(str::to_owned),
            Some(class) => class.referral_url(url, given, self.collapse()),
        }
    }

    /// The next page of a gallery URL, if its URL class knows how to make
    /// one.
    pub fn next_gallery_page(&self, url: &str) -> Option<Result<String, UrlClassError>> {
        let class = self.class_for(url)?;
        class
            .can_generate_next_gallery_page()
            .then(|| class.next_gallery_page(url, self.collapse()))
    }

    /// What kind of URL this is and whether a parser can read it.
    pub fn parse_capability(&self, url: &str) -> ParseCapability {
        let Some(class) = self.class_for(url) else {
            return ParseCapability {
                url_type: UrlType::Unknown,
                match_name: "unknown url".into(),
                parser: Err("unknown url class".into()),
            };
        };
        let parser = match self.api_class_and_url(url) {
            Err(e) => Err(format!("Could not find a URL class for {url}!\n\n{e}")),
            Ok((api_class, _)) => {
                let key = hex::encode(&api_class.key);
                let linked = self
                    .settings
                    .parser_links
                    .iter()
                    .find(|(class_key, _)| *class_key == key)
                    .and_then(|(_, parser)| parser.as_ref());
                match linked {
                    Some(parser) if self.parsers.contains(parser) => Ok(()),
                    _ => Err(format!(
                        "Could not find a parser for {} URL Class!",
                        api_class.name
                    )),
                }
            }
        };
        ParseCapability {
            url_type: class.url_type,
            match_name: class.name.clone(),
            parser,
        }
    }

    /// Whether files found at this URL should remember it.
    pub fn should_associate_with_files(&self, url: &str) -> bool {
        self.class_for(url)
            .is_none_or(|class| class.should_be_associated_with_files)
    }

    /// Every URL a (normalised) URL may have been stored under.
    pub fn search_urls(&self, url: &str) -> Vec<String> {
        let mut normalised = Vec::new();
        if let Ok(for_server) = self.normalise(url, true) {
            normalised.push(for_server);
            if let Ok(stored) = self.normalise(url, false) {
                normalised.push(stored);
            }
        }
        search_urls(url, &normalised)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::url::class::tests::gelbooru_post;

    fn registry() -> UrlClasses {
        let class = gelbooru_post();
        UrlClasses::new(UrlClassSettings {
            parser_links: vec![(hex::encode(&class.key), Some("ab".into()))],
            parser_keys: vec!["ab".into()],
            url_classes: vec![class],
            collapse_leading_slashes: false,
        })
    }

    #[test]
    fn unknown_urls_are_just_encoded() {
        let r = registry();
        assert_eq!(r.normalise("not a url", false).unwrap(), "not a url");
        assert_eq!(
            r.normalise("https://example.com/a b#frag", false).unwrap(),
            "https://example.com/a%20b"
        );
        let c = r.parse_capability("https://example.com/x");
        assert_eq!(c.url_type, UrlType::Unknown);
        assert_eq!(c.parser, Err("unknown url class".into()));
    }

    #[test]
    fn known_urls_are_classified() {
        let r = registry();
        let url = "https://gelbooru.com/index.php?id=5&page=post&s=view";
        let c = r.parse_capability(url);
        assert_eq!(c.match_name, "gelbooru file page");
        assert_eq!(c.parser, Ok(()));
        assert_eq!(r.url_to_fetch(url).unwrap(), url);
        let search = r.search_urls(url);
        assert!(
            search.contains(&"http://www.gelbooru.com/index.php?id=5&page=post&s=view".to_owned())
        );
        assert!(search.contains(&format!("{url}/")));
    }
}

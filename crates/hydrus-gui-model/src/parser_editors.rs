//! Native parser drafts, typed test context and URL-class associations. The
//! same parser engine drives previews and live downloads; only Apply persists.
use crate::favourites::non_dupe_name;
use crate::formula_editors::{FormulaTestData, new_formula};
use hydrus_core::pages::PageKey;
use hydrus_core::url::strings::{StringConverter, StringMatch};
use hydrus_core::url::{UrlClassSettings, UrlType};
use hydrus_parse::content::{ContentKind, ContentParser, PageParser, ParseFailure, ParsedPost};
use hydrus_parse::downloaders::Downloaders;
use hydrus_parse::formula::ParsingContext;
use hydrus_store::{Store, StoreError, settings};

/// Reference content choices, including existing temporary variables.
pub const CONTENT_TYPES: [&str; 9] = [
    "urls",
    "tags",
    "notes",
    "file hash",
    "timestamp",
    "watcher title",
    "http headers",
    "temporary variable",
    "veto",
];
/// Reference parser list headings.
pub const PARSER_COLUMNS: [&str; 3] = ["name", "example urls", "produces"];
/// Reference URL class link headings.
pub const LINK_COLUMNS: [&str; 3] = ["url class", "type", "parser"];
/// Question before discarding an edited content parser.
pub const CONTENT_CANCEL: &str =
    "It looks like you have made changes to the content parser--are you sure you want to cancel?";
/// Test context owns the URL and post counter separately from named variables.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TestContext {
    pub url: String,
    pub post_index: u64,
    pub variables: ParsingContext,
}
impl TestContext {
    /// Read newline-separated key=value variables without silently dropping errors.
    pub fn parse(url: String, post_index: &str, variables: &str) -> Result<Self, String> {
        let post_index = post_index
            .parse()
            .map_err(|_| "Post index must be a non-negative integer.".to_owned())?;
        let mut values = ParsingContext::new();
        for line in variables.lines().filter(|line| !line.trim().is_empty()) {
            let (key, value) = line
                .split_once('=')
                .ok_or_else(|| "Context variables must use key=value.".to_owned())?;
            if key.is_empty()
                || matches!(key, "url" | "post_index")
                || values.insert(key.to_owned(), value.to_owned()).is_some()
            {
                return Err(
                    "Context variable names must be unique and cannot be url or post_index.".into(),
                );
            }
        }
        Ok(Self {
            url,
            post_index,
            variables: values,
        })
    }
    /// Supply the runtime parser's context vocabulary.
    pub fn values(&self) -> ParsingContext {
        let mut values = self.variables.clone();
        values.insert("url".into(), self.url.clone());
        values.insert("post_index".into(), self.post_index.to_string());
        values
    }
}
/// Isolated content edit, preserving the original formula unless explicitly edited.
#[derive(Debug, Clone)]
pub struct ContentEditor {
    pub parser: ContentParser,
    pub test: FormulaTestData,
    original: ContentParser,
}
impl ContentEditor {
    /// Open an isolated copy with the caller's actual test data.
    pub fn new(parser: &ContentParser, test: FormulaTestData) -> Self {
        Self {
            parser: parser.clone(),
            original: parser.clone(),
            test,
        }
    }
    /// The current content choice in the reference order.
    pub fn kind_index(&self) -> usize {
        kind_index(&self.parser.kind)
    }
    /// Select a new content type, keeping the formula and its processing intact.
    pub fn change_kind(&mut self, index: usize) {
        if index == self.kind_index() {
            return;
        }
        self.parser.kind = default_kind(index);
        self.test.collapse_newlines = index != 2;
    }
    /// Apply reference normalization for an empty note name.
    pub fn value(&self) -> ContentParser {
        let mut parser = self.parser.clone();
        if let ContentKind::Note { name } = &mut parser.kind
            && name.is_empty()
        {
            *name = "note".into();
        }
        parser
    }
    /// Whether cancelling should ask about discarded changes.
    pub fn changed(&self) -> bool {
        self.value() != self.original
    }
    /// Execute the real content parser, including URL cleaning, notes and veto.
    pub fn preview(&self) -> Result<ParsedPost, ParseFailure> {
        self.value().parse(&self.test.context, &self.test.text)
    }
}
/// A content type's display index.
pub fn kind_index(kind: &ContentKind) -> usize {
    match kind {
        ContentKind::Url { .. } => 0,
        ContentKind::Tag { .. } => 1,
        ContentKind::Note { .. } => 2,
        ContentKind::Hash { .. } => 3,
        ContentKind::Timestamp { .. } => 4,
        ContentKind::Title { .. } => 5,
        ContentKind::HttpHeader { .. } => 6,
        ContentKind::Variable { .. } => 7,
        ContentKind::Veto { .. } => 8,
    }
}
/// Reference defaults for a newly selected content type.
pub fn default_kind(index: usize) -> ContentKind {
    match index {
        1 => ContentKind::Tag {
            namespace: Some(String::new()),
        },
        2 => ContentKind::Note {
            name: "note".into(),
        },
        3 => ContentKind::Hash {
            hash_type: "md5".into(),
            encoding: "hex".into(),
        },
        4 => ContentKind::Timestamp {
            timestamp_type: Some(0),
        },
        5 => ContentKind::Title { priority: 50 },
        6 => ContentKind::HttpHeader {
            name: String::new(),
        },
        7 => ContentKind::Variable {
            name: String::new(),
        },
        8 => ContentKind::Veto {
            if_matches_found: true,
            string_match: StringMatch::any(),
        },
        _ => ContentKind::Url {
            url_type: 7,
            priority: 50,
        },
    }
}
/// A new HTML content node (the reusable editor can change it to JSON).
pub fn new_content() -> ContentParser {
    ContentParser {
        name: "new content parser".into(),
        kind: default_kind(0),
        formula: new_formula(false),
    }
}
/// New named page parser with an independent stable runtime key.
pub fn new_page() -> PageParser {
    PageParser {
        name: "new page parser".into(),
        key: PageKey::random().to_hex(),
        converter: StringConverter::default(),
        subsidiary: Vec::new(),
        content_parsers: Vec::new(),
        example_urls: Vec::new(),
    }
}
/// Staged parser list and class links loaded together from the native store.
#[derive(Debug, Clone)]
pub struct Draft {
    pub parsers: Vec<PageParser>,
    pub classes: UrlClassSettings,
    original_parsers: Vec<PageParser>,
    original_links: Vec<(String, Option<String>)>,
}
impl Draft {
    /// Open real definitions without writing or losing generator settings.
    pub fn load(store: &Store) -> hydrus_store::Result<Self> {
        store.read(|conn| {
            let definitions: Downloaders = settings::get(conn)?;
            let classes: UrlClassSettings = settings::get(conn)?;
            Ok(Self::new(definitions.parsers, classes))
        })
    }
    /// Construct a draft for fixture replay.
    pub fn new(parsers: Vec<PageParser>, classes: UrlClassSettings) -> Self {
        Self {
            original_parsers: parsers.clone(),
            original_links: classes.parser_links.clone(),
            parsers,
            classes,
        }
    }
    /// Replace a parser by stable key, never a potentially stale row index.
    pub fn put(&mut self, replacing: Option<&str>, mut parser: PageParser) -> Result<(), String> {
        let index = replacing
            .map(|key| {
                self.parsers
                    .iter()
                    .position(|p| p.key == key)
                    .ok_or_else(|| "The parser being edited no longer exists.".to_owned())
            })
            .transpose()?;
        let names = self
            .parsers
            .iter()
            .enumerate()
            .filter(|(i, _)| Some(*i) != index)
            .map(|(_, p)| p.name.clone())
            .collect::<Vec<_>>();
        parser.name = non_dupe_name(&parser.name, &|name| names.iter().any(|old| old == name));
        if let Some(index) = index {
            parser.key.clone_from(&self.parsers[index].key);
            self.parsers[index] = parser;
        } else {
            parser.key = PageKey::random().to_hex();
            self.parsers.push(parser);
        }
        Ok(())
    }
    /// Remove parsers and their links, keeping other class settings intact.
    pub fn remove(&mut self, keys: &[String]) {
        self.parsers.retain(|p| !keys.contains(&p.key));
        for (_, parser) in &mut self.classes.parser_links {
            if parser.as_ref().is_some_and(|key| keys.contains(key)) {
                *parser = None;
            }
        }
    }
    /// Classes which can directly own a parser; redirect sources use their target.
    pub fn linkable(&self) -> Vec<usize> {
        self.classes
            .url_classes
            .iter()
            .enumerate()
            .filter(|(_, c)| {
                matches!(
                    c.url_type,
                    UrlType::Post | UrlType::Gallery | UrlType::Watchable
                ) && !c.uses_api_url()
            })
            .map(|(i, _)| i)
            .collect()
    }
    /// Stage an association using keys, validating both endpoints.
    pub fn link(&mut self, class_key: &str, parser_key: Option<&str>) -> Result<(), String> {
        if !self
            .linkable()
            .iter()
            .any(|&i| hex::encode(&self.classes.url_classes[i].key) == class_key)
        {
            return Err("This URL class cannot own a parser.".into());
        }
        if parser_key.is_some_and(|key| !self.parsers.iter().any(|p| p.key == key)) {
            return Err("The selected parser no longer exists.".into());
        }
        let value = parser_key.map(str::to_owned);
        if let Some((_, old)) = self
            .classes
            .parser_links
            .iter_mut()
            .find(|(key, _)| key == class_key)
        {
            *old = value;
        } else {
            self.classes
                .parser_links
                .push((class_key.to_owned(), value));
        }
        Ok(())
    }
    /// Persist both settings in one transaction, rejecting concurrent parser/link
    /// edits and preserving unrelated settings, classes, generators and legacy data.
    pub fn save(&self, store: &Store) -> hydrus_store::Result<()> {
        let draft = self.clone();
        store.write_and_refresh(move |ctx| {
            let conn = ctx.conn();
            let mut current: Downloaders = settings::get(conn)?;
            let mut classes: UrlClassSettings = settings::get(conn)?;
            let parsers_changed = draft.parsers != draft.original_parsers;
            let links_changed = draft.classes.parser_links != draft.original_links;
            if (parsers_changed && current.parsers != draft.original_parsers) || (links_changed && classes.parser_links != draft.original_links) {
                return Err(StoreError::Invalid("Parser definitions or links changed in another editor. Reopen this dialog before applying.".into()));
            }
            if parsers_changed { current.parsers = draft.parsers; settings::set(conn, &current)?; }
            if links_changed { classes.parser_links = draft.classes.parser_links; }
            classes.parser_keys = current.parsers.iter().map(|p| p.key.clone()).collect();
            // A class removed while this editor was open must never be resurrected.
            classes.parser_links.retain(|(key, _)| classes.url_classes.iter().any(|c| hex::encode(&c.key) == *key));
            settings::set(conn, &classes)
        })
    }
}
/// Format real parsed contents without hiding content kinds or empty results.
pub fn preview_text(posts: &[ParsedPost]) -> String {
    if posts.is_empty() {
        return "No parsed results.".into();
    }
    posts
        .iter()
        .enumerate()
        .map(|(i, post)| {
            format!(
                "Post {}\n{}",
                i + 1,
                post.contents
                    .iter()
                    .map(|c| format!("{}: {}", CONTENT_TYPES[kind_index(&c.kind)], c.text))
                    .collect::<Vec<_>>()
                    .join("\n")
            )
        })
        .collect::<Vec<_>>()
        .join("\n\n")
}

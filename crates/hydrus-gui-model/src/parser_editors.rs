//! Native parser drafts, typed test context and URL-class associations. The
//! same parser engine drives previews and live downloads; only Apply persists.
use crate::favourites::non_dupe_name;
use crate::formula_editors::{FormulaTestData, new_formula};
use hydrus_core::pages::PageKey;
use hydrus_core::url::strings::{StringConverter, StringMatch};
use hydrus_core::url::{UrlClass, UrlClassSettings, UrlType};
use hydrus_parse::content::{
    ContentKind, ContentParser, PageParser, ParseFailure, ParsedPost, SubsidiaryPageParser,
};
use hydrus_parse::downloaders::Downloaders;
use hydrus_parse::formula::{
    Formula, FormulaKind, HtmlContent, HtmlRule, HtmlWalk, ParsingContext, TagSearch,
};
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
    inactive_namespace: String,
}
impl ContentEditor {
    /// Open an isolated copy with the caller's actual test data.
    pub fn new(parser: &ContentParser, test: FormulaTestData) -> Self {
        Self {
            parser: parser.clone(),
            original: parser.clone(),
            inactive_namespace: match &parser.kind {
                ContentKind::Tag { namespace } => namespace.clone().unwrap_or_default(),
                _ => String::new(),
            },
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
    /// Keep a disabled namespace intact while selecting any namespace.
    pub fn set_any_namespace(&mut self, any: bool) {
        if let ContentKind::Tag { namespace } = &mut self.parser.kind {
            if any {
                if let Some(value) = namespace.take() {
                    self.inactive_namespace = value;
                }
            } else if namespace.is_none() {
                *namespace = Some(self.inactive_namespace.clone());
            }
        }
    }
    /// Namespace shown by its field even while the control is disabled.
    pub fn namespace_text(&self) -> &str {
        match &self.parser.kind {
            ContentKind::Tag {
                namespace: Some(value),
            } => value,
            _ => &self.inactive_namespace,
        }
    }
    /// Apply reference normalization for an empty note name and the only
    /// supported timestamp choice, including imported unset/obsolete types.
    pub fn value(&self) -> ContentParser {
        let mut parser = self.parser.clone();
        if let ContentKind::Note { name } = &mut parser.kind
            && name.is_empty()
        {
            *name = "note".into();
        }
        if let ContentKind::Timestamp { timestamp_type } = &mut parser.kind {
            *timestamp_type = Some(hydrus_parse::content::TIMESTAMP_MODIFIED_DOMAIN);
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
        reference_auxiliary: None,
        name: "new page parser".into(),
        key: PageKey::random().to_hex(),
        converter: StringConverter::default(),
        subsidiary: Vec::new(),
        content_parsers: Vec::new(),
        example_urls: Vec::new(),
    }
}
/// Reference defaults for a newly added recursive subsidiary page parser.
pub fn new_subsidiary() -> SubsidiaryPageParser {
    let mut parser = new_page();
    parser.name = "new sub page parser".into();
    let mut formula = new_formula(false);
    formula.kind = FormulaKind::Html {
        rules: vec![HtmlRule {
            walk: HtmlWalk::Descendants(TagSearch {
                attrs: vec![("class".into(), "thumb".into())],
                ..TagSearch::default()
            }),
            tag_name: Some("div".into()),
            text_match: None,
        }],
        content: HtmlContent::Html,
    };
    SubsidiaryPageParser {
        formula,
        sort_by_source_time: false,
        parser,
    }
}
/// Separation controls accompany the ordinary reusable child page draft.
#[derive(Debug, Clone)]
pub struct SubsidiaryEditor {
    pub formula: Formula,
    pub sort_by_source_time: bool,
}
impl SubsidiaryEditor {
    /// Copy the child separation and source-time controls into an isolated draft.
    #[must_use]
    pub fn new(parser: &SubsidiaryPageParser) -> Self {
        Self {
            formula: parser.formula.clone(),
            sort_by_source_time: parser.sort_by_source_time,
        }
    }
    /// Preserve the edited page's key, recursive definitions and auxiliary data.
    pub fn value(&self, parser: PageParser) -> SubsidiaryPageParser {
        SubsidiaryPageParser {
            formula: self.formula.clone(),
            sort_by_source_time: self.sort_by_source_time,
            parser,
        }
    }
    /// Reference child previews convert the raw document, then separate posts.
    pub fn child_test_data(
        &self,
        parser: &PageParser,
        test: &FormulaTestData,
    ) -> Result<FormulaTestData, String> {
        let mut input = test.clone();
        input.prepare_examples();
        let mut child = input.clone();
        child.examples.clear();
        child.source_urls.clear();
        for (i, text) in input.examples.iter().enumerate() {
            let converted = parser.converter.convert(text).map_err(|e| e.to_string())?;
            let texts = self
                .formula
                .parse(&test.context, &converted, false)
                .map_err(|e| e.to_string())?;
            let url = test
                .source_urls
                .get(i)
                .cloned()
                .unwrap_or_else(|| test.context.get("url").cloned());
            for text in texts {
                child.examples.push(text);
                child.source_urls.push(url.clone());
            }
        }
        if child.examples.is_empty() {
            child.examples.push(String::new());
            child.source_urls.push(test.context.get("url").cloned());
        }
        child.prepare_examples();
        Ok(child)
    }
    /// Run the complete subsidiary with the same parser engine used by downloads.
    pub fn preview(
        &self,
        parser: &PageParser,
        context: &mut ParsingContext,
        text: &str,
    ) -> Result<Vec<ParsedPost>, ParseFailure> {
        let mut parent = new_page();
        parent.subsidiary.push(self.value(parser.clone()));
        parent.parse(context, text)
    }
}
/// Staged parser list and class links loaded together from the native store.
#[derive(Debug, Clone)]
pub struct Draft {
    pub auxiliary: crate::downloader_interchange::Auxiliary,
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
            let mut draft = Self::new(definitions.parsers, classes);
            draft.auxiliary = settings::get(conn)?;
            Ok(draft)
        })
    }
    /// Construct a draft for fixture replay.
    pub fn new(parsers: Vec<PageParser>, classes: UrlClassSettings) -> Self {
        Self {
            auxiliary: crate::downloader_interchange::Auxiliary::default(),
            original_parsers: parsers.clone(),
            original_links: classes.parser_links.clone(),
            parsers,
            classes,
        }
    }
    /// Import a list of reference page parsers without partial draft changes.
    pub fn import(
        &mut self,
        definitions: Vec<crate::downloader_interchange::Definition>,
    ) -> Result<crate::downloader_interchange::Review, String> {
        let mut next = self.clone();
        let mut review = crate::downloader_interchange::Review::default();
        for mut definition in definitions {
            let crate::downloader_interchange::Native::Page(parser) = &mut definition.native else {
                return Err("Import page parsers here; use the downloader package importer for mixed bundles.".into());
            };
            next.put(None, parser.clone())?;
            *parser = next.parsers.last().ok_or("No parser imported.")?.clone();
            review.added.push(parser.name.clone());
            next.auxiliary.retain(&definition);
        }
        *self = next;
        Ok(review)
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
            .filter(|(_, c)| can_link(c))
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
            draft.auxiliary.save(conn)?;
            let mut current: Downloaders = settings::get(conn)?;
            let mut classes: UrlClassSettings = settings::get(conn)?;
            let parsers_changed = draft.parsers != draft.original_parsers;
            let links_changed = draft.classes.parser_links != draft.original_links;
            if (parsers_changed && current.parsers != draft.original_parsers) || (links_changed && classes.parser_links != draft.original_links) {
                return Err(StoreError::Invalid("Parser definitions or links changed in another editor. Reopen this dialog before applying.".into()));
            }
            if parsers_changed { current.parsers = draft.parsers; settings::set(conn, &current)?; }
            if links_changed {
                if draft.classes.parser_links.iter().any(|(_, key)| {
                    key.as_ref().is_some_and(|key| !current.parsers.iter().any(|p| &p.key == key))
                }) {
                    return Err(StoreError::Invalid("A linked parser no longer exists. Reopen this dialog before applying.".into()));
                }
                for (class_key, parser_key) in &draft.classes.parser_links {
                    let original = draft.original_links.iter().find(|(key, _)|key == class_key).and_then(|(_,key)|key.as_ref());
                    if parser_key.is_some() && parser_key.as_ref() != original
                        && !classes.url_classes.iter().any(|class| hex::encode(&class.key) == *class_key && can_link(class)) {
                        return Err(StoreError::Invalid("A URL class changed and can no longer own this parser link. Reopen this dialog before applying.".into()));
                    }
                }
                classes.parser_links = draft.classes.parser_links;
            }
            if parsers_changed {
                for (_, key) in &mut classes.parser_links {
                    if key.as_ref().is_some_and(|key| !current.parsers.iter().any(|p| &p.key == key)) {
                        *key = None;
                    }
                }
            }
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

fn can_link(class: &UrlClass) -> bool {
    matches!(
        class.url_type,
        UrlType::Post | UrlType::Gallery | UrlType::Watchable
    ) && !class.uses_api_url()
}

/// Append a subsidiary package, sort the queue and return exactly the added rows.
/// Duplicate keys remain intact, as reference subsidiary clipboard imports do.
pub fn append_subsidiaries(
    page: &mut PageParser,
    imported: Vec<SubsidiaryPageParser>,
) -> Vec<usize> {
    let mut rows = std::mem::take(&mut page.subsidiary)
        .into_iter()
        .map(|parser| (false, parser))
        .collect::<Vec<_>>();
    rows.extend(imported.into_iter().map(|parser| (true, parser)));
    rows.sort_by_cached_key(|(_, parser)| hydrus_core::casefold::casefold(&parser.parser.name));
    let added = rows
        .iter()
        .enumerate()
        .filter_map(|(i, (added, _))| added.then_some(i))
        .collect();
    page.subsidiary = rows.into_iter().map(|(_, parser)| parser).collect();
    added
}

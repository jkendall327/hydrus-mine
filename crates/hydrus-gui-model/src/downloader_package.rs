//! The reference's downloader sharing panels as decisions: the export list
//! (`DownloaderExportPanel`), filled by its "add gug", "add url class", "add
//! parser", "add login script" and "add headers/bandwidth rules" buttons and
//! summarised before it becomes a PNG; and the import (`ReviewDownloaderImport.
//! _ImportPayloads`): what is new, the "select objects to add" choice, the
//! domain metadata shown one at a time, the final question and the notice.
//! The window (`hydrus-gui`'s `downloader_interchange_window.rs`) asks the
//! questions; this says what they are and what each answer does.

use std::collections::BTreeSet;

use hydrus_core::url::{AnyGug, GugOptions};

use crate::downloader_interchange::{Definition, DomainMetadata, Draft, Native, Review};

/// The reference shows at most this many objects in a summary.
const SUMMARY_LIMIT: usize = 20;
/// And at most this many domain metadata packages one at a time.
const TOO_MANY_DM: usize = 8;

/// One of the export list's "add" buttons that opens a chooser.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Add {
    Gug,
    UrlClass,
    Parser,
    LoginScript,
}

impl Add {
    /// The chooser's title.
    pub fn title(self) -> &'static str {
        match self {
            Self::Gug => "select gugs",
            Self::UrlClass => "select url classes to include",
            Self::Parser => "select parsers to include",
            Self::LoginScript => "select login scripts",
        }
    }
}

/// A row of a chooser (`SelectMultipleFromList`): its label, the object,
/// and whether it starts checked.
#[derive(Debug, Clone, PartialEq)]
pub struct Choice {
    pub label: String,
    pub definition: Definition,
    pub checked: bool,
}

/// The chooser's rows, in the order it shows them (`HumanTextSortKey`).
fn sorted(mut choices: Vec<Choice>) -> Vec<Choice> {
    choices.sort_by_cached_key(|c| hydrus_core::sort::human_sort_key(&c.label));
    choices
}

/// The domain of a URL, as `ConvertURLIntoDomain` gives it.
fn domain(url: &str) -> String {
    hydrus_core::url::url_domain(url).unwrap_or_default()
}

/// `GetSafeSummary`: what an object is, and the domains it is for.
pub fn safe_summary(definition: &Definition) -> String {
    match &definition.native {
        Native::Gug(AnyGug::Single(gug)) => format!(
            "Downloader \"{}\" - {}",
            gug.name,
            gug.example_url(GugOptions::default())
                .map(|url| domain(&url))
                .unwrap_or_default()
        ),
        Native::Gug(AnyGug::Nested(nested)) => format!(
            "Nested downloader \"{}\" - {}",
            nested.name,
            nested
                .gugs
                .iter()
                .map(|(_, name)| name.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        ),
        Native::Class(class) => {
            format!(
                "URL Class \"{}\" - {}",
                class.name,
                domain(&class.example_url)
            )
        }
        Native::Page(parser) => {
            let domains: BTreeSet<String> =
                parser.example_urls.iter().map(|url| domain(url)).collect();
            format!(
                "Parser \"{}\" - {}",
                parser.name,
                domains.into_iter().collect::<Vec<_>>().join(", ")
            )
        }
        Native::Login(script) => format!(
            "Login Script \"{}\" - {}",
            script.name,
            script
                .examples
                .iter()
                .map(|e| e.domain.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        ),
        Native::Domain(metadata) => metadata.safe_summary(),
        _ => definition.name().to_owned(),
    }
}

/// The export list's type column (`SERIALISABLE_NAME`).
pub fn type_name(definition: &Definition) -> &'static str {
    match &definition.native {
        Native::Gug(AnyGug::Single(_)) => "Gallery URL Generator",
        Native::Gug(AnyGug::Nested(_)) => "Nested Gallery URL Generator",
        Native::Class(_) => "URL Class",
        Native::Page(_) => "Page Parser",
        Native::Login(_) => "Login Script - Domain",
        Native::Domain(_) => "Domain Metadata",
        _ => "",
    }
}

/// The export list's name column: a domain metadata package's domain, else
/// the object's name.
pub fn row_name(definition: &Definition) -> String {
    match &definition.native {
        Native::Domain(metadata) => metadata.domain.clone(),
        _ => definition.name().to_owned(),
    }
}

/// A summary of `objects` as the reference's questions give it: the first
/// twenty, and how many others.
fn summary(objects: &[String]) -> String {
    let mut text = objects
        .iter()
        .take(SUMMARY_LIMIT)
        .cloned()
        .collect::<Vec<_>>()
        .join("\n");
    if objects.len() > SUMMARY_LIMIT {
        text.push_str(&format!(
            "\n(and {} others)",
            hydrus_core::numbers::human_int(
                u64::try_from(objects.len() - SUMMARY_LIMIT).unwrap_or(u64::MAX)
            )
        ));
    }
    text
}

/// The export panel's list: the objects to share, in the order they were
/// added (each once).
#[derive(Debug, Clone, Default)]
pub struct ExportList {
    items: Vec<Definition>,
}

impl ExportList {
    pub fn items(&self) -> &[Definition] {
        &self.items
    }

    fn has(&self, definition: &Definition) -> bool {
        self.items.iter().any(|d| d.native == definition.native)
    }

    fn push(&mut self, definition: Definition) {
        if !self.has(&definition) {
            self.items.push(definition);
        }
    }

    /// What `add`'s chooser offers: the client's objects of that kind not
    /// listed yet (generators only where they work), none checked.
    pub fn choices(&self, draft: &Draft, add: Add) -> Vec<Choice> {
        let choices = draft
            .definitions()
            .into_iter()
            .filter(|d| !self.has(d))
            .filter(|d| match (&d.native, add) {
                (Native::Gug(gug), Add::Gug) => functional(draft, gug),
                (Native::Class(_), Add::UrlClass)
                | (Native::Page(_), Add::Parser)
                | (Native::Login(_), Add::LoginScript) => true,
                _ => false,
            })
            .map(|definition| Choice {
                label: definition.name().to_owned(),
                definition,
                checked: false,
            })
            .collect();
        sorted(choices)
    }

    /// Add what was chosen in `add`'s chooser, with what it needs: for
    /// downloaders, the generators a nested one runs, the headers and
    /// bandwidth rules of their example URLs' domains, their URL classes
    /// (with API links) and parsers; for URL classes, their API links and
    /// parsers (`_AddGUG`, `_AddURLClass`).
    pub fn add(&mut self, draft: &Draft, add: Add, chosen: &[Definition]) {
        match add {
            Add::Gug | Add::UrlClass => {
                let definitions = draft.definitions();
                let mut wanted: Vec<Definition> = chosen.to_vec();
                if add == Add::Gug {
                    // (the generators nested ones run, where they work)
                    loop {
                        let before = wanted.len();
                        let members: Vec<Definition> = wanted
                            .iter()
                            .filter_map(|d| match &d.native {
                                Native::Gug(AnyGug::Nested(nested)) => Some(nested.gugs.clone()),
                                _ => None,
                            })
                            .flatten()
                            .filter_map(|(key, name)| {
                                definitions.iter().find(|d| {
                                    matches!(&d.native, Native::Gug(g) if g.key() == key && g.name() == name && functional(draft, g))
                                })
                            })
                            .filter(|d| !self.has(d))
                            .cloned()
                            .collect();
                        for member in members {
                            if !wanted.iter().any(|w| w.native == member.native) {
                                wanted.push(member);
                            }
                        }
                        if wanted.len() == before {
                            break;
                        }
                    }
                }
                let selected: BTreeSet<usize> = wanted
                    .iter()
                    .filter_map(|w| definitions.iter().position(|d| d.native == w.native))
                    .collect();
                let listed_domains: BTreeSet<String> = self
                    .items
                    .iter()
                    .filter_map(|d| match &d.native {
                        Native::Domain(m) => Some(m.domain.clone()),
                        _ => None,
                    })
                    .collect();
                let needed = draft.export(&selected);
                let of = |pick: fn(&Native) -> bool| {
                    needed
                        .iter()
                        .filter(|d| pick(&d.native))
                        .cloned()
                        .collect::<Vec<_>>()
                };
                let domains = of(|n| matches!(n, Native::Domain(_)));
                for definition in domains {
                    if let Native::Domain(m) = &definition.native
                        && !listed_domains.contains(&m.domain)
                    {
                        self.push(definition);
                    }
                }
                for definition in of(|n| matches!(n, Native::Gug(_))) {
                    self.push(definition);
                }
                for definition in of(|n| matches!(n, Native::Class(_))) {
                    self.push(definition);
                }
                for definition in of(|n| matches!(n, Native::Page(_))) {
                    self.push(definition);
                }
            }
            Add::Parser | Add::LoginScript => {
                for definition in chosen {
                    self.push(definition.clone());
                }
            }
        }
    }

    /// "add headers/bandwidth rules" for a typed domain: its (and its
    /// parents') headers and rules not listed yet, with each one's detail;
    /// or the reference's notice when there are none.
    pub fn add_domain(&mut self, draft: &Draft, domain: &str) -> Result<Vec<String>, String> {
        let listed = self
            .items
            .iter()
            .filter_map(|d| match &d.native {
                Native::Domain(m) => Some(m.domain.clone()),
                _ => None,
            })
            .collect();
        let found = draft.domain_metadata_for(&[domain.to_owned()], &listed);
        if found.is_empty() {
            return Err("No headers/bandwidth rules found!".into());
        }
        let details = found.iter().map(DomainMetadata::detailed_summary).collect();
        for metadata in found {
            self.push(Definition::new(Native::Domain(metadata)));
        }
        Ok(details)
    }

    /// Take the rows at `rows` out.
    pub fn remove(&mut self, rows: &BTreeSet<usize>) {
        let mut row = 0;
        self.items.retain(|_| {
            let keep = !rows.contains(&row);
            row += 1;
            keep
        });
    }

    /// The question "export to png" asks first.
    pub fn question(&self) -> String {
        let summaries: Vec<String> = self.items.iter().map(safe_summary).collect();
        format!(
            "The end-user will see this sort of summary:\n\n{}\n\nDoes that look good? (Ideally, every object should have correct and sane domains listed here)",
            summary(&summaries)
        )
    }
}

/// Whether a generator works (`IsFunctional`), as the client's own
/// definitions have it.
fn functional(draft: &Draft, gug: &AnyGug) -> bool {
    crate::gallery_source::check_functional(
        gug,
        &draft.downloaders,
        &draft.classes,
        GugOptions::default(),
    )
    .is_ok()
}

/// A package being imported, once its new objects are known.
#[derive(Debug, Clone)]
pub struct Import {
    definitions: Vec<Definition>,
    /// The new objects, as "Kind: name", and how many were already here.
    review: Review,
}

/// What an import says or asks next.
#[derive(Debug, Clone, PartialEq)]
pub enum Step {
    /// Nothing is new: the notice says so.
    NothingNew(String),
    /// "select objects to add", every new object checked.
    Choose(Vec<Choice>),
    /// The new domain metadata shown in detail, one notice each (the first
    /// eight), then the final question.
    Confirm {
        details: Vec<String>,
        question: String,
    },
}

impl Import {
    /// What importing `definitions` into `draft` would add.
    pub fn new(draft: &Draft, definitions: Vec<Definition>) -> Result<Self, String> {
        let review = draft.clone().import(definitions.clone())?;
        Ok(Self {
            definitions,
            review,
        })
    }

    /// The first step: the notice that nothing is new, the chooser (when
    /// "select objects from list" is on), or the confirmation.
    pub fn start(&self, draft: &Draft, select_from_list: bool) -> Result<Step, String> {
        if self.review.added.is_empty() {
            return Ok(Step::NothingNew(format!(
                "All {} downloader objects in that package appeared to already be in the client, so nothing need be added.",
                hydrus_core::numbers::human_int(
                    u64::try_from(self.review.duplicates).unwrap_or(u64::MAX)
                )
            )));
        }
        if select_from_list {
            let choices = self
                .review
                .added
                .iter()
                .filter_map(|label| {
                    Some(Choice {
                        label: label.clone(),
                        definition: self.labelled(label)?.clone(),
                        checked: true,
                    })
                })
                .collect();
            return Ok(Step::Choose(sorted(choices)));
        }
        self.confirm(draft, &self.review.added)
    }

    /// The incoming object a "Kind: name" label stands for.
    fn labelled(&self, label: &str) -> Option<&Definition> {
        self.definitions.iter().find(|d| label_of(d) == label)
    }

    /// The package with only the new objects `chosen` (by label) kept:
    /// duplicates stay, as they link what is kept.
    fn kept(&self, chosen: &[String]) -> Vec<Definition> {
        self.definitions
            .iter()
            .filter(|d| {
                let label = label_of(d);
                !self.review.added.contains(&label) || chosen.contains(&label)
            })
            .cloned()
            .collect()
    }

    /// The details and final question for the new objects `chosen` (by
    /// label).
    pub fn confirm(&self, draft: &Draft, chosen: &[String]) -> Result<Step, String> {
        let mut staged = draft.clone();
        staged.import(self.kept(chosen))?;
        let details = staged
            .domain_metadata
            .iter()
            .take(TOO_MANY_DM)
            .map(DomainMetadata::detailed_summary)
            .collect();
        // (generators, URL classes, parsers and login scripts by name, then
        // domain metadata by domain, each as the package has it)
        let mut kinds: [Vec<&Definition>; 4] = Default::default();
        for definition in &self.definitions {
            if !chosen.contains(&label_of(definition)) {
                continue;
            }
            let kind = match definition.native {
                Native::Gug(_) => 0,
                Native::Class(_) => 1,
                Native::Page(_) => 2,
                Native::Login(_) => 3,
                _ => continue,
            };
            kinds[kind].push(definition);
        }
        let mut summaries = Vec::new();
        for mut kind in kinds {
            kind.sort_by(|a, b| a.name().cmp(b.name()));
            summaries.extend(kind.into_iter().map(safe_summary));
        }
        summaries.extend(
            staged
                .domain_metadata
                .iter()
                .map(DomainMetadata::safe_summary),
        );
        Ok(Step::Confirm {
            details,
            question: format!(
                "The client is about to add and link these objects:\n\n{}\n\nDoes that sound good?",
                summary(&summaries)
            ),
        })
    }

    /// Import the new objects `chosen` (by label) into `draft`; the notice
    /// that says what was done.
    pub fn apply(&self, draft: &mut Draft, chosen: &[String]) -> Result<String, String> {
        let review = draft.import(self.kept(chosen))?;
        let gugs = review
            .added
            .iter()
            .filter(|l| l.starts_with("GUG: "))
            .count();
        let mut notice = format!(
            "Successfully added {} new downloaders and {} auxiliary objects.",
            hydrus_core::numbers::human_int(u64::try_from(gugs).unwrap_or(u64::MAX)),
            hydrus_core::numbers::human_int(
                u64::try_from(review.added.len() - gugs).unwrap_or(u64::MAX)
            )
        );
        if self.review.duplicates > 0 {
            notice.push_str(&format!(
                " {} duplicate objects were not added (but some additional metadata may have been merged).",
                hydrus_core::numbers::human_int(u64::try_from(self.review.duplicates).unwrap_or(u64::MAX))
            ));
        }
        Ok(notice)
    }

    /// Every new object, by label (all chosen).
    pub fn all(&self) -> Vec<String> {
        self.review.added.clone()
    }
}

/// "Kind: name", as the review and the chooser label an object.
fn label_of(definition: &Definition) -> String {
    format!(
        "{}: {}",
        crate::downloader_interchange::category(definition),
        definition.name()
    )
}

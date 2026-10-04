//! The search box's autocomplete: the tags matching what has been typed,
//! with their counts, as the reference's read autocomplete lists them
//! (display tags, in the page's file domains and tag service); and before
//! anything is typed, the system predicates: those that need no more input,
//! with theirs, then those that open an editor
//! ([`crate::predicate_editors`]).

use std::sync::Arc;

use hydrus_core::mime::SEARCHABLE_MIMES;
use hydrus_core::search::context::{LocationContext, TagContext};
use hydrus_core::service::ServiceType;
use hydrus_core::tag_presentation::TagPresentation;
use hydrus_core::{ServiceId, ServiceKey};
use hydrus_store::Store;
use hydrus_store::autocomplete::{
    self, AutocompleteInput, AutocompleteSettings, CountDomain, CountRange, TagDisplayType,
    TagSearchScope,
};
use hydrus_store::services::ServiceRegistry;

use crate::predicate_editors::{self, Blank};

/// One suggestion.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Suggestion {
    /// As listed, e.g. `blue eyes (12)`.
    pub label: String,
    /// What choosing it adds to the search, e.g. `blue eyes` or `-blue eyes`.
    pub predicate: String,
    /// A system predicate that needs more: the editor choosing it opens.
    pub editor: Option<Blank>,
}

pub struct Autocomplete {
    store: Arc<Store>,
    /// The page's file domains and tag service, which the counts are for.
    context: (LocationContext, TagContext),
    text: String,
    suggestions: Vec<Suggestion>,
    highlighted: usize,
}

impl std::fmt::Debug for Autocomplete {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Autocomplete")
            .field("text", &self.text)
            .field("suggestions", &self.suggestions.len())
            .field("highlighted", &self.highlighted)
            .finish_non_exhaustive()
    }
}

impl Autocomplete {
    pub fn new(store: Arc<Store>) -> Self {
        Self {
            store,
            context: (LocationContext::default(), TagContext::default()),
            text: String::new(),
            suggestions: Vec::new(),
            highlighted: 0,
        }
    }

    /// Count in these file domains and this tag service from now on.
    pub fn set_context(&mut self, location: &LocationContext, tags: &TagContext) {
        self.context = (location.clone(), tags.clone());
        let text = std::mem::take(&mut self.text);
        self.set_text(&text);
    }

    pub fn text(&self) -> &str {
        &self.text
    }

    pub fn suggestions(&self) -> &[Suggestion] {
        &self.suggestions
    }

    pub fn highlighted(&self) -> Option<usize> {
        (!self.suggestions.is_empty()).then_some(self.highlighted)
    }

    /// Move the highlight up (negative) or down, staying in the list.
    pub fn move_highlight(&mut self, by: isize) {
        if let Some(last) = self.suggestions.len().checked_sub(1) {
            self.highlighted = self.highlighted.saturating_add_signed(by).min(last);
        }
    }

    /// What pressing enter adds: the highlighted suggestion, else the text
    /// as typed (a system predicate, say).
    pub fn chosen(&self) -> Option<String> {
        match self.highlighted() {
            Some(i) => Some(self.suggestions[i].predicate.clone()),
            None => Some(self.text.trim().to_owned()).filter(|t| !t.is_empty()),
        }
    }

    pub fn clear(&mut self) {
        self.set_text("");
    }

    /// The text changed: search again.
    pub fn set_text(&mut self, text: &str) {
        text.clone_into(&mut self.text);
        self.highlighted = 0;
        self.suggestions = self.search(false).unwrap_or_default();
    }

    /// How the user has tags shown.
    fn presentation(&self) -> TagPresentation {
        self.store
            .read(hydrus_store::settings::get)
            .unwrap_or_default()
    }

    /// Explicit fetch, used by Ctrl+Space when automatic fetching is off.
    pub fn fetch(&mut self) {
        self.highlighted = 0;
        self.suggestions = self.search(true).unwrap_or_default();
    }

    fn search(&self, manual: bool) -> Option<Vec<Suggestion>> {
        if self.text.trim().is_empty() {
            return self.system_predicates();
        }
        let input = AutocompleteInput::parse(&self.text);
        let snapshot = self.store.snapshot();
        let registry = &snapshot.services;
        let (location, tags) = &self.context;
        let tag_service = if tags.is_all_known_tags() {
            None
        } else {
            Some(registry.by_key(&tags.service).ok()?.id)
        };
        let scope = TagSearchScope {
            domains: count_domains(registry, location),
            tag_service,
            display: TagDisplayType::Display,
            include_current: tags.include_current,
            include_pending: tags.include_pending,
        };
        let matches = self
            .store
            .read(|conn| {
                let rules =
                    hydrus_store::settings::get::<AutocompleteSettings>(conn)?.rules(&tags.service);
                let options = hydrus_store::settings::get::<
                    hydrus_store::tag_display_config::AutocompleteWidgetSettings,
                >(conn)?
                .options(&tags.service);
                let Some(query) = options.query(&input, &rules, manual) else {
                    return Ok(Vec::new());
                };
                autocomplete::search_tags(conn, registry, &snapshot.display, &scope, &query)
            })
            .ok()?;
        // the exact match first, then the most used
        let typed = input.search_text();
        let mut matches = matches;
        matches.sort_by(|a, b| {
            (b.tag == typed)
                .cmp(&(a.tag == typed))
                .then(b.count.min_total().cmp(&a.count.min_total()))
                .then(a.tag.cmp(&b.tag))
        });
        let sign = if input.inclusive() { "" } else { "-" };
        let presentation = self.presentation();
        Some(
            matches
                .into_iter()
                .map(|m| Suggestion {
                    label: format!("{sign}{} {}", presentation.render(&m.tag), m.count.suffix()),
                    predicate: format!("{sign}{}", m.tag),
                    editor: None,
                })
                .collect(),
        )
    }

    /// `system:everything`, `system:inbox` and `system:archive`, with how
    /// many files each finds in the page's file domains
    /// (`_GetFileSystemPredicates`): each domain's viewable files and
    /// inbox, combined as tag counts are; then those that open an editor.
    /// Searching all known files, only `system:everything` is offered
    /// (without a count) and the editors needing no file's metadata.
    fn system_predicates(&self) -> Option<Vec<Suggestion>> {
        let (location, _) = &self.context;
        let presentation = self.presentation();
        let snapshot = self.store.snapshot();
        let registry = &snapshot.services;
        let ratings = [
            ServiceType::LocalRatingLike,
            ServiceType::LocalRatingNumerical,
            ServiceType::LocalRatingIncDec,
            ServiceType::RatingLikeRepository,
            ServiceType::RatingNumericalRepository,
        ]
        .into_iter()
        .any(|t| registry.of_type(t).next().is_some());
        let blanks = predicate_editors::offered(location.is_all_known_files(), ratings)
            .into_iter()
            .map(|blank| Suggestion {
                label: presentation.render(blank.text()),
                predicate: blank.text().to_owned(),
                editor: Some(blank),
            });
        if location.is_all_known_files() {
            let mut out = vec![Suggestion {
                label: presentation.render("system:everything"),
                predicate: "system:everything".into(),
                editor: None,
            }];
            out.extend(blanks);
            return Some(out);
        }
        let real = |key: &ServiceKey| {
            registry
                .by_key(key)
                .ok()
                .filter(|s| REAL_FILE_SERVICES.contains(&s.service_type()))
                .map(|s| s.id)
        };
        let current: Vec<ServiceId> = location.current().iter().filter_map(real).collect();
        let deleted: Vec<ServiceId> = location.deleted().iter().filter_map(real).collect();
        let (everything, inbox, archive) = self
            .store
            .read(|conn| {
                let mut everything: Option<CountRange> = None;
                let mut inbox: Option<CountRange> = None;
                let mut archive: Option<CountRange> = None;
                let add = |total: &mut Option<CountRange>, n: i64| {
                    let n = CountRange::current(u64::try_from(n).unwrap_or(0));
                    match total {
                        Some(t) => t.merge(n),
                        None => *total = Some(n),
                    }
                };
                for &service in &current {
                    let viewable: i64 = conn.query_row(
                        "SELECT count(*) FROM file_domain_current AS d
                         JOIN files AS f ON f.hash_id = d.hash_id
                         WHERE d.service_id = ?1 AND f.mime IN rarray(?2)",
                        rusqlite::params![service, searchable_mimes()],
                        |r| r.get(0),
                    )?;
                    add(&mut everything, viewable);
                    // (inbox and archive can't be counted well with deleted
                    // files in the mix)
                    if !deleted.is_empty() {
                        continue;
                    }
                    let in_inbox: i64 = conn.query_row(
                        "SELECT count(*) FROM file_domain_current AS d
                         JOIN file_inbox AS i ON i.hash_id = d.hash_id
                         WHERE d.service_id = ?1",
                        [service],
                        |r| r.get(0),
                    )?;
                    add(&mut inbox, in_inbox);
                    add(&mut archive, viewable - in_inbox);
                }
                for &service in &deleted {
                    let n: i64 = conn.query_row(
                        "SELECT count(*) FROM file_domain_deleted WHERE service_id = ?1",
                        [service],
                        |r| r.get(0),
                    )?;
                    add(&mut everything, n);
                }
                Ok((everything, inbox, archive))
            })
            .ok()?;
        Some(
            [
                ("system:everything", everything),
                ("system:inbox", inbox),
                ("system:archive", archive),
            ]
            .into_iter()
            .map(|(predicate, count)| {
                let suffix = count.map(|c| c.suffix()).unwrap_or_default();
                let shown = presentation.render(predicate);
                Suggestion {
                    label: if suffix.is_empty() {
                        shown
                    } else {
                        format!("{shown} {suffix}")
                    },
                    predicate: predicate.to_owned(),
                    editor: None,
                }
            })
            .chain(blanks)
            .collect(),
        )
    }
}

/// The file services hydrus counts files in (`HC.REAL_FILE_SERVICES`).
const REAL_FILE_SERVICES: &[ServiceType] = &[
    ServiceType::LocalFileDomain,
    ServiceType::LocalFileUpdateDomain,
    ServiceType::LocalFileTrashDomain,
    ServiceType::HydrusLocalFileStorage,
    ServiceType::CombinedLocalFileDomains,
    ServiceType::CombinedDeletedFile,
    ServiceType::FileRepository,
    ServiceType::Ipfs,
];

/// The types hydrus counts as viewable files (`HC.SEARCHABLE_MIMES`).
fn searchable_mimes() -> std::rc::Rc<Vec<rusqlite::types::Value>> {
    std::rc::Rc::new(
        SEARCHABLE_MIMES
            .iter()
            .map(|m| rusqlite::types::Value::Integer(i64::from(m.code())))
            .collect(),
    )
}

/// Where tag counts come from for a page's file domains: each current
/// domain exactly, and, for deleted files, "deleted from anywhere" as an
/// upper bound (as the Client API's tag search does).
pub(crate) fn count_domains(
    registry: &ServiceRegistry,
    location: &LocationContext,
) -> Vec<CountDomain> {
    let mut domains: Vec<CountDomain> = location
        .current()
        .iter()
        .filter_map(|key| registry.by_key(key).ok())
        .map(|s| CountDomain {
            service: s.id,
            exact: true,
        })
        .collect();
    if !location.deleted().is_empty()
        && let Some(deleted) = registry.of_type(ServiceType::CombinedDeletedFile).next()
    {
        domains.push(CountDomain {
            service: deleted.id,
            exact: false,
        });
    }
    domains
}

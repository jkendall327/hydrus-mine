//! Managing files' tags (the reference's "manage tags" dialog, F3), on the
//! local tag services: the files' tags on the service chosen, an input
//! whose tag, entered, is added to the files (or, if they all have it,
//! removed), as the reference's is, and its suggestions. Changes wait
//! until applied, as the dialog's do. Plain Rust, tested directly.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use hydrus_core::{HashId, ServiceId, ServiceType, Tag};
use hydrus_store::Store;
use hydrus_store::autocomplete::{
    self, AutocompleteInput, AutocompleteSettings, CountDomain, TagDisplayType, TagSearchScope,
};
use hydrus_store::content::MappingAction;

/// How many suggestions are offered.
const SUGGESTIONS: usize = 12;

pub struct ManageTags {
    store: Arc<Store>,
    files: Vec<HashId>,
    /// The local tag services, by name.
    services: Vec<(ServiceId, String)>,
    service: usize,
    /// Each service's current tags, by file count, as stored.
    stored: Vec<BTreeMap<String, BTreeSet<HashId>>>,
    /// Each service's changes waiting to be applied: a tag added (`true`)
    /// to the files lacking it, or removed from them all.
    staged: Vec<BTreeMap<String, bool>>,
    text: String,
    suggestions: Vec<(String, String)>,
    highlighted: usize,
}

impl std::fmt::Debug for ManageTags {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ManageTags")
            .field("files", &self.files.len())
            .field("service", &self.service)
            .finish_non_exhaustive()
    }
}

impl ManageTags {
    /// Manage `files`' tags; `None` without files or local tag services.
    pub fn new(store: Arc<Store>, files: Vec<HashId>) -> Option<Self> {
        if files.is_empty() {
            return None;
        }
        let snapshot = store.snapshot();
        let services: Vec<(ServiceId, String)> = snapshot
            .services
            .of_type(ServiceType::LocalTag)
            .map(|s| (s.id, s.name.clone()))
            .collect();
        if services.is_empty() {
            return None;
        }
        let stored = services
            .iter()
            .map(|(service, _)| current_tags(&store, *service, &files))
            .collect();
        Some(Self {
            staged: vec![BTreeMap::new(); services.len()],
            store,
            files,
            services,
            service: 0,
            stored,
            text: String::new(),
            suggestions: Vec::new(),
            highlighted: 0,
        })
    }

    pub fn store(&self) -> &Arc<Store> {
        &self.store
    }

    pub fn service_names(&self) -> Vec<String> {
        self.services.iter().map(|(_, n)| n.clone()).collect()
    }

    pub fn service(&self) -> usize {
        self.service
    }

    pub fn choose_service(&mut self, index: usize) {
        if index < self.services.len() {
            self.service = index;
            self.set_text(&self.text.clone());
        }
    }

    /// The files having each tag on the service chosen, its waiting
    /// changes made.
    fn tags(&self) -> BTreeMap<String, usize> {
        let all = self.files.len();
        let mut out: BTreeMap<String, usize> = self.stored[self.service]
            .iter()
            .map(|(tag, files)| (tag.clone(), files.len()))
            .collect();
        for (tag, &add) in &self.staged[self.service] {
            if add {
                out.insert(tag.clone(), all);
            } else {
                out.remove(tag);
            }
        }
        out
    }

    /// The tag list's rows: each tag as shown, with how many of the files
    /// have it when not all do (`tag (2)`), sorted as the media viewer's
    /// list is; each as (tag, row).
    pub fn rows(&self) -> Vec<(String, String)> {
        use hydrus_core::tag_sort::sort_tags;
        let presentation: hydrus_core::tag_presentation::TagPresentation = self
            .store
            .read(hydrus_store::settings::get)
            .unwrap_or_default();
        let all = self.files.len();
        let mut rows: Vec<(String, usize)> = self.tags().into_iter().collect();
        sort_tags(
            &presentation.media_viewer_sort,
            &mut rows,
            |(tag, _)| tag,
            |(_, n)| *n as u64,
            &presentation.user_namespaces,
        );
        rows.into_iter()
            .map(|(tag, n)| {
                let mut row = presentation.render(&tag);
                if n < all {
                    row.push_str(&format!(" ({})", hydrus_core::numbers::human_int(n as u64)));
                }
                (tag, row)
            })
            .collect()
    }

    /// Enter a tag, as typed: added to the files that lack it, or, if
    /// they all have it, removed from them all. Errs on what isn't a tag.
    pub fn enter(&mut self, typed: &str) -> Result<(), String> {
        let tag = Tag::new(typed).ok_or_else(|| format!("\"{typed}\" is not a valid tag"))?;
        let tag = tag.as_str().to_owned();
        let everyone = self.tags().get(&tag) == Some(&self.files.len());
        let stored = self.stored[self.service].get(&tag).map_or(0, BTreeSet::len);
        let staged = &mut self.staged[self.service];
        if everyone {
            // (undoing an add, if it was one)
            if stored == 0 {
                staged.remove(&tag);
            } else {
                staged.insert(tag, false);
            }
        } else if stored == self.files.len() {
            staged.remove(&tag);
        } else {
            staged.insert(tag, true);
        }
        self.text.clear();
        self.suggestions.clear();
        self.highlighted = 0;
        Ok(())
    }

    /// A listed tag double-clicked: entered again (removed, if all the
    /// files have it).
    pub fn toggle_row(&mut self, index: usize) {
        if let Some((tag, _)) = self.rows().get(index).cloned() {
            let _ = self.enter(&tag);
        }
    }

    pub fn has_changes(&self) -> bool {
        self.staged.iter().any(|s| !s.is_empty())
    }

    /// Write the waiting changes.
    pub fn apply(&self) -> hydrus_store::Result<()> {
        let mut changes: Vec<(ServiceId, String, bool, Vec<HashId>)> = Vec::new();
        for (i, staged) in self.staged.iter().enumerate() {
            let service = self.services[i].0;
            for (tag, &add) in staged {
                let have = self.stored[i].get(tag);
                let files: Vec<HashId> = self
                    .files
                    .iter()
                    .copied()
                    .filter(|f| have.is_some_and(|h| h.contains(f)) != add)
                    .collect();
                changes.push((service, tag.clone(), add, files));
            }
        }
        self.store.write_content(move |w| {
            for (service, tag, add, files) in &changes {
                let tag = Tag::new(tag).expect("cleaned when entered");
                let id = hydrus_store::master::intern_tag(w.conn(), &tag)?;
                let action = if *add {
                    MappingAction::Add
                } else {
                    MappingAction::Delete
                };
                w.update_mappings(*service, &action, id, files)?;
            }
            Ok(())
        })
    }

    pub fn text(&self) -> &str {
        &self.text
    }

    /// The input's text changed: suggest the service's tags matching it,
    /// the exact match first, then the most used.
    pub fn set_text(&mut self, text: &str) {
        text.clone_into(&mut self.text);
        self.highlighted = 0;
        self.suggestions = self.search(false).unwrap_or_default();
    }

    /// (tag, label) of each suggestion.
    pub fn suggestions(&self) -> &[(String, String)] {
        &self.suggestions
    }

    pub fn highlighted(&self) -> Option<usize> {
        (!self.suggestions.is_empty()).then_some(self.highlighted)
    }

    pub fn move_highlight(&mut self, by: isize) {
        let n = self.suggestions.len();
        if n > 0 {
            self.highlighted = self.highlighted.saturating_add_signed(by).min(n - 1);
        }
    }

    /// Enter the input: the suggestion highlighted, else the text; with
    /// nothing typed, nothing (the window applies).
    pub fn enter_input(&mut self) -> Result<(), String> {
        let chosen = self
            .highlighted()
            .and_then(|i| self.suggestions.get(i))
            .map(|(tag, _)| tag.clone());
        match chosen {
            Some(tag) => self.enter(&tag),
            None if self.text.trim().is_empty() => Ok(()),
            None => {
                let text = self.text.clone();
                self.enter(&text)
            }
        }
    }

    /// Explicitly fetch suggestions even when fetch-as-you-type is disabled.
    pub fn fetch(&mut self) {
        self.suggestions = self.search(true).unwrap_or_default();
    }

    fn search(&self, manual: bool) -> Option<Vec<(String, String)>> {
        if self.text.trim().is_empty() {
            return Some(Vec::new());
        }
        let input = AutocompleteInput::parse(&self.text);
        let snapshot = self.store.snapshot();
        let registry = &snapshot.services;
        let service = self.services[self.service].0;
        let key = registry.get(service).ok()?.key.clone();
        let options = self
            .store
            .read(
                hydrus_store::settings::get::<
                    hydrus_store::tag_display_config::AutocompleteWidgetSettings,
                >,
            )
            .ok()?
            .options(&key);
        let domains = options
            .write_location
            .current()
            .iter()
            .filter_map(|key| registry.by_key(key).ok())
            .map(|s| CountDomain {
                service: s.id,
                exact: true,
            })
            .collect();
        let scope = TagSearchScope {
            domains,
            tag_service: registry
                .by_key(&options.write_tag_service)
                .ok()
                .filter(|s| s.service_type() != ServiceType::CombinedTag)
                .map(|s| s.id),
            display: TagDisplayType::Storage,
            include_current: true,
            include_pending: true,
        };
        let mut matches = self
            .store
            .read(|conn| {
                let rules = hydrus_store::settings::get::<AutocompleteSettings>(conn)?.rules(&key);
                let Some(query) = options.query(&input, &rules, manual) else {
                    return Ok(Vec::new());
                };
                autocomplete::search_tags(conn, registry, &snapshot.display, &scope, &query)
            })
            .ok()?;
        let typed = input.search_text();
        matches.sort_by(|a, b| {
            (b.tag == typed)
                .cmp(&(a.tag == typed))
                .then(b.count.min_total().cmp(&a.count.min_total()))
                .then(a.tag.cmp(&b.tag))
        });
        let presentation: hydrus_core::tag_presentation::TagPresentation = self
            .store
            .read(hydrus_store::settings::get)
            .unwrap_or_default();
        let mut out: Vec<(String, String)> = matches
            .into_iter()
            .take(SUGGESTIONS)
            .map(|m| {
                let label = format!("{} {}", presentation.render(&m.tag), m.count.suffix());
                (m.tag, label)
            })
            .collect();
        // the tag as typed comes first, so enter adds just that, as the
        // reference's tag entry does
        if let Some(typed) = Tag::new(&self.text)
            && !out.iter().any(|(tag, _)| tag == typed.as_str())
        {
            let tag = typed.as_str().to_owned();
            out.insert(0, (tag.clone(), presentation.render(&tag)));
        }
        Some(out)
    }
}

/// The files' current tags on `service`, as stored, each with the files
/// that have it.
fn current_tags(
    store: &Store,
    service: ServiceId,
    files: &[HashId],
) -> BTreeMap<String, BTreeSet<HashId>> {
    let snapshot = store.snapshot();
    let Ok(batch) = store.read(|c| hydrus_store::media::load(c, &snapshot.services, None, files))
    else {
        return BTreeMap::new();
    };
    let mut out: BTreeMap<String, BTreeSet<HashId>> = BTreeMap::new();
    for m in &batch.results {
        let Some(tags) = m.tags.get(&service) else {
            continue;
        };
        for id in tags
            .by_status
            .get(&hydrus_core::ContentStatus::Current)
            .into_iter()
            .flatten()
        {
            if let Some(tag) = batch.tags.get(id) {
                out.entry(tag.to_string()).or_default().insert(m.hash_id);
            }
        }
    }
    out
}

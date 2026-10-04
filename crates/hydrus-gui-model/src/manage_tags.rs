//! Managing files' tags (the reference's "manage tags" dialog, F3), on the
//! local tag services: the files' tags on the service chosen, an input
//! whose tag, entered, is added to the files (or, if they all have it,
//! removed), as the reference's is, and its suggestions. Changes wait
//! until applied, as the dialog's do. Plain Rust, tested directly.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use hydrus_core::{HashId, ServiceId, ServiceType, Tag};
use hydrus_store::Store;
use hydrus_store::content::MappingAction;

use crate::write_autocomplete::WriteAutocomplete;

/// A rendered row retains its logical tag even when displaying an implied parent.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TagRow {
    pub tag: String,
    pub colour_tag: String,
    pub label: String,
    pub parent_row: bool,
}

pub struct ManageTags {
    store: Arc<Store>,
    files: Vec<HashId>,
    location: hydrus_core::search::context::LocationContext,
    /// The local tag services, by name.
    services: Vec<(ServiceId, String)>,
    service: usize,
    /// Each service's current tags, by file count, as stored.
    stored: Vec<BTreeMap<String, BTreeSet<HashId>>>,
    /// Each service's changes waiting to be applied: a tag added (`true`)
    /// to the files lacking it, or removed from them all.
    staged: Vec<BTreeMap<String, bool>>,
    input: WriteAutocomplete,
    dialog_preferences: hydrus_store::tag_editing::TagEditingSettings,
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
        let mut services: Vec<(ServiceId, String)> = snapshot
            .services
            .of_type(ServiceType::LocalTag)
            .map(|s| (s.id, s.name.clone()))
            .collect();
        services.sort_by_key(|(_, name)| name.to_lowercase());
        if services.is_empty() {
            return None;
        }
        let preference = store
            .read(hydrus_store::settings::get::<hydrus_store::tag_editing::TagEditingSettings>)
            .unwrap_or_default();
        let service = services
            .iter()
            .position(|(id, _)| {
                snapshot
                    .services
                    .get(*id)
                    .is_ok_and(|s| s.key == preference.default_service)
            })
            .unwrap_or(0);
        let stored: Vec<BTreeMap<String, BTreeSet<HashId>>> = services
            .iter()
            .map(|(service, _)| current_tags(&store, *service, &files))
            .collect();
        let location = hydrus_core::search::context::LocationContext::default();
        let mut input = WriteAutocomplete::new(
            store.clone(),
            snapshot.services.get(services[service].0).ok()?.key.clone(),
            location.clone(),
        );
        input.set_context_tags(stored[service].keys().cloned());
        Some(Self {
            staged: vec![BTreeMap::new(); services.len()],
            store,
            files,
            location,
            services,
            service,
            stored,
            input,
            dialog_preferences: preference,
        })
    }

    /// File domain of the page/viewer that launched this editor.
    pub fn set_location(&mut self, location: hydrus_core::search::context::LocationContext) {
        self.location = location;
        if let Some(key) = self.migration_service_key() {
            self.input.set_context(key, self.location.clone());
        }
    }
    /// Selected file IDs used to launch migration.
    pub fn files(&self) -> &[HashId] {
        &self.files
    }
    /// Stable key of the active tag service.
    pub fn migration_service_key(&self) -> Option<hydrus_core::ServiceKey> {
        self.store
            .snapshot()
            .services
            .get(self.services[self.service].0)
            .ok()
            .map(|s| s.key.clone())
    }
    /// Refresh committed tags while preserving this editor's staged changes.
    pub fn refresh_stored(&mut self) {
        self.stored = self
            .services
            .iter()
            .map(|(id, _)| current_tags(&self.store, *id, &self.files))
            .collect();
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

    /// Change the active service and remember that tab immediately when enabled.
    /// Tag edits remain staged; preference memory also survives tag-dialog Cancel.
    pub fn choose_service(&mut self, index: usize) -> hydrus_store::Result<()> {
        if index < self.services.len() && index != self.service {
            let key = self
                .store
                .snapshot()
                .services
                .get(self.services[index].0)?
                .key
                .clone();
            self.store
                .write(move |ctx| hydrus_store::tag_editing::remember_service(ctx.conn(), &key))?;
            self.service = index;
            if let Some(key) = self.migration_service_key() {
                self.input.set_context_tags(self.tags().into_keys());
                self.input.set_context(key, self.location.clone());
            }
        }
        Ok(())
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
    fn plain_rows(&self) -> Vec<(String, String)> {
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

    /// Preferences are defaults captured when this dialog opens, as in Qt.
    pub fn dialog_preferences(&self) -> &hydrus_store::tag_editing::TagEditingSettings {
        &self.dialog_preferences
    }
    pub fn rows(&self) -> Vec<(String, String)> {
        self.display_rows()
            .into_iter()
            .map(|row| (row.tag, row.label))
            .collect()
    }
    pub fn display_rows(&self) -> Vec<TagRow> {
        let rows = self.plain_rows();
        let presentation: hydrus_core::tag_presentation::TagPresentation = self
            .store
            .read(hydrus_store::settings::get)
            .unwrap_or_default();
        let snapshot = self.store.snapshot();
        let graph = snapshot.display.get(self.services[self.service].0);
        let details = self
            .store
            .read(|conn| {
                let mut details = BTreeMap::new();
                for (tag, _) in &rows {
                    let Some(cleaned) = Tag::new(tag) else {
                        continue;
                    };
                    let Some(id) = hydrus_store::master::tag_id(conn, &cleaned)? else {
                        continue;
                    };
                    let ideal = graph.ideal(id);
                    let mut ids = graph.ancestors(id).to_vec();
                    ids.push(ideal);
                    let texts = hydrus_store::master::tags(conn, &ids)?;
                    let ideal = (ideal != id).then(|| texts[&ideal].as_str().to_owned());
                    let mut parents: Vec<_> = graph
                        .ancestors(id)
                        .iter()
                        .map(|id| texts[id].as_str().to_owned())
                        .collect();
                    hydrus_core::sort::human_sort(&mut parents);
                    details.insert(tag.clone(), (ideal, parents));
                }
                Ok(details)
            })
            .unwrap_or_default();
        let preferences = &self.dialog_preferences;
        let mut out = Vec::new();
        for (tag, mut label) in rows {
            let (ideal, parents) = details.get(&tag).cloned().unwrap_or_default();
            if preferences.tag_list_show_siblings
                && let Some(ideal) = ideal
            {
                label.push_str(&presentation.sibling_connector);
                label.push_str(&ideal);
            }
            if preferences.tag_list_show_parents
                && !preferences.tag_list_expand_parents
                && !parents.is_empty()
            {
                label.push_str(&format!(
                    " ({} parents)",
                    hydrus_core::numbers::human_int(parents.len() as u64)
                ));
            }
            out.push(TagRow {
                tag: tag.clone(),
                colour_tag: tag.clone(),
                label,
                parent_row: false,
            });
            if preferences.tag_list_show_parents && preferences.tag_list_expand_parents {
                for parent in parents {
                    out.push(TagRow {
                        tag: tag.clone(),
                        colour_tag: parent.clone(),
                        label: format!("    {parent}"),
                        parent_row: true,
                    });
                }
            }
        }
        out
    }

    /// Enter a tag, as typed: added to the files that lack it, or, if
    /// they all have it, removed from them all. Errs on what isn't a tag.
    pub fn enter(&mut self, typed: &str) -> Result<(), String> {
        self.stage_tag(typed)?;
        self.input.clear();
        Ok(())
    }
    fn stage_tag(&mut self, typed: &str) -> Result<(), String> {
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
        self.input.set_context_tags(self.tags().into_keys());
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
        self.input.text()
    }
    pub fn set_text(&mut self, text: &str) {
        self.input.set_text(text);
    }
    pub fn suggestions(&self) -> &[(String, String)] {
        self.input.suggestions()
    }
    pub fn highlighted(&self) -> Option<usize> {
        self.input.highlighted()
    }
    pub fn move_highlight(&mut self, by: isize) {
        self.input.move_highlight(by);
    }
    pub fn enter_input(&mut self) -> Result<(), String> {
        for tag in self.input.chosen_tags(None) {
            self.enter(&tag)?;
        }
        Ok(())
    }
    pub fn choose_suggestion(&mut self, index: usize) -> Result<(), String> {
        for tag in self.input.chosen_tags(Some(index)) {
            self.enter(&tag)?;
        }
        Ok(())
    }
    pub fn fetch(&mut self) {
        self.input.fetch();
    }
    pub fn write_input(&self) -> &WriteAutocomplete {
        &self.input
    }
    pub fn write_input_mut(&mut self) -> &mut WriteAutocomplete {
        &mut self.input
    }
    pub fn autocomplete_tab(&self) -> crate::write_autocomplete::Tab {
        self.input.tab()
    }
    pub fn choose_autocomplete_tab(&mut self, tab: crate::write_autocomplete::Tab) {
        self.input.set_context_tags(self.tags().into_keys());
        self.input.set_tab(tab);
    }
    pub fn suggestion_rows(&self) -> &[crate::write_autocomplete::Suggestion] {
        self.input.rows()
    }
    pub fn autocomplete_options(&self) -> hydrus_store::tag_editing::TagEditingSettings {
        self.input.options()
    }
    /// Clipboard entry only adds: unlike typed entry it never toggles existing tags off.
    pub fn paste_tags(&mut self, tags: &[String]) -> Result<(), String> {
        let cleaned: Vec<Tag> = tags
            .iter()
            .map(|t| Tag::new(t).ok_or_else(|| format!("\"{t}\" is not a valid tag")))
            .collect::<Result<_, _>>()?;
        for tag in cleaned {
            if self.tags().get(tag.as_str()) != Some(&self.files.len()) {
                self.stage_tag(tag.as_str())?;
            }
        }
        self.input.set_context_tags(self.tags().into_keys());
        Ok(())
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

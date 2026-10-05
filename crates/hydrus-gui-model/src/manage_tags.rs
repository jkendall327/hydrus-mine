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
    pub parts: Vec<hydrus_core::tag_presentation::TagText>,
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
    deleted: Vec<BTreeMap<String, BTreeSet<HashId>>>,
    /// Each service's changes waiting to be applied: a tag added (`true`)
    /// to the files lacking it, or removed from them all.
    staged: Vec<BTreeMap<String, BTreeMap<HashId, bool>>>,
    input: WriteAutocomplete,
    dialog_preferences: hydrus_store::tag_editing::TagEditingSettings,
    suggestion_preferences: hydrus_store::settings::TagSuggestionSettings,
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
        let deleted = services
            .iter()
            .map(|(service, _)| {
                status_tags(
                    &store,
                    *service,
                    &files,
                    hydrus_core::ContentStatus::Deleted,
                )
            })
            .collect();
        let location = hydrus_core::search::context::LocationContext::default();
        let mut input = WriteAutocomplete::new(
            store.clone(),
            snapshot.services.get(services[service].0).ok()?.key.clone(),
            location.clone(),
        );
        input.set_context_tags(stored[service].keys().cloned());
        let suggestion_preferences = store.read(hydrus_store::settings::get).unwrap_or_default();
        Some(Self {
            staged: vec![BTreeMap::new(); services.len()],
            store,
            files,
            location,
            services,
            service,
            stored,
            deleted,
            input,
            dialog_preferences: preference,
            suggestion_preferences,
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
        self.deleted = self
            .services
            .iter()
            .map(|(id, _)| {
                status_tags(
                    &self.store,
                    *id,
                    &self.files,
                    hydrus_core::ContentStatus::Deleted,
                )
            })
            .collect();
        self.stored = self
            .services
            .iter()
            .map(|(id, _)| current_tags(&self.store, *id, &self.files))
            .collect();
    }
    /// The toggle is a live global preference, independent of staged mappings.
    pub fn show_deleted(&self) -> bool {
        self.store
            .read(hydrus_store::settings::get::<hydrus_store::tag_editing::ManageTagsSettings>)
            .unwrap_or_default()
            .show_deleted
    }
    pub fn flip_show_deleted(&self) -> hydrus_store::Result<()> {
        self.store.write(|ctx| {
            let mut settings: hydrus_store::tag_editing::ManageTagsSettings =
                hydrus_store::settings::get(ctx.conn())?;
            settings.show_deleted = !settings.show_deleted;
            hydrus_store::settings::set(ctx.conn(), &settings)
        })
    }
    fn deleted_tags(&self) -> BTreeMap<String, BTreeSet<HashId>> {
        let mut out = self.deleted[self.service].clone();
        for (tag, changes) in &self.staged[self.service] {
            for (file, add) in changes {
                if *add {
                    if let Some(files) = out.get_mut(tag) {
                        files.remove(file);
                    }
                } else {
                    out.entry(tag.clone()).or_default().insert(*file);
                }
            }
        }
        out.retain(|_, files| !files.is_empty());
        out
    }
    pub fn deleted_count(&self) -> usize {
        self.deleted_tags().values().map(BTreeSet::len).sum()
    }
    pub fn deleted_count_label(&self) -> String {
        format!(
            "{} deleted mappings",
            hydrus_core::numbers::human_int(self.deleted_count() as u64)
        )
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
    fn current_tags(&self) -> BTreeMap<String, BTreeSet<HashId>> {
        let mut out = self.stored[self.service].clone();
        for (tag, changes) in &self.staged[self.service] {
            for (file, add) in changes {
                if *add {
                    out.entry(tag.clone()).or_default().insert(*file);
                } else if let Some(files) = out.get_mut(tag) {
                    files.remove(file);
                }
            }
        }
        out.retain(|_, files| !files.is_empty());
        out
    }
    fn tags(&self) -> BTreeMap<String, usize> {
        self.current_tags()
            .into_iter()
            .map(|(tag, files)| (tag, files.len()))
            .collect()
    }
    /// The child is bound to this ordered selection and active service's preview.
    pub fn incremental(&self) -> Option<crate::incremental_tagging::IncrementalTagging> {
        if self.files.len() < 2 {
            return None;
        }
        let mut current: BTreeMap<HashId, BTreeSet<String>> = self
            .files
            .iter()
            .map(|file| (*file, BTreeSet::new()))
            .collect();
        for (tag, files) in self.current_tags() {
            for file in files {
                current.entry(file).or_default().insert(tag.clone());
            }
        }
        let pending = status_tags(
            &self.store,
            self.services[self.service].0,
            &self.files,
            hydrus_core::ContentStatus::Pending,
        );
        for (tag, files) in pending {
            for file in files {
                current.entry(file).or_default().insert(tag.clone());
            }
        }
        Some(crate::incremental_tagging::IncrementalTagging::new(
            self.store.clone(),
            self.files.clone(),
            current,
        ))
    }
    /// Add one numbered tag to each original file, preserving all other tags.
    pub fn apply_incremental(
        &mut self,
        service: usize,
        pairs: Vec<(HashId, Tag)>,
    ) -> Result<(), String> {
        if service != self.service
            || pairs.len() != self.files.len()
            || pairs
                .iter()
                .zip(&self.files)
                .any(|((file, _), expected)| file != expected)
        {
            return Err("The incremental selection or tag service changed.".into());
        }
        for (file, tag) in pairs {
            self.stage_mapping(tag.as_str(), file, true);
        }
        self.input.set_context_tags(self.tags().into_keys());
        Ok(())
    }
    fn stage_mapping(&mut self, tag: &str, file: HashId, add: bool) {
        let already = self.stored[self.service]
            .get(tag)
            .is_some_and(|files| files.contains(&file));
        let deleted = self.deleted[self.service]
            .get(tag)
            .is_some_and(|files| files.contains(&file));
        let changes = self.staged[self.service].entry(tag.to_owned()).or_default();
        if (add && already) || (!add && deleted) {
            changes.remove(&file);
        } else {
            changes.insert(file, add);
        }
        if changes.is_empty() {
            self.staged[self.service].remove(tag);
        }
    }
    /// Storage rows include current counts and, when shown, deleted counts,
    /// sorted as the media viewer's list is; each as (logical tag, row).
    fn plain_rows(&self) -> Vec<(String, String)> {
        use hydrus_core::tag_sort::sort_tags;
        let presentation: hydrus_core::tag_presentation::TagPresentation = self
            .store
            .read(hydrus_store::settings::get)
            .unwrap_or_default();
        let deleted = if self.show_deleted() {
            self.deleted_tags()
        } else {
            BTreeMap::new()
        };
        let mut tags = self.tags();
        for tag in deleted.keys() {
            tags.entry(tag.clone()).or_default();
        }
        let mut rows: Vec<(String, usize)> = tags.into_iter().collect();
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
                if n > 0 {
                    row.push_str(&format!(" ({})", hydrus_core::numbers::human_int(n as u64)));
                }
                if let Some(files) = deleted.get(&tag) {
                    row.push_str(&format!(
                        " (X{})",
                        hydrus_core::numbers::human_int(files.len() as u64)
                    ));
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
        let colours: hydrus_core::tag_presentation::NamespaceColours = self
            .store
            .read(hydrus_store::settings::get)
            .unwrap_or_default();
        let style: hydrus_core::tag_presentation::SiblingConnectorColours = self
            .store
            .read(hydrus_store::settings::get)
            .unwrap_or_default();
        let preferences = &self.dialog_preferences;
        let mut out = Vec::new();
        for (tag, mut label) in rows {
            let (ideal, parents) = details.get(&tag).cloned().unwrap_or_default();
            let prefix = label.clone();
            let shown_ideal = ideal.filter(|_| preferences.tag_list_show_siblings);
            if let Some(ideal) = &shown_ideal {
                label.push_str(&presentation.sibling_connector);
                label.push_str(ideal);
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
            let parts = if let Some(ideal) = shown_ideal.as_ref() {
                let suffix = label
                    .strip_prefix(&format!(
                        "{prefix}{}{ideal}",
                        presentation.sibling_connector
                    ))
                    .unwrap_or_default()
                    .to_owned();
                style.runs(
                    (&tag, prefix),
                    &presentation.sibling_connector,
                    (ideal, ideal.clone()),
                    suffix,
                    &colours,
                    false,
                )
            } else {
                let suffix = label.strip_prefix(&prefix).unwrap_or_default().to_owned();
                style.parent_runs((&tag, prefix), suffix, &colours, false)
            };
            out.push(TagRow {
                tag: tag.clone(),
                colour_tag: tag.clone(),
                label,
                parts,
                parent_row: false,
            });
            if preferences.tag_list_show_parents && preferences.tag_list_expand_parents {
                for parent in parents {
                    out.push(TagRow {
                        tag: tag.clone(),
                        colour_tag: parent.clone(),
                        label: format!("    {parent}"),
                        parts: Vec::new(),
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

    /// Side panels capture their layout at opening, like the reference dialog.
    pub fn suggestion_preferences(&self) -> &hydrus_store::settings::TagSuggestionSettings {
        &self.suggestion_preferences
    }

    /// Capture the service and current/pending selected-file context for a worker.
    pub fn related_query(
        &self,
        local: bool,
        display: bool,
    ) -> hydrus_store::Result<hydrus_store::related_tags::Query> {
        let preferences: hydrus_store::related_tags::Settings =
            self.store.read(hydrus_store::settings::get)?;
        let mut tags = self.current_tags();
        for (tag, files) in status_tags(
            &self.store,
            self.services[self.service].0,
            &self.files,
            hydrus_core::ContentStatus::Pending,
        ) {
            tags.entry(tag).or_default().extend(files);
        }
        Ok(hydrus_store::related_tags::Query {
            service: self.migration_service_key().ok_or_else(|| {
                hydrus_store::StoreError::Invalid("The tag service has been removed.".into())
            })?,
            searches: tags.into_keys().collect(),
            local,
            display,
            weights: preferences.weights,
            concurrence_percent: preferences.concurrence_percent,
        })
    }
    /// Filter related results with the same add-only current/pending rule as other sides.
    pub fn useful_related(&self, source: Vec<String>) -> Vec<String> {
        let mut counts = self.current_tags();
        for (tag, files) in status_tags(
            &self.store,
            self.services[self.service].0,
            &self.files,
            hydrus_core::ContentStatus::Pending,
        ) {
            counts.entry(tag).or_default().extend(files);
        }
        crate::tag_suggestions::useful(
            source,
            &counts
                .into_iter()
                .map(|(tag, files)| (tag, files.len()))
                .collect(),
            self.files.len(),
        )
    }

    /// Re-read broadcast most-used changes, filtering current/pending tags on every file.
    pub fn side_suggestions(&self, recent: bool) -> Vec<String> {
        let snapshot = self.store.snapshot();
        let service = self.services[self.service].0;
        let source = if recent {
            crate::tag_suggestions::recent(
                &self.store,
                service,
                self.suggestion_preferences.recent_limit.unwrap_or(20),
            )
            .unwrap_or_default()
        } else {
            snapshot
                .services
                .get(service)
                .map(|s| crate::tag_suggestions::most_used(&self.store, &s.key))
                .unwrap_or_default()
        };
        let mut counts = self.current_tags();
        for (tag, files) in status_tags(
            &self.store,
            service,
            &self.files,
            hydrus_core::ContentStatus::Pending,
        ) {
            counts.entry(tag).or_default().extend(files);
        }
        crate::tag_suggestions::useful(
            source,
            &counts
                .into_iter()
                .map(|(tag, files)| (tag, files.len()))
                .collect(),
            self.files.len(),
        )
    }

    /// Suggestion activation only adds missing mappings, even for a stale selection.
    pub fn add_side_suggestions(&mut self, tags: &[String]) {
        for tag in tags.iter().filter_map(|tag| Tag::new(tag)) {
            for file in self.files.clone() {
                self.stage_mapping(tag.as_str(), file, true);
            }
        }
        self.input.set_context_tags(self.tags().into_keys());
    }
    fn stage_tag(&mut self, typed: &str) -> Result<(), String> {
        let tag = Tag::new(typed).ok_or_else(|| format!("\"{typed}\" is not a valid tag"))?;
        let tag = tag.as_str().to_owned();
        let everyone = self.tags().get(&tag) == Some(&self.files.len());
        for file in self.files.clone() {
            self.stage_mapping(&tag, file, !everyone);
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
            for (tag, edits) in staged {
                for add in [true, false] {
                    let files: Vec<_> = edits
                        .iter()
                        .filter_map(|(file, value)| (*value == add).then_some(*file))
                        .collect();
                    if !files.is_empty() {
                        changes.push((service, tag.clone(), add, files));
                    }
                }
            }
        }
        self.store.write_content(move |w| {
            let now = hydrus_core::time::TimestampMs::now().0;
            for (service, tag, add, files) in &changes {
                let tag = Tag::new(tag).expect("cleaned when entered");
                let id = hydrus_store::master::intern_tag(w.conn(), &tag)?;
                let action = if *add {
                    MappingAction::Add
                } else {
                    MappingAction::Delete
                };
                w.update_mappings(*service, &action, id, files)?;
                if *add {
                    let preferences: hydrus_store::settings::TagSuggestionSettings = hydrus_store::settings::get(w.conn())?;
                    if preferences.recent_limit.is_some() {
                        w.conn().execute("INSERT INTO recent_tags(service_id,tag_id,used_ms) VALUES(?,?,?) ON CONFLICT(service_id,tag_id) DO UPDATE SET used_ms=excluded.used_ms",rusqlite::params![service,id,now])?;
                    }
                }
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
    status_tags(store, service, files, hydrus_core::ContentStatus::Current)
}

fn status_tags(
    store: &Store,
    service: ServiceId,
    files: &[HashId],
    status: hydrus_core::ContentStatus,
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
        for id in tags.by_status.get(&status).into_iter().flatten() {
            if let Some(tag) = batch.tags.get(id) {
                out.entry(tag.to_string()).or_default().insert(m.hash_id);
            }
        }
    }
    out
}

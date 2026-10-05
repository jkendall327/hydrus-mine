//! Shared write-tag suggestions, highlighted entry and multiline paste decisions.
use std::{collections::BTreeSet, sync::Arc};

use hydrus_core::{ServiceKey, ServiceType, Tag, search::context::LocationContext};
use hydrus_store::{
    Store,
    autocomplete::{
        self, AutocompleteInput, AutocompleteSettings, CountRange, TagDisplayType, TagMatch,
        TagSearchScope,
    },
    settings,
    tag_editing::TagEditingSettings,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Suggestion {
    pub tag: String,
    pub label: String,
    pub parts: Vec<hydrus_core::tag_presentation::TagText>,
    pub colour_tag: String,
    pub counted: bool,
    pub count: CountRange,
    pub parents: Vec<String>,
    /// Expanded parent rows belong to their originating tag, as in the Qt list.
    pub parent_row: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Paste {
    /// A single line, or declined multiline input, is left to the text editor.
    Text,
    Tags(Vec<String>),
    Confirm {
        message: String,
        tags: Vec<String>,
    },
}

/// Clean, deduplicate and sort pasted tags; clipboard entry only adds tags.
pub fn pasted_tags(text: &str) -> Vec<String> {
    let mut tags: Vec<_> = text
        .lines()
        // HydrusText.DeserialiseNewlinedTexts strips clipboard line input
        // before CleanTags, so a bare colon followed by spaces is empty.
        .map(|line| line.trim_start_matches('\u{feff}').trim())
        .filter_map(Tag::new)
        .map(|t| t.as_str().to_owned())
        .collect();
    tags.sort();
    tags.dedup();
    tags
}

pub fn paste(text: &str, button: bool, options: &TagEditingSettings) -> Paste {
    let tags = pasted_tags(text);
    if button || (tags.len() > 1 && options.skip_multiline_paste_confirmation) {
        Paste::Tags(tags)
    } else if tags.len() > 1 {
        Paste::Confirm {
            message: format!(
                "You have pasted multiple lines of content. Want to enter them all as separate tags? You entered:\n\n{}",
                tags.join("\n")
            ),
            tags,
        }
    } else {
        Paste::Text
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Tab {
    #[default]
    Tags,
    Favourites,
    Children,
}
impl Tab {
    pub fn index(self) -> usize {
        match self {
            Self::Tags => 0,
            Self::Favourites => 1,
            Self::Children => 2,
        }
    }
    pub fn from_index(index: usize) -> Self {
        match index {
            1 => Self::Favourites,
            2 => Self::Children,
            _ => Self::Tags,
        }
    }
}

/// The reference tag list keeps earlier selections during Shift ranges and
/// uses Ctrl+Shift to remove a reversible range, unlike a table selection.
#[derive(Debug, Default)]
pub(crate) struct Selection {
    pub(crate) selected: BTreeSet<usize>,
    last: Option<usize>,
    anchor: Option<usize>,
    added: BTreeSet<usize>,
    removed: BTreeSet<usize>,
}
impl Selection {
    pub(crate) fn reset(&mut self, first: Option<usize>) {
        *self = Self::default();
        if let Some(first) = first {
            self.click(first, false, false);
        }
    }
    pub(crate) fn click(&mut self, hit: usize, ctrl: bool, shift: bool) {
        if !shift {
            self.anchor = Some(hit);
            self.added.clear();
            self.removed.clear();
        }
        if shift {
            if self.anchor.is_none() || self.last.is_none() {
                self.anchor = Some(self.last.unwrap_or(hit));
            }
            if ctrl && !self.added.is_empty() {
                self.anchor = self.last;
                self.added.clear();
            } else if !ctrl && !self.removed.is_empty() {
                self.anchor = self.last;
                self.removed.clear();
            }
            let anchor = self.anchor.unwrap_or(hit);
            let range: BTreeSet<_> = (anchor.min(hit)..=anchor.max(hit)).collect();
            if ctrl {
                let removed: Vec<_> = range.intersection(&self.selected).copied().collect();
                let restored: Vec<_> = self.removed.difference(&range).copied().collect();
                for index in removed {
                    self.selected.remove(&index);
                    self.removed.insert(index);
                }
                for index in restored {
                    self.selected.insert(index);
                    self.removed.remove(&index);
                }
            } else {
                let added: Vec<_> = range.difference(&self.selected).copied().collect();
                let restored: Vec<_> = self.added.difference(&range).copied().collect();
                for index in added {
                    self.selected.insert(index);
                    self.added.insert(index);
                }
                for index in restored {
                    self.selected.remove(&index);
                    self.added.remove(&index);
                }
            }
        } else if ctrl {
            if !self.selected.remove(&hit) {
                self.selected.insert(hit);
            }
        } else if !self.selected.contains(&hit) {
            self.selected.clear();
            self.selected.insert(hit);
        }
        self.last = Some(hit);
    }
}

pub struct WriteAutocomplete {
    store: Arc<Store>,
    service: ServiceKey,
    location: LocationContext,
    text: String,
    rows: Vec<Suggestion>,
    suggestions: Vec<(String, String)>,
    selections: [Selection; 3],
    selection_rows: [Vec<(String, CountRange)>; 3],
    tab: Tab,
    context_tags: std::collections::BTreeSet<String>,
    decorations: [[Option<bool>; 3]; 3],
    domains: std::collections::BTreeMap<String, crate::domains::Domains>,
}
impl std::fmt::Debug for WriteAutocomplete {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WriteAutocomplete")
            .field("service", &self.service)
            .field("text", &self.text)
            .field("rows", &self.rows.len())
            .finish_non_exhaustive()
    }
}
impl WriteAutocomplete {
    pub fn new(store: Arc<Store>, service: ServiceKey, location: LocationContext) -> Self {
        Self {
            store,
            service,
            location,
            text: String::new(),
            rows: Vec::new(),
            suggestions: Vec::new(),
            selections: std::array::from_fn(|_| Selection::default()),
            selection_rows: std::array::from_fn(|_| Vec::new()),
            tab: Tab::Tags,
            context_tags: std::collections::BTreeSet::new(),
            decorations: [[None; 3]; 3],
            domains: std::collections::BTreeMap::new(),
        }
    }
    pub fn store(&self) -> &Arc<Store> {
        &self.store
    }
    pub fn options(&self) -> TagEditingSettings {
        let mut options: TagEditingSettings = self.store.read(settings::get).unwrap_or_default();
        let [parents, expanded, siblings] = self.decorations[self.tab.index()];
        if let Some(value) = parents {
            options.autocomplete_show_parents = value;
        }
        if let Some(value) = expanded {
            options.autocomplete_expand_parents = value;
        }
        if let Some(value) = siblings {
            options.autocomplete_show_siblings = value;
        }
        options
    }
    pub fn decorate(&mut self, tab: Tab, kind: crate::write_tag_menu::Decoration, value: bool) {
        use crate::write_tag_menu::Decoration;
        let index = match kind {
            Decoration::Parents => 0,
            Decoration::Expanded => 1,
            Decoration::Siblings => 2,
        };
        self.decorations[tab.index()][index] = Some(value);
        self.refresh(true);
    }
    pub fn set_context(&mut self, service: ServiceKey, location: LocationContext) {
        self.service = service;
        self.location = location;
        self.refresh(false);
    }
    /// Per-widget domains start from its service's autocomplete defaults and never persist as options.
    pub fn domains(&self) -> crate::domains::Domains {
        if let Some(domains) = self.domains.get(&self.service.to_hex()) {
            return domains.clone();
        }
        let widgets: hydrus_store::tag_display_config::AutocompleteWidgetSettings =
            self.store.read(settings::get).unwrap_or_default();
        let options = widgets.options(&self.service);
        let mut location = if options.override_location {
            options.write_location
        } else {
            self.location.clone()
        };
        if location.is_all_known_files()
            && options.write_tag_service.as_bytes()
                == hydrus_core::service::builtin_keys::COMBINED_TAG
        {
            location = LocationContext::single(ServiceKey::new(
                hydrus_core::service::builtin_keys::HYDRUS_LOCAL_FILE_STORAGE.to_vec(),
            ));
        }
        crate::domains::Domains {
            location,
            tags: hydrus_core::search::context::TagContext {
                service: options.write_tag_service,
                include_current: true,
                include_pending: true,
                display_service: self.service.clone(),
            },
        }
    }
    pub fn choose_domain(&mut self, choice: crate::domains::Choice) {
        let mut domains = self.domains();
        match choice {
            crate::domains::Choice::Location(location) => {
                domains.choose_location(&self.store.snapshot().services, location);
            }
            crate::domains::Choice::Tags(key) => {
                let defaults: settings::SearchDefaults =
                    self.store.read(settings::get).unwrap_or_default();
                domains.choose_tags(key, &defaults.local_location);
            }
            crate::domains::Choice::Multiple => return,
        }
        self.domains.insert(self.service.to_hex(), domains);
        self.refresh(true);
    }
    pub fn domain_labels(&self) -> (String, String) {
        let domains = self.domains();
        let snapshot = self.store.snapshot();
        (
            crate::domains::location_label(&snapshot.services, &domains.location),
            crate::domains::tag_label(&snapshot.services, &domains.tags),
        )
    }
    pub fn tab(&self) -> Tab {
        self.tab
    }
    pub fn set_tab(&mut self, tab: Tab) {
        self.tab = tab;
        self.refresh(false);
    }
    pub fn set_context_tags(&mut self, tags: impl IntoIterator<Item = String>) {
        let context = tags.into_iter().collect();
        if self.context_tags != context {
            self.context_tags = context;
            if self.tab == Tab::Children {
                self.refresh(false);
            }
        }
    }
    pub fn text(&self) -> &str {
        &self.text
    }
    pub fn set_text(&mut self, text: &str) {
        text.clone_into(&mut self.text);
        self.refresh(false);
    }
    pub fn clear(&mut self) {
        self.set_text("");
    }
    pub fn rows(&self) -> &[Suggestion] {
        &self.rows
    }
    pub fn suggestions(&self) -> &[(String, String)] {
        &self.suggestions
    }
    fn primary_rows(&self) -> impl Iterator<Item = (usize, &Suggestion)> {
        self.rows
            .iter()
            .enumerate()
            .filter(|(_, row)| !row.parent_row)
    }
    fn logical_index(&self, physical: usize) -> Option<usize> {
        let row = self.rows.get(physical)?;
        self.primary_rows()
            .position(|(_, primary)| primary.tag == row.tag)
    }
    pub fn highlighted(&self) -> Option<usize> {
        let logical = self.selections[self.tab.index()].last?;
        self.primary_rows()
            .nth(logical)
            .map(|(physical, _)| physical)
    }
    pub fn selection_mask(&self) -> Vec<bool> {
        let selection = &self.selections[self.tab.index()].selected;
        self.rows
            .iter()
            .enumerate()
            .map(|(physical, _)| {
                self.logical_index(physical)
                    .is_some_and(|logical| selection.contains(&logical))
            })
            .collect()
    }
    pub fn selected_tags(&self) -> Vec<String> {
        let selected = &self.selections[self.tab.index()].selected;
        self.primary_rows()
            .enumerate()
            .filter(|(logical, _)| selected.contains(logical))
            .map(|(_, (_, row))| row.tag.clone())
            .collect()
    }
    /// Clicking never enters a tag; the owner activates the selected batch separately.
    pub fn click(&mut self, physical: usize, ctrl: bool, shift: bool) {
        if let Some(logical) = self.logical_index(physical) {
            self.selections[self.tab.index()].click(logical, ctrl, shift);
        }
    }
    /// Up/down wrap. Page keys move by physical rows, including expanded parents.
    /// The six directions are up, down, home, end, page up and page down.
    pub fn navigate(&mut self, direction: i32, ctrl: bool, shift: bool) {
        let count = self.primary_rows().count();
        let Some(last) = self.selections[self.tab.index()].last else {
            return;
        };
        if count <= 1 {
            return;
        }
        let target = match direction {
            0 => (last + count - 1) % count,
            1 => (last + 1) % count,
            2 => 0,
            3 => count - 1,
            4 | 5 => {
                let physical = self.highlighted().unwrap_or(0);
                let distance =
                    usize::try_from(self.options().autocomplete_list_height.clamp(1, 128))
                        .unwrap_or(11);
                let physical = if direction == 4 {
                    physical.saturating_sub(distance)
                } else {
                    physical.saturating_add(distance).min(self.rows.len() - 1)
                };
                self.logical_index(physical).unwrap_or(last)
            }
            _ => return,
        };
        self.selections[self.tab.index()].click(target, ctrl, shift);
    }
    pub fn move_highlight(&mut self, by: isize) {
        if by != 0 {
            self.navigate(i32::from(by > 0), false, false);
        }
    }
    /// Select-all retains the last hit and range anchor, just like the Qt list.
    pub fn select_all(&mut self) {
        self.selections[self.tab.index()].selected = (0..self.primary_rows().count()).collect();
    }
    pub fn deselect(&mut self) -> bool {
        let selection = &mut self.selections[self.tab.index()];
        let had_selection = !selection.selected.is_empty();
        selection.selected.clear();
        had_selection
    }
    /// Ctrl+C uses the whole result when nothing is selected. Parents appear once.
    pub fn copy_selection(&self, include_parents: bool) -> Option<String> {
        let selected = &self.selections[self.tab.index()].selected;
        let mut tags = Vec::new();
        for (logical, (_, row)) in self.primary_rows().enumerate() {
            if !selected.is_empty() && !selected.contains(&logical) {
                continue;
            }
            for tag in
                std::iter::once(&row.tag).chain(row.parents.iter().filter(|_| include_parents))
            {
                if !tags.contains(tag) {
                    tags.push(tag.clone());
                }
            }
        }
        (!tags.is_empty()).then(|| tags.join("\n"))
    }
    pub fn chosen(&self, index: Option<usize>) -> Option<String> {
        index
            .and_then(|i| self.rows.get(i))
            .map(|row| row.tag.clone())
            .or_else(|| self.selected_tags().into_iter().next())
            .or_else(|| Tag::new(&self.text).map(|tag| tag.as_str().to_owned()))
    }
    /// A double-click retains a selected batch, or selects the clicked new tag.
    pub fn chosen_tags(&mut self, index: Option<usize>) -> Vec<String> {
        if let Some(index) = index {
            if self.logical_index(index).is_none() {
                return Vec::new();
            }
            self.click(index, false, false);
        }
        let selected = self.selected_tags();
        if selected.is_empty() && self.rows.is_empty() {
            Tag::new(&self.text)
                .map(|tag| vec![tag.as_str().to_owned()])
                .unwrap_or_default()
        } else {
            selected
        }
    }
    pub fn fetch(&mut self) {
        self.refresh(true);
    }
    fn refresh(&mut self, manual: bool) {
        self.rows = self.search(manual).unwrap_or_default();
        let keys: Vec<_> = self
            .primary_rows()
            .map(|(_, row)| (row.tag.clone(), row.count))
            .collect();
        let tab = self.tab.index();
        if self.selection_rows[tab] != keys {
            let first = if self.options().select_first_with_count {
                self.primary_rows()
                    .position(|(_, row)| row.counted)
                    .unwrap_or(0)
            } else {
                0
            };
            self.selections[tab].reset((!keys.is_empty()).then_some(first));
            self.selection_rows[tab] = keys;
        }
        self.suggestions = self
            .rows
            .iter()
            .map(|r| (r.tag.clone(), r.label.clone()))
            .collect();
    }
    fn search(&self, manual: bool) -> Option<Vec<Suggestion>> {
        if self.tab == Tab::Tags && self.text.trim().is_empty() {
            return Some(Vec::new());
        }
        let input = AutocompleteInput::parse(&self.text);
        let snapshot = self.store.snapshot();
        let registry = &snapshot.services;
        let service = registry.by_key(&self.service).ok()?.id;
        let options = self
            .store
            .read(settings::get::<hydrus_store::tag_display_config::AutocompleteWidgetSettings>)
            .ok()?
            .options(&self.service);
        let domains = self.domains();
        let location = domains.location;
        let scope = TagSearchScope {
            domains: crate::autocomplete::count_domains(registry, &location),
            tag_service: registry
                .by_key(&domains.tags.service)
                .ok()
                .filter(|s| s.service_type() != ServiceType::CombinedTag)
                .map(|s| s.id),
            display: TagDisplayType::Storage,
            include_current: true,
            include_pending: true,
        };
        let mut matches = match self.tab {
            Tab::Tags => self
                .store
                .read(|conn| {
                    let rules = settings::get::<AutocompleteSettings>(conn)?.rules(&self.service);
                    let Some(query) = options.query(&input, &rules, manual) else {
                        return Ok(Vec::new());
                    };
                    autocomplete::search_tags_for_write(
                        conn,
                        registry,
                        &snapshot.display,
                        &scope,
                        &query,
                        service,
                    )
                })
                .ok()?,
            Tab::Favourites => {
                let favourites: settings::FavouriteTags = self.store.read(settings::get).ok()?;
                let mut tags = pasted_tags(&favourites.0.join("\n"));
                tags.sort();
                tags.into_iter()
                    .map(|tag| TagMatch {
                        tag,
                        count: CountRange::default(),
                    })
                    .collect()
            }
            Tab::Children => self
                .store
                .read(|conn| {
                    let sources: Vec<_> = scope.tag_service.map_or_else(
                        || registry.tag_services().map(|s| s.id).collect(),
                        |id| vec![id],
                    );
                    let tags: Vec<_> = self
                        .context_tags
                        .iter()
                        .filter_map(|t| Tag::new(t))
                        .collect();
                    let context = hydrus_store::master::tag_ids(conn, &tags)?;
                    let mut children = std::collections::BTreeSet::new();
                    for source in sources {
                        let graph = snapshot.display.get(source);
                        for id in context.values() {
                            children.extend(graph.descendants(*id).iter().copied());
                        }
                    }
                    children.retain(|id| !context.values().any(|c| c == id));
                    let mut display_scope = scope.clone();
                    display_scope.display = TagDisplayType::Display;
                    let mut children = autocomplete::count_tags(
                        conn,
                        registry,
                        &display_scope,
                        &children.into_iter().collect::<Vec<_>>(),
                    )?;
                    let options: settings::TagAutocompleteTabs = settings::get(conn)?;
                    if let Some(limit) = options.children_limit {
                        children.truncate(limit);
                    }
                    Ok(children
                        .into_iter()
                        .map(|m| TagMatch {
                            tag: m.tag,
                            count: CountRange::default(),
                        })
                        .collect())
                })
                .ok()?,
        };
        if self.tab == Tab::Tags {
            matches.sort_by_cached_key(|m| {
                (
                    std::cmp::Reverse(m.count.min_total()),
                    format!("{} {}", m.tag, m.count.suffix()),
                )
            });
        }
        let graph_service = if self.tab == Tab::Favourites {
            registry.by_key(&domains.tags.service).ok()?.id
        } else {
            service
        };
        let graph = snapshot.display.get(graph_service);
        let typed =
            if self.tab != Tab::Tags || self.text.contains('*') || self.text.starts_with("system:")
            {
                None
            } else {
                Tag::new(&self.text)
            };
        if let Some(typed) = &typed {
            let at = matches.iter().position(|m| m.tag == typed.as_str());
            let exact = at.map_or_else(
                || TagMatch {
                    tag: typed.as_str().to_owned(),
                    count: CountRange::default(),
                },
                |i| matches.remove(i),
            );
            matches.insert(0, exact);
            let ideal = self
                .store
                .read(|conn| {
                    let Some(id) = hydrus_store::master::tag_id(conn, typed)? else {
                        return Ok(None);
                    };
                    let ideal = graph.ideal(id);
                    Ok((ideal != id).then_some(ideal))
                })
                .ok()?;
            if let Some(ideal) = ideal {
                let ideal = self
                    .store
                    .read(|conn| hydrus_store::master::tags(conn, &[ideal]))
                    .ok()?
                    .into_values()
                    .next()?;
                let at = matches.iter().position(|m| m.tag == ideal.as_str());
                let exact = at.map_or_else(
                    || TagMatch {
                        tag: ideal.as_str().to_owned(),
                        count: CountRange::default(),
                    },
                    |i| matches.remove(i),
                );
                matches.insert(0, exact);
            }
        }
        let presentation: hydrus_core::tag_presentation::TagPresentation =
            self.store.read(settings::get).unwrap_or_default();
        let colours: hydrus_core::tag_presentation::NamespaceColours =
            self.store.read(settings::get).unwrap_or_default();
        let style: hydrus_core::tag_presentation::SiblingConnectorColours =
            self.store.read(settings::get).unwrap_or_default();
        let prefs = self.options();
        let mut rows = Vec::new();
        for m in matches {
            if self.tab == Tab::Children {
                rows.push(Suggestion {
                    colour_tag: m.tag.clone(),
                    label: presentation.render(&m.tag),
                    parts: Vec::new(),
                    tag: m.tag,
                    counted: false,
                    count: CountRange::default(),
                    parents: Vec::new(),
                    parent_row: false,
                });
                continue;
            }
            let tag = Tag::new(&m.tag)?;
            let (ideal, parents) = self
                .store
                .read(|conn| {
                    let Some(id) = hydrus_store::master::tag_id(conn, &tag)? else {
                        return Ok((None, Vec::new()));
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
                    parents.sort();
                    Ok((ideal, parents))
                })
                .ok()?;
            let mut label = presentation.render(&m.tag);
            let count = m.count.suffix();
            if !count.is_empty() {
                label.push(' ');
                label.push_str(&count);
            }
            let prefix = label.clone();
            let shown_ideal = ideal.filter(|_| prefs.autocomplete_show_siblings);
            if let Some(ideal) = &shown_ideal {
                label.push_str(&presentation.sibling_connector);
                label.push_str(&presentation.render(ideal));
            }
            if !parents.is_empty()
                && !prefs.autocomplete_expand_parents
                && prefs.autocomplete_show_parents
            {
                label.push_str(&format!(" ({} parents)", parents.len()));
            }
            let parts = if let Some(ideal) = shown_ideal.as_ref() {
                let ideal_text = presentation.render(ideal);
                let suffix = label
                    .strip_prefix(&format!(
                        "{prefix}{}{ideal_text}",
                        presentation.sibling_connector
                    ))
                    .unwrap_or_default()
                    .to_owned();
                style.runs(
                    (&m.tag, prefix),
                    &presentation.sibling_connector,
                    (ideal, ideal_text),
                    suffix,
                    &colours,
                    true,
                )
            } else {
                let suffix = label.strip_prefix(&prefix).unwrap_or_default().to_owned();
                style.parent_runs((&m.tag, prefix), suffix, &colours, true)
            };
            rows.push(Suggestion {
                tag: m.tag.clone(),
                colour_tag: m.tag.clone(),
                label,
                parts,
                counted: m.count.max_current > 0 || m.count.max_pending > 0,
                count: m.count,
                parents: parents.clone(),
                parent_row: false,
            });
            if prefs.autocomplete_expand_parents && prefs.autocomplete_show_parents {
                let all_parents = parents.clone();
                rows.extend(parents.into_iter().map(|parent| Suggestion {
                    tag: m.tag.clone(),
                    label: format!("    {}", presentation.render(&parent)),
                    parts: Vec::new(),
                    colour_tag: parent,
                    counted: m.count.max_current > 0 || m.count.max_pending > 0,
                    count: m.count,
                    parents: all_parents.clone(),
                    parent_row: true,
                }));
            }
        }
        Some(rows)
    }
}

/// A detached list of tags: Apply returns it to the caller; Cancel discards it.
#[derive(Debug)]
pub struct TagEntry {
    pub input: WriteAutocomplete,
    tags: std::collections::BTreeSet<String>,
    add_only: bool,
    additions: std::collections::BTreeSet<String>,
}
impl TagEntry {
    pub fn new(mut input: WriteAutocomplete, initial: &[String]) -> Self {
        input.set_context_tags(initial.iter().cloned());
        Self {
            input,
            add_only: false,
            additions: std::collections::BTreeSet::new(),
            tags: initial
                .iter()
                .filter_map(|t| Tag::new(t))
                .map(|t| t.as_str().to_owned())
                .collect(),
        }
    }
    /// The options favourite list adds choices and sorts its rows naturally.
    #[must_use]
    pub fn additions_only(mut self) -> Self {
        self.add_only = true;
        self
    }
    pub fn tags(&self) -> Vec<String> {
        let mut tags: Vec<_> = self.tags.iter().cloned().collect();
        if self.add_only {
            hydrus_core::sort::human_sort(&mut tags);
        }
        tags
    }
    /// Explicit additive choices, retained only while present in the final list.
    /// A selected-files owner uses these to distinguish re-entering an existing
    /// union tag from applying an unchanged heterogeneous selection.
    pub fn additions(&self) -> Vec<String> {
        self.additions.intersection(&self.tags).cloned().collect()
    }
    pub fn enter(&mut self, index: Option<usize>) {
        let tags = self.input.chosen_tags(index);
        if !tags.is_empty() {
            for tag in tags {
                if self.add_only || !self.tags.remove(&tag) {
                    self.additions.insert(tag.clone());
                    self.tags.insert(tag);
                }
            }
            self.input.set_context_tags(self.tags.iter().cloned());
            self.input.clear();
        }
    }
    pub fn paste(&mut self, tags: &[String]) {
        for tag in tags.iter().filter_map(|t| Tag::new(t)) {
            let tag = tag.as_str().to_owned();
            // Autocomplete paste is "only add": existing union members are
            // not an explicit choice to spread across the selected files.
            if self.tags.insert(tag.clone()) {
                self.additions.insert(tag);
            }
        }
        self.input.set_context_tags(self.tags.iter().cloned());
    }
    pub fn remove(&mut self, index: usize) {
        if let Some(tag) = self.tags().get(index).cloned() {
            self.tags.remove(&tag);
            self.additions.remove(&tag);
            self.input.set_context_tags(self.tags.iter().cloned());
        }
    }
}

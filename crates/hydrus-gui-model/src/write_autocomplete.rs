//! Shared write-tag suggestions, highlighted entry and multiline paste decisions.
use std::sync::Arc;

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

pub struct WriteAutocomplete {
    store: Arc<Store>,
    service: ServiceKey,
    location: LocationContext,
    text: String,
    rows: Vec<Suggestion>,
    suggestions: Vec<(String, String)>,
    highlighted: usize,
    tab: Tab,
    context_tags: std::collections::BTreeSet<String>,
    decorations: [[Option<bool>; 3]; 3],
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
            highlighted: 0,
            tab: Tab::Tags,
            context_tags: std::collections::BTreeSet::new(),
            decorations: [[None; 3]; 3],
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
    pub fn highlighted(&self) -> Option<usize> {
        (!self.rows.is_empty()).then_some(self.highlighted)
    }
    pub fn move_highlight(&mut self, by: isize) {
        if let Some(last) = self.rows.len().checked_sub(1) {
            self.highlighted = self.highlighted.saturating_add_signed(by).min(last);
        }
    }
    pub fn chosen(&self, index: Option<usize>) -> Option<String> {
        index
            .or_else(|| self.highlighted())
            .and_then(|i| self.rows.get(i))
            .map(|r| r.tag.clone())
            .or_else(|| Tag::new(&self.text).map(|t| t.as_str().to_owned()))
    }
    pub fn fetch(&mut self) {
        self.refresh(true);
    }
    fn refresh(&mut self, manual: bool) {
        self.rows = self.search(manual).unwrap_or_default();
        self.highlighted = if self.options().select_first_with_count {
            self.rows
                .iter()
                .position(|r| r.counted && !r.parent_row)
                .unwrap_or(0)
        } else {
            0
        };
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
        let location = if options.override_location {
            options.write_location.clone()
        } else {
            self.location.clone()
        };
        let location = if location.is_all_known_files()
            && options.write_tag_service.as_bytes()
                == hydrus_core::service::builtin_keys::COMBINED_TAG
        {
            LocationContext::single(ServiceKey::new(
                hydrus_core::service::builtin_keys::HYDRUS_LOCAL_FILE_STORAGE.to_vec(),
            ))
        } else {
            location
        };
        let scope = TagSearchScope {
            domains: crate::autocomplete::count_domains(registry, &location),
            tag_service: registry
                .by_key(&options.write_tag_service)
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
            registry.by_key(&options.write_tag_service).ok()?.id
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
        let prefs = self.options();
        let mut rows = Vec::new();
        for m in matches {
            if self.tab == Tab::Children {
                rows.push(Suggestion {
                    colour_tag: m.tag.clone(),
                    label: presentation.render(&m.tag),
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
            if prefs.autocomplete_show_siblings
                && let Some(ideal) = ideal
            {
                label.push_str(" → ");
                label.push_str(&presentation.render(&ideal));
            }
            if !parents.is_empty()
                && !prefs.autocomplete_expand_parents
                && prefs.autocomplete_show_parents
            {
                label.push_str(&format!(" ({} parents)", parents.len()));
            }
            rows.push(Suggestion {
                tag: m.tag.clone(),
                colour_tag: m.tag.clone(),
                label,
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
}
impl TagEntry {
    pub fn new(mut input: WriteAutocomplete, initial: &[String]) -> Self {
        input.set_context_tags(initial.iter().cloned());
        Self {
            input,
            tags: initial
                .iter()
                .filter_map(|t| Tag::new(t))
                .map(|t| t.as_str().to_owned())
                .collect(),
        }
    }
    pub fn tags(&self) -> Vec<String> {
        self.tags.iter().cloned().collect()
    }
    pub fn enter(&mut self, index: Option<usize>) {
        if let Some(tag) = self.input.chosen(index) {
            if !self.tags.remove(&tag) {
                self.tags.insert(tag);
            }
            self.input.set_context_tags(self.tags.iter().cloned());
            self.input.clear();
        }
    }
    pub fn paste(&mut self, tags: &[String]) {
        self.tags.extend(
            tags.iter()
                .filter_map(|t| Tag::new(t))
                .map(|t| t.as_str().to_owned()),
        );
        self.input.set_context_tags(self.tags.iter().cloned());
        self.input.clear();
    }
    pub fn remove(&mut self, index: usize) {
        if let Some(tag) = self.tags.iter().nth(index).cloned() {
            self.tags.remove(&tag);
            self.input.set_context_tags(self.tags.iter().cloned());
        }
    }
}

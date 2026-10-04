//! The manual export's read-only DISPLAY_ACTUAL all-known-tags sidebar.
use crate::{
    list_selection::ListSelection,
    write_tag_menu::{Action, Entry},
};
use hydrus_core::search::{
    context::{LocationContext, TagContext},
    predicate::Predicate,
};
use hydrus_core::{
    HashId, Tag, TagId,
    tag::split_tag,
    tag_presentation::{NamespaceColours, TagPresentation},
    tag_sort::{TagSort, sort_tags},
};
use hydrus_store::{Store, settings};
use std::{collections::BTreeSet, sync::Arc};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Row {
    pub id: TagId,
    pub tag: String,
    pub counts: [u64; 3],
    pub text: String,
    pub colour: [u8; 3],
}
impl Row {
    fn suffix(&self) -> String {
        let mut suffix = String::new();
        for (count, prefix) in self.counts.into_iter().zip(["", "+", "-"]) {
            if count > 0 {
                suffix.push_str(&format!(
                    " ({prefix}{})",
                    hydrus_core::numbers::human_int(count)
                ));
            }
        }
        suffix
    }
}

/// Local sort and selection are never written to the user's default tag sort.
pub struct Tags {
    store: Arc<Store>,
    pub sort: TagSort,
    rows: Vec<Row>,
    pub selection: ListSelection<TagId>,
    text_ascending: bool,
    count_ascending: bool,
}
impl Tags {
    pub fn new(store: Arc<Store>) -> hydrus_store::Result<Self> {
        let presentation: TagPresentation = store.read(settings::get)?;
        let sort = presentation.search_page_sort;
        Ok(Self {
            store,
            sort,
            rows: Vec::new(),
            selection: ListSelection::default(),
            text_ascending: if sort.sort_type == hydrus_core::tag_sort::TagSortType::Count {
                true
            } else {
                sort.ascending
            },
            count_ascending: sort.sort_type == hydrus_core::tag_sort::TagSortType::Count
                && sort.ascending,
        })
    }
    pub fn rows(&self) -> &[Row] {
        &self.rows
    }
    /// The caller supplies selected files, falling back to all kept files if none.
    pub fn refresh(&mut self, files: &[HashId]) -> hydrus_store::Result<()> {
        let snapshot = self.store.snapshot();
        let (counts, names, presentation, colours) = self.store.read(|conn| {
            let counts = hydrus_store::media::tag_counts(
                conn,
                &snapshot.services,
                &snapshot.display,
                None,
                files,
                &hydrus_store::tag_display::TagHider::default(),
            )?;
            let ids: Vec<_> = counts
                .current
                .keys()
                .chain(counts.pending.keys())
                .chain(counts.petitioned.keys())
                .copied()
                .collect();
            Ok((
                counts,
                hydrus_store::master::tags(conn, &ids)?,
                settings::get::<TagPresentation>(conn)?,
                settings::get::<NamespaceColours>(conn)?,
            ))
        })?;
        let mut rows: Vec<_> = names
            .into_iter()
            .map(|(id, tag)| {
                let count =
                    |map: &std::collections::HashMap<_, _>| map.get(&id).copied().unwrap_or(0);
                let mut row = Row {
                    id,
                    tag: tag.as_str().to_owned(),
                    counts: [
                        count(&counts.current),
                        count(&counts.pending),
                        count(&counts.petitioned),
                    ],
                    text: presentation.render(tag.as_str()),
                    colour: colours.tag(tag.as_str()),
                };
                let suffix = row.suffix();
                row.text.push_str(&suffix);
                row
            })
            .collect();
        sort_tags(
            &self.sort,
            &mut rows,
            |r| &r.tag,
            |r| r.counts.iter().sum(),
            &presentation.user_namespaces,
        );
        let removed: Vec<_> = self
            .rows
            .iter()
            .filter(|old| !rows.iter().any(|r| r.id == old.id))
            .map(|r| r.id)
            .collect();
        for id in removed {
            self.selection.forget(id);
        }
        self.rows = rows;
        Ok(())
    }
    pub fn click(&mut self, index: usize, ctrl: bool, shift: bool) {
        let order: Vec<_> = self.rows.iter().map(|r| r.id).collect();
        self.selection.click(&order, index, ctrl, shift);
    }
    pub fn select_all(&mut self) {
        self.selection
            .select_many(&self.rows.iter().map(|r| r.id).collect::<Vec<_>>());
    }
    pub fn selected(&self) -> Vec<&Row> {
        self.rows
            .iter()
            .filter(|r| self.selection.is_selected(r.id))
            .collect()
    }
    pub fn sort_chosen(&mut self, part: usize, index: usize) {
        if !matches!((part, index), (0 | 2, 0..=2) | (1, 0..=1)) {
            return;
        }
        self.sort = crate::options::tag_sort_chosen(&self.sort, part, index);
        if part == 0 {
            self.sort.ascending =
                if self.sort.sort_type == hydrus_core::tag_sort::TagSortType::Count {
                    self.count_ascending
                } else {
                    self.text_ascending
                };
        }
        if part == 1 {
            if self.sort.sort_type == hydrus_core::tag_sort::TagSortType::Count {
                self.count_ascending = self.sort.ascending;
            } else {
                self.text_ascending = self.sort.ascending;
            }
        }
    }
    /// Copy raw tags in display order, with the reference's subtag deduplication.
    pub fn copy(&self, all: bool, subtags: bool, counts: bool, underscores: bool) -> String {
        let rows = if all {
            self.rows.iter().collect()
        } else {
            self.selected()
        };
        let mut output = Vec::new();
        for row in rows {
            let mut text = if subtags {
                split_tag(&row.tag).1.to_owned()
            } else {
                row.tag.clone()
            };
            if counts {
                text.push_str(&row.suffix());
            }
            if underscores {
                text = text.replace(' ', "_");
            }
            if !text.is_empty() && ((counts && !underscores) || !output.contains(&text)) {
                output.push(text);
            }
        }
        output.join("\n")
    }
    pub fn launch(&self, or: bool) -> Option<Action> {
        let mut predicates: Vec<_> = self
            .selected()
            .into_iter()
            .map(|row| Predicate::Tag {
                tag: Tag::new(&row.tag).expect("stored tag"),
                inclusive: true,
            })
            .collect();
        if predicates.is_empty() {
            return None;
        }
        if or && predicates.len() > 1 {
            predicates = vec![Predicate::Or(predicates)];
        }
        Some(self.launch_action(predicates, false))
    }
    fn launch_action(&self, predicates: Vec<Predicate>, duplicate: bool) -> Action {
        let defaults: settings::SearchDefaults = self.store.read(settings::get).unwrap_or_default();
        Action::Launch {
            location: LocationContext::default(),
            context: TagContext::new(defaults.tag_service, true, true),
            predicates,
            duplicate,
        }
    }
    /// Normal display-list actions reuse the shared guarded page/relationship/
    /// favourite consumers; storage-only decorators and hide controls are absent.
    pub fn menu(&self) -> Vec<Entry> {
        if self.rows.is_empty() {
            return Vec::new();
        }
        let selected = self.selected();
        let selection = if selected.len() == 1 {
            selected[0].tag.clone()
        } else {
            format!(
                "{} selected",
                hydrus_core::numbers::human_int(selected.len() as u64)
            )
        };
        let item = |label: String, action| Entry::Item(label, action);
        let copy = |label: String, all, subtags, counts, underscores| {
            item(
                label,
                Action::Copy(self.copy(all, subtags, counts, underscores)),
            )
        };
        let mut copies = Vec::new();
        if !selected.is_empty() {
            copies.push(copy(selection.clone(), false, false, false, false));
            let tags: BTreeSet<_> = selected.iter().map(|r| r.tag.as_str()).collect();
            let subtags: BTreeSet<_> = selected.iter().map(|r| split_tag(&r.tag).1).collect();
            let sub_label = (tags != subtags).then(|| {
                if subtags.len() == 1 {
                    (*subtags.first().unwrap()).to_owned()
                } else {
                    format!("{} selected subtags", subtags.len())
                }
            });
            if let Some(label) = &sub_label {
                copies.push(copy(label.clone(), false, true, false, false));
            }
            let underscored: BTreeSet<_> = subtags.iter().map(|t| t.replace(' ', "_")).collect();
            if underscored
                .iter()
                .map(String::as_str)
                .collect::<BTreeSet<_>>()
                != subtags
            {
                copies.push(copy(
                    if underscored.len() == 1 {
                        underscored.first().unwrap().clone()
                    } else {
                        format!("{} selected subtags with underscores", underscored.len())
                    },
                    false,
                    true,
                    false,
                    true,
                ));
            }
            copies.push(Entry::Separator);
            copies.push(copy(
                format!("{selection} with counts"),
                false,
                false,
                true,
                false,
            ));
            if let Some(label) = sub_label {
                copies.push(copy(
                    format!("{label} with counts"),
                    false,
                    true,
                    true,
                    false,
                ));
            }
        }
        if selected.len() < self.rows.len() {
            if !copies.is_empty() {
                copies.push(Entry::Separator);
            }
            for (label, subtags, counts) in [
                ("all tags", false, false),
                ("all subtags", true, false),
                ("all tags with counts", false, true),
                ("all subtags with counts", true, true),
            ] {
                copies.push(copy(label.into(), true, subtags, counts, false));
            }
        }
        let mut entries = vec![Entry::Menu("copy".into(), copies)];
        if selected.len() == 1 {
            entries.extend(
                crate::write_tag_menu::relationship_entries(&self.store, &selected[0].tag)
                    .unwrap_or_default(),
            );
        } else if !selected.is_empty() {
            for (label, kind) in [
                ("siblings", hydrus_store::display::RelationKind::Siblings),
                ("parents", hydrus_store::display::RelationKind::Parents),
            ] {
                entries.push(Entry::Menu(
                    label.into(),
                    vec![item(
                        format!("add {label} to selection"),
                        Action::Relationship {
                            kind,
                            tags: selected.iter().map(|r| r.tag.clone()).collect(),
                        },
                    )],
                ));
            }
        }
        if let Some(and) = self.launch(false) {
            entries.push(Entry::Separator);
            let mut opens = vec![item(format!("open a new search page for {selection}"), and)];
            if selected.len() > 1 {
                opens.push(item(
                    format!("open a new OR search page for {selection}"),
                    self.launch(true).unwrap(),
                ));
                let defaults: settings::SearchDefaults =
                    self.store.read(settings::get).unwrap_or_default();
                opens.push(item(
                    "open new search pages for each in selection".into(),
                    Action::LaunchMany {
                        location: LocationContext::default(),
                        context: TagContext::new(defaults.tag_service, true, true),
                        pages: selected
                            .iter()
                            .map(|r| {
                                vec![Predicate::Tag {
                                    tag: Tag::new(&r.tag).expect("stored tag"),
                                    inclusive: true,
                                }]
                            })
                            .collect(),
                    },
                ));
            }
            opens.push(Entry::Separator);
            let predicates = selected
                .iter()
                .map(|r| Predicate::Tag {
                    tag: Tag::new(&r.tag).expect("stored tag"),
                    inclusive: true,
                })
                .collect();
            opens.push(item(
                format!("open a new duplicate filter page for {selection}"),
                self.launch_action(predicates, true),
            ));
            entries.push(Entry::Menu("open".into(), opens));
        }
        if selected.len() == 1 {
            let tag = &selected[0].tag;
            let favourites: settings::FavouriteTags =
                self.store.read(settings::get).unwrap_or_default();
            let remove = favourites.0.contains(tag);
            let mut favourites = vec![
                item(
                    format!(
                        "{} \"{tag}\" {} favourites",
                        if remove { "remove" } else { "add" },
                        if remove { "from" } else { "to" }
                    ),
                    Action::Favourite {
                        tag: tag.clone(),
                        service: None,
                        remove,
                        question: remove
                            .then(|| format!("Remove \"{tag}\" from the favourites list?")),
                    },
                ),
                Entry::Separator,
            ];
            let tabs: settings::TagAutocompleteTabs =
                self.store.read(settings::get).unwrap_or_default();
            for service in self.store.snapshot().services.tag_services() {
                let remove = tabs
                    .most_used
                    .get(&service.key.to_hex())
                    .is_some_and(|tags| tags.contains(tag));
                favourites.push(item(
                    format!(
                        "{} \"{tag}\" {} most used for \"{}\"",
                        if remove { "remove" } else { "add" },
                        if remove { "from" } else { "to" },
                        service.name
                    ),
                    Action::Favourite {
                        tag: tag.clone(),
                        service: Some(service.key.clone()),
                        remove,
                        question: remove.then(|| {
                            format!(
                                "Remove \"{tag}\" from the most used list for \"{}\"?",
                                service.name
                            )
                        }),
                    },
                ));
            }
            entries.push(Entry::Menu("favourites".into(), favourites));
        }
        entries
    }
}

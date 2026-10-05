//! Write-tag list copy, local decorations and immediate favourite settings actions.
use crate::write_autocomplete::{Suggestion, Tab, WriteAutocomplete};
use hydrus_core::{
    ServiceKey, Tag,
    search::{
        context::{LocationContext, TagContext},
        predicate::Predicate,
    },
    sort::human_sort,
    tag::split_tag,
};
use hydrus_store::{Store, settings};

pub const REGENERATE_QUESTION: &str = "!!WARNING EXPERIMENTAL!!\n\nThis will delete and then regenerate all the display calculations for the selected tags and their siblings and parents, with the intention of fixing bad autocomplete counts or sibling/parent presentation. It is functionally similar to the 'tag storage mappings cache' regeneration job, but just for these tags.\n\nIt might take a while to run, perhaps many minutes for a heavily-siblinged/-parented/-mapped tag, during which the database will be locked. Doing it on a thousand tags is going to completely gonk you. Also, any sibling or parent rules will be reset, and they will have to be recalculated, which will probably occur in a few seconds in the background after the regeneration job completes.";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Decoration {
    Parents,
    Expanded,
    Siblings,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    Copy(String),
    Regenerate {
        tags: Vec<String>,
    },
    Domain(crate::domains::Choice),
    Locations(LocationContext),
    Relationship {
        kind: hydrus_store::display::RelationKind,
        tags: Vec<String>,
    },
    Launch {
        location: LocationContext,
        context: TagContext,
        predicates: Vec<Predicate>,
        duplicate: bool,
    },
    LaunchMany {
        location: LocationContext,
        context: TagContext,
        pages: Vec<Vec<Predicate>>,
    },
    Decorate {
        tab: Tab,
        kind: Decoration,
        value: bool,
    },
    Favourite {
        tag: String,
        service: Option<ServiceKey>,
        remove: bool,
        question: Option<String>,
    },
}
impl Action {
    pub fn question(&self) -> Option<&str> {
        if matches!(self, Self::Regenerate { .. }) {
            return Some(REGENERATE_QUESTION);
        }
        if let Self::Favourite { question, .. } = self {
            question.as_deref()
        } else {
            None
        }
    }
    /// Re-read inside the write: other windows' favourites must survive a delayed answer.
    pub fn persist(&self, store: &Store) -> hydrus_store::Result<()> {
        if let Self::Regenerate { tags } = self {
            let tags: Vec<_> = tags.iter().filter_map(|tag| Tag::new(tag)).collect();
            return hydrus_store::maintenance::regenerate_tag_display(store, &tags);
        }
        let Self::Favourite {
            tag,
            service,
            remove,
            ..
        } = self
        else {
            return Ok(());
        };
        let tag = tag.clone();
        let service = service.clone();
        let remove = *remove;
        store.write(move |ctx| {
            let update = |tags: &mut Vec<String>| {
                tags.retain(|t| t != &tag);
                if !remove {
                    tags.push(tag.clone());
                }
                human_sort(tags);
            };
            if let Some(service) = service {
                let mut options: settings::TagAutocompleteTabs = settings::get(ctx.conn())?;
                update(options.most_used.entry(service.to_hex()).or_default());
                settings::set(ctx.conn(), &options)
            } else {
                let mut favourites: settings::FavouriteTags = settings::get(ctx.conn())?;
                update(&mut favourites.0);
                settings::set(ctx.conn(), &favourites)
            }
        })
    }
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Entry {
    Check(String, Action, bool),
    Item(String, Action),
    Menu(String, Vec<Entry>),
    Separator,
}
/// Immediate global and per-service favourite actions shared by read/write tag panes.
pub fn favourite_entries(store: &Store, tag: &str) -> Vec<Entry> {
    let favourites: settings::FavouriteTags = store.read(settings::get).unwrap_or_default();
    let remove = favourites.0.iter().any(|saved| saved == tag);
    let mut favourite_entries = vec![
        item(
            format!(
                "{} \"{tag}\" {} favourites",
                if remove { "remove" } else { "add" },
                if remove { "from" } else { "to" }
            ),
            Action::Favourite {
                tag: tag.to_owned(),
                service: None,
                remove,
                question: remove.then(|| format!("Remove \"{tag}\" from the favourites list?")),
            },
        ),
        Entry::Separator,
    ];
    let tabs: settings::TagAutocompleteTabs = store.read(settings::get).unwrap_or_default();
    for service in store.snapshot().services.tag_services() {
        let remove = tabs
            .most_used
            .get(&service.key.to_hex())
            .is_some_and(|tags| tags.iter().any(|saved| saved == tag));
        favourite_entries.push(item(
            format!(
                "{} \"{tag}\" {} most used for \"{}\"",
                if remove { "remove" } else { "add" },
                if remove { "from" } else { "to" },
                service.name
            ),
            Action::Favourite {
                tag: tag.to_owned(),
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
    favourite_entries
}

fn item(label: impl Into<String>, action: Action) -> Entry {
    Entry::Item(label.into(), action)
}
fn copy(label: impl Into<String>, text: impl Into<String>) -> Entry {
    item(label, Action::Copy(text.into()))
}
fn copies(rows: &[&Suggestion], subtags: bool, counts: bool) -> String {
    let mut texts = Vec::new();
    for row in rows {
        let tag = if subtags {
            split_tag(&row.tag).1
        } else {
            &row.tag
        };
        let count = if counts {
            row.count.suffix()
        } else {
            String::new()
        };
        let text = if count.is_empty() {
            tag.to_owned()
        } else {
            format!("{tag} {count}")
        };
        if counts || !texts.contains(&text) {
            texts.push(text);
        }
    }
    texts.join("\n")
}
impl WriteAutocomplete {
    pub fn domain_menu(&self, tags: bool) -> Vec<Entry> {
        let domains = self.domains();
        let snapshot = self.store().snapshot();
        let mut rows = if tags {
            crate::domains::tag_menu(&snapshot.services, &domains.tags)
        } else {
            let settings::AdvancedMode(advanced) =
                self.store().read(settings::get).unwrap_or_default();
            crate::domains::location_menu(&snapshot.services, advanced, &domains.location)
        };
        if !tags && !rows.iter().flatten().any(|row| {
            matches!(&row.choice, crate::domains::Choice::Location(location) if location.is_all_known_files())
        }) {
            let multiple = rows.pop();
            let location = LocationContext::single(ServiceKey::new(
                hydrus_core::service::builtin_keys::COMBINED_FILE.to_vec(),
            ));
            rows.push(Some(crate::domains::Row {
                label: "all known files with tags".into(),
                checked: location == domains.location,
                choice: crate::domains::Choice::Location(location),
            }));
            rows.push(None);
            if let Some(Some(mut multiple)) = multiple {
                multiple.checked = !rows.iter().flatten().any(|row| row.checked);
                rows.push(Some(multiple));
            }
        }
        rows.into_iter()
            .map(|row| {
                row.map_or(Entry::Separator, |row| {
                    let action = if row.choice == crate::domains::Choice::Multiple {
                        Action::Locations(domains.location.clone())
                    } else {
                        Action::Domain(row.choice)
                    };
                    Entry::Check(row.label, action, row.checked)
                })
            })
            .collect()
    }
    pub fn menu(&self, index: usize) -> Vec<Entry> {
        let Some(selected) = self.rows().get(index) else {
            return Vec::new();
        };
        let rows: Vec<_> = self.rows().iter().filter(|r| !r.parent_row).collect();
        let mut entries = Vec::new();
        let prefs = self.options();
        if self.tab() != Tab::Children {
            let mut decorate = |label: &str, kind, value| {
                entries.push(item(
                    label,
                    Action::Decorate {
                        tab: self.tab(),
                        kind,
                        value,
                    },
                ));
            };
            if prefs.autocomplete_show_parents {
                if !prefs.autocomplete_expand_parents {
                    decorate("expand parent rows", Decoration::Expanded, true);
                } else if self.rows().iter().any(|r| r.parent_row) {
                    decorate("collapse parent rows", Decoration::Expanded, false);
                }
                decorate("hide parent decorators", Decoration::Parents, false);
            } else {
                decorate("show parent decorators", Decoration::Parents, true);
            }
            if prefs.autocomplete_show_siblings {
                decorate("hide sibling decorators", Decoration::Siblings, false);
            } else {
                decorate("show sibling decorators", Decoration::Siblings, true);
            }
            entries.push(Entry::Separator);
        }
        let selected_tags = self.selected_tags();
        if selected_tags.len() > 1 && selected_tags.contains(&selected.tag) {
            let selected_rows: Vec<_> = rows
                .iter()
                .copied()
                .filter(|row| selected_tags.contains(&row.tag))
                .collect();
            entries.extend(batch_entries(
                self.store(),
                self.tab(),
                &rows,
                &selected_rows,
            ));
            return entries;
        }
        let subtag = split_tag(&selected.tag).1;
        let mut copy_entries = vec![copy(&selected.tag, &selected.tag)];
        if subtag != selected.tag {
            copy_entries.push(copy(subtag, subtag));
        }
        if subtag.contains(' ') {
            copy_entries.push(copy(subtag.replace(' ', "_"), subtag.replace(' ', "_")));
        }
        if self.tab() != Tab::Favourites {
            copy_entries.push(Entry::Separator);
            copy_entries.push(copy(
                format!("{} with counts", selected.tag),
                copies(&[selected], false, true),
            ));
            if subtag != selected.tag {
                copy_entries.push(copy(
                    format!("{subtag} with counts"),
                    copies(&[selected], true, true),
                ));
            }
        }
        if !selected.parents.is_empty() {
            copy_entries.push(Entry::Separator);
            let mut tags = vec![selected.tag.clone()];
            tags.extend(selected.parents.iter().cloned());
            copy_entries.push(copy(
                format!("{} and {} parents", selected.tag, selected.parents.len()),
                tags.join("\n"),
            ));
        }
        if rows.len() > 1 {
            copy_entries.push(Entry::Separator);
            copy_entries.push(copy("all tags", copies(&rows, false, false)));
            copy_entries.push(copy("all subtags", copies(&rows, true, false)));
            if self.tab() != Tab::Favourites {
                copy_entries.push(copy("all tags with counts", copies(&rows, false, true)));
                copy_entries.push(copy("all subtags with counts", copies(&rows, true, true)));
            }
        }
        entries.push(Entry::Menu("copy".into(), copy_entries));
        entries.extend(relationship_entries(self.store(), &selected.tag).unwrap_or_default());
        let tag = &selected.tag;
        let store = self.store();
        let defaults: settings::SearchDefaults = store.read(settings::get).unwrap_or_default();
        let predicates = vec![Predicate::Tag {
            tag: Tag::new(tag).expect("cleaned suggestion"),
            inclusive: true,
        }];
        let launch = |duplicate| Action::Launch {
            location: LocationContext::default(),
            context: TagContext::new(defaults.tag_service.clone(), true, true),
            predicates: predicates.clone(),
            duplicate,
        };
        let selection = if selected.parents.is_empty() {
            tag.clone()
        } else {
            format!("{tag} and {} parents", selected.parents.len())
        };
        entries.push(Entry::Menu(
            "open".into(),
            vec![
                item(
                    format!("open a new search page for {selection}"),
                    launch(false),
                ),
                Entry::Separator,
                item(
                    format!("open a new duplicate filter page for {selection}"),
                    launch(true),
                ),
            ],
        ));
        let favourite_entries = favourite_entries(store, &tag);
        entries.push(Entry::Menu("favourites".into(), favourite_entries));
        entries.push(maintenance_entry(vec![tag.clone()]));
        entries
    }
}

fn batch_entries(
    store: &Store,
    tab: Tab,
    rows: &[&Suggestion],
    selected: &[&Suggestion],
) -> Vec<Entry> {
    let label = format!("{} selected", selected.len());
    let subtags: Vec<_> = selected.iter().map(|row| split_tag(&row.tag).1).collect();
    let subtag_count = subtags
        .iter()
        .collect::<std::collections::BTreeSet<_>>()
        .len();
    let sublabel = if subtag_count == 1 {
        subtags[0].to_owned()
    } else {
        format!("{subtag_count} selected subtags")
    };
    let mut copy_entries = vec![copy(&label, copies(selected, false, false))];
    let has_namespace = selected.iter().any(|row| split_tag(&row.tag).1 != row.tag);
    if has_namespace {
        copy_entries.push(copy(&sublabel, copies(selected, true, false)));
    }
    if subtags.iter().any(|tag| tag.contains(' ')) {
        let mut underscores: Vec<_> = subtags.iter().map(|tag| tag.replace(' ', "_")).collect();
        let mut seen = std::collections::BTreeSet::new();
        underscores.retain(|tag| seen.insert(tag.clone()));
        let underscores_label = if underscores.len() == 1 {
            underscores[0].clone()
        } else {
            format!("{} selected subtags with underscores", underscores.len())
        };
        copy_entries.push(copy(underscores_label, underscores.join("\n")));
    }
    if tab != Tab::Favourites {
        copy_entries.push(Entry::Separator);
        copy_entries.push(copy(
            format!("{label} with counts"),
            copies(selected, false, true),
        ));
        if has_namespace {
            copy_entries.push(copy(
                format!("{sublabel} with counts"),
                copies(selected, true, true),
            ));
        }
    }
    let tags: Vec<_> = selected.iter().map(|row| row.tag.clone()).collect();
    let mut with_parents = Vec::new();
    for row in selected {
        for tag in std::iter::once(&row.tag).chain(&row.parents) {
            if !with_parents.contains(tag) {
                with_parents.push(tag.clone());
            }
        }
    }
    let num_parents = with_parents.len() - tags.len();
    let selection = if num_parents > 0 {
        copy_entries.push(Entry::Separator);
        let label = format!("{label} and {num_parents} parents");
        copy_entries.push(copy(&label, with_parents.join("\n")));
        label
    } else {
        label
    };
    if rows.len() > selected.len() {
        copy_entries.push(Entry::Separator);
        copy_entries.push(copy("all tags", copies(rows, false, false)));
        copy_entries.push(copy("all subtags", copies(rows, true, false)));
        if tab != Tab::Favourites {
            copy_entries.push(copy("all tags with counts", copies(rows, false, true)));
            copy_entries.push(copy("all subtags with counts", copies(rows, true, true)));
        }
    }
    let defaults: settings::SearchDefaults = store.read(settings::get).unwrap_or_default();
    let context = TagContext::new(defaults.tag_service, true, true);
    let predicates: Vec<_> = tags
        .iter()
        .map(|tag| Predicate::Tag {
            tag: Tag::new(tag).expect("cleaned suggestion"),
            inclusive: true,
        })
        .collect();
    let launch = |predicates, duplicate| Action::Launch {
        location: LocationContext::default(),
        context: context.clone(),
        predicates,
        duplicate,
    };
    vec![
        Entry::Menu("copy".into(), copy_entries),
        Entry::Menu(
            "siblings".into(),
            vec![item(
                "add siblings to selection",
                Action::Relationship {
                    kind: hydrus_store::display::RelationKind::Siblings,
                    tags: tags.clone(),
                },
            )],
        ),
        Entry::Menu(
            "parents".into(),
            vec![item(
                "add parents to selection",
                Action::Relationship {
                    kind: hydrus_store::display::RelationKind::Parents,
                    tags: tags.clone(),
                },
            )],
        ),
        Entry::Menu(
            "open".into(),
            vec![
                item(
                    format!("open a new search page for {selection}"),
                    launch(predicates.clone(), false),
                ),
                item(
                    format!("open a new OR search page for {selection}"),
                    launch(vec![Predicate::Or(predicates.clone())], false),
                ),
                item(
                    "open new search pages for each in selection",
                    Action::LaunchMany {
                        location: LocationContext::default(),
                        context: context.clone(),
                        pages: predicates
                            .iter()
                            .cloned()
                            .map(|predicate| vec![predicate])
                            .collect(),
                    },
                ),
                Entry::Separator,
                item(
                    format!("open a new duplicate filter page for {selection}"),
                    launch(predicates, true),
                ),
            ],
        ),
        maintenance_entry(tags),
    ]
}

fn maintenance_entry(tags: Vec<String>) -> Entry {
    Entry::Menu(
        "maintenance".into(),
        vec![item("regenerate tag display", Action::Regenerate { tags })],
    )
}

fn spam(entries: &mut Vec<Entry>, labels: &[(String, String)]) {
    let count = if labels.len() > 10 { 9 } else { labels.len() };
    entries.extend(
        labels
            .iter()
            .take(count)
            .map(|(label, text)| copy(label, text)),
    );
    if labels.len() > count {
        let label = format!("{} more...", labels.len() - count);
        entries.push(copy(&label, &label));
    }
}
fn sorted_tags(tags: &mut [String]) {
    use hydrus_core::tag_sort::{TagGroupBy, TagSort, TagSortType, sort_tags};
    sort_tags(
        &TagSort {
            sort_type: TagSortType::Tag,
            ascending: true,
            group_by: TagGroupBy::Nothing,
        },
        tags,
        String::as_str,
        |_| 0,
        &[],
    );
}
/// Shared display/storage tag lists use the actual relationship lookup and dialogs.
pub fn relationship_entries(store: &Store, tag: &str) -> hydrus_store::Result<Vec<Entry>> {
    use hydrus_store::display::RelationKind;
    use std::collections::{BTreeMap, BTreeSet};
    type Groups = BTreeMap<BTreeSet<usize>, (Vec<String>, Vec<String>)>;
    let snapshot = store.snapshot();
    let mut services: Vec<_> = [
        hydrus_core::ServiceType::LocalTag,
        hydrus_core::ServiceType::TagRepository,
    ]
    .into_iter()
    .flat_map(|ty| snapshot.services.of_type(ty))
    .collect();
    services.sort_by_cached_key(|s| {
        (
            usize::from(s.service_type() != hydrus_core::ServiceType::LocalTag),
            s.name.to_lowercase(),
        )
    });
    let selected = Tag::new(tag).expect("cleaned suggestion");
    let id = store.read(|conn| hydrus_store::master::tag_id(conn, &selected))?;
    let mut ideals: BTreeMap<String, BTreeSet<usize>> = BTreeMap::new();
    let mut siblings: BTreeMap<String, BTreeSet<usize>> = BTreeMap::new();
    let mut parents: BTreeMap<String, BTreeSet<usize>> = BTreeMap::new();
    let mut children: BTreeMap<String, BTreeSet<usize>> = BTreeMap::new();
    let mut all_siblings = BTreeSet::new();
    if let Some(id) = id {
        for (index, service) in services.iter().enumerate() {
            let graph = snapshot.display.get(service.id);
            let chain = graph.chain(id);
            let ancestors = graph.ancestors(id);
            let descendants = graph.descendants(id);
            let ids: Vec<_> = chain
                .iter()
                .chain(ancestors)
                .chain(descendants)
                .copied()
                .collect();
            let texts = store.read(|conn| hydrus_store::master::tags(conn, &ids))?;
            for sibling in &chain {
                let text = texts[sibling].as_str().to_owned();
                if text != tag {
                    all_siblings.insert(text.clone());
                }
                if *sibling == graph.ideal(id) {
                    ideals.entry(text).or_default().insert(index);
                } else if text != tag {
                    siblings.entry(text).or_default().insert(index);
                }
            }
            for parent in ancestors {
                parents
                    .entry(texts[parent].as_str().to_owned())
                    .or_default()
                    .insert(index);
            }
            for child in descendants {
                children
                    .entry(texts[child].as_str().to_owned())
                    .or_default()
                    .insert(index);
            }
        }
    }
    let names = |group: &BTreeSet<usize>| {
        if group.len() == services.len() {
            "all services".to_owned()
        } else {
            group
                .iter()
                .map(|i| services[*i].name.clone())
                .collect::<Vec<_>>()
                .join(", ")
        }
    };
    let mut sibling_entries = vec![item(
        format!("add siblings to {tag}"),
        Action::Relationship {
            kind: RelationKind::Siblings,
            tags: vec![tag.to_owned()],
        },
    )];
    for (ideal, group) in ideals {
        if ideal != tag {
            sibling_entries.push(copy(
                format!("ideal is \"{ideal}\" on: {}", names(&group)),
                ideal,
            ));
        }
    }
    let mut groups: Groups = BTreeMap::new();
    for (sibling, group) in siblings {
        groups.entry(group).or_default().0.push(sibling);
    }
    let mut sibling_groups: Vec<_> = groups.into_iter().collect();
    sibling_groups.sort_by_key(|(group, _)| (std::cmp::Reverse(group.len()), names(group)));
    for (group, (mut tags, _)) in sibling_groups {
        if group.len() != services.len() {
            let label = format!("--{}--", names(&group));
            sibling_entries.push(copy(&label, &label));
        }
        sorted_tags(&mut tags);
        spam(
            &mut sibling_entries,
            &tags
                .into_iter()
                .map(|tag| (tag.clone(), tag))
                .collect::<Vec<_>>(),
        );
    }
    let num_parents = parents.len();
    let num_children = children.len();
    let mut parent_entries = vec![item(
        format!("add parents to {tag}"),
        Action::Relationship {
            kind: RelationKind::Parents,
            tags: vec![tag.to_owned()],
        },
    )];
    let mut groups: Groups = BTreeMap::new();
    for (tag, group) in parents {
        groups.entry(group).or_default().0.push(tag);
    }
    for (tag, group) in children {
        groups.entry(group).or_default().1.push(tag);
    }
    let mut parent_groups: Vec<_> = groups.into_iter().collect();
    parent_groups.sort_by_key(|(group, _)| (std::cmp::Reverse(group.len()), names(group)));
    for (group, (mut parents, mut children)) in parent_groups {
        if group.len() != services.len() {
            let label = format!("--{}--", names(&group));
            parent_entries.push(copy(&label, &label));
        }
        sorted_tags(&mut parents);
        sorted_tags(&mut children);
        spam(
            &mut parent_entries,
            &parents
                .into_iter()
                .map(|tag| (format!("parent: {tag}"), tag))
                .collect::<Vec<_>>(),
        );
        spam(
            &mut parent_entries,
            &children
                .into_iter()
                .map(|tag| (format!("child: {tag}"), tag))
                .collect::<Vec<_>>(),
        );
    }
    Ok(vec![
        Entry::Menu(
            if all_siblings.is_empty() {
                "no siblings".into()
            } else {
                format!("{} siblings", all_siblings.len())
            },
            sibling_entries,
        ),
        Entry::Menu(
            if num_parents + num_children == 0 {
                "no parents".into()
            } else {
                format!("{num_parents} parents, {num_children} children")
            },
            parent_entries,
        ),
    ])
}

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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Decoration {
    Parents,
    Expanded,
    Siblings,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    Copy(String),
    Launch {
        location: LocationContext,
        context: TagContext,
        predicates: Vec<Predicate>,
        duplicate: bool,
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
        if let Self::Favourite { question, .. } = self {
            question.as_deref()
        } else {
            None
        }
    }
    /// Re-read inside the write: other windows' favourites must survive a delayed answer.
    pub fn persist(&self, store: &Store) -> hydrus_store::Result<()> {
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
    Item(String, Action),
    Menu(String, Vec<Entry>),
    Separator,
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
                ))
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
        let favourites: settings::FavouriteTags = store.read(settings::get).unwrap_or_default();
        let remove = favourites.0.contains(&selected.tag);
        let mut favourite_entries = vec![
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
                .is_some_and(|tags| tags.contains(tag));
            favourite_entries.push(item(
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
        entries.push(Entry::Menu("favourites".into(), favourite_entries));
        entries
    }
}

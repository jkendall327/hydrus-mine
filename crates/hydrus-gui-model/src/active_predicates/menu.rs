//! The active predicate list's whole right-click menu, as the reference
//! builds it for its `ListBoxTagsActiveSearchPredicates`
//! (`ListBoxTags.ShowMenuFromSignal`): the list's own copy, open and search
//! entries (done by the list, [`Action::Owner`]), with the tag list entries it
//! inherits for the selected tags: their siblings and parents, selecting files
//! with them (which does nothing on this list), favourites and maintenance.

use hydrus_core::sort::human_sort;
use hydrus_search::Predicate;
use hydrus_store::Store;

use super::Command;
use super::routes::{Copy, Open, Route};
use crate::write_tag_menu::{Action, Entry, favourite_entries, relationship_entries};

/// One of the list's own entries: its id and label.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Own {
    Route(Route, String),
    Search(Command, String),
}

/// The id of "select > files with ...", which does nothing on this list.
pub const SELECT_FILES: i32 = -1;

fn separator(entries: &mut Vec<Entry>) {
    if entries
        .last()
        .is_some_and(|last| !matches!(last, Entry::Separator))
    {
        entries.push(Entry::Separator);
    }
}

fn finished(mut entries: Vec<Entry>) -> Vec<Entry> {
    while matches!(entries.last(), Some(Entry::Separator)) {
        entries.pop();
    }
    entries
}

/// The section of its submenu an entry of the list's own is in; sections are
/// apart by separators.
fn section(own: &Own) -> (u8, u8) {
    match own {
        Own::Route(Route::Copy(copy), _) => (
            0,
            match copy {
                Copy::Selected | Copy::Subtags | Copy::Underscores => 0,
                Copy::Collapsed => 1,
                Copy::All | Copy::AllSubtags => 2,
            },
        ),
        Own::Route(Route::Open(open), _) => (1, u8::from(*open == Open::Duplicates)),
        Own::Search(command, _) => (
            2,
            match command {
                Command::Edit => 0,
                Command::Remove | Command::Invert | Command::InvertToggle => 1,
                Command::ReplaceOr | Command::StartOr | Command::DissolveOr => 2,
                Command::Namespace | Command::ExcludeNamespace => 3,
            },
        ),
    }
}

/// The submenu `menu` of the list's own entries, apart into their sections.
fn own_menu(own: &[Own], menu: u8) -> Vec<Entry> {
    let mut entries = Vec::new();
    let mut last = None;
    for entry in own.iter().filter(|o| section(o).0 == menu) {
        let at = section(entry).1;
        if last.is_some_and(|last| last != at) {
            separator(&mut entries);
        }
        last = Some(at);
        let (id, label) = match entry {
            Own::Route(route, label) => (route.id(), label),
            Own::Search(command, label) => (command.id(), label),
        };
        entries.push(Entry::Item(label.clone(), Action::Owner(id)));
    }
    finished(entries)
}

/// The selected tags' label for "select" (`SortNumericTags`, joined, and cut
/// to 64 characters with an ellipsis).
fn select_label(tags: &[String]) -> String {
    let mut shown = tags.to_vec();
    human_sort(&mut shown);
    let mut text = shown.join(", ");
    while text.chars().count() > 64 {
        if shown.len() == 1 {
            text = "(many/long tags)".into();
        } else {
            shown.pop();
            text = shown
                .iter()
                .cloned()
                .chain(std::iter::once("\u{2026}".to_owned()))
                .collect::<Vec<_>>()
                .join(", ");
        }
    }
    text
}

/// The whole menu for `selected` (the list's selection, captured as it
/// opens), given the list's own entries for it.
pub fn context_menu(store: &Store, selected: &[Predicate], own: &[Own]) -> Vec<Entry> {
    // (the tags of the selected tag predicates, `_GetTagsFromTerms`)
    let mut tags: Vec<String> = Vec::new();
    for predicate in selected {
        if let Predicate::Tag { tag, .. } = predicate
            && !tags.contains(&tag.as_str().to_owned())
        {
            tags.push(tag.as_str().to_owned());
        }
    }
    let mut entries = vec![Entry::Menu("copy".into(), own_menu(own, 0))];
    match tags.as_slice() {
        [] => {}
        [tag] => entries.extend(relationship_entries(store, tag).unwrap_or_default()),
        _ => {
            for (menu, label, kind) in [
                (
                    "siblings",
                    "add siblings to selection",
                    hydrus_store::display::RelationKind::Siblings,
                ),
                (
                    "parents",
                    "add parents to selection",
                    hydrus_store::display::RelationKind::Parents,
                ),
            ] {
                entries.push(Entry::Menu(
                    menu.into(),
                    vec![Entry::Item(
                        label.into(),
                        Action::Relationship {
                            kind,
                            tags: tags.clone(),
                        },
                    )],
                ));
            }
        }
    }
    if selected.is_empty() {
        return entries;
    }
    separator(&mut entries);
    let open = own_menu(own, 1);
    if !open.is_empty() {
        entries.push(Entry::Menu("open".into(), open));
    }
    let search = own_menu(own, 2);
    if !search.is_empty() {
        entries.push(Entry::Menu("search".into(), search));
    }
    if !tags.is_empty() {
        let shown = select_label(&tags);
        let mut select = vec![Entry::Item(
            if tags.len() == 1 {
                format!("files with \"{shown}\"")
            } else {
                format!("files with all of \"{shown}\"")
            },
            Action::Owner(SELECT_FILES),
        )];
        if tags.len() > 1 {
            select.push(Entry::Item(
                format!("files with any of \"{shown}\""),
                Action::Owner(SELECT_FILES),
            ));
        }
        entries.push(Entry::Menu("select".into(), select));
    }
    if let [tag] = tags.as_slice() {
        entries.push(Entry::Menu(
            "favourites".into(),
            favourite_entries(store, tag),
        ));
    }
    if !tags.is_empty() {
        separator(&mut entries);
        entries.push(Entry::Menu(
            "maintenance".into(),
            vec![Entry::Item(
                "regenerate tag display".into(),
                Action::Regenerate { tags },
            )],
        ));
    }
    finished(entries)
}

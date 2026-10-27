//! A media tag list, as the reference's `ListBoxTagsMedia` lists are: the
//! search page's "selection tags" box (`ListBoxTagsMediaSidebar`) and the
//! media viewer's tags hover (`ListBoxTagsMediaHoverFrame`).
//!
//! Its rows are selected by the reference's `ListBox` hit rules and keys
//! (shared with the suggestion lists, [`TagListBox`]): any mouse press hits
//! the row under it first, so a right or middle press acts on what it
//! selects. Its right-click menu (`ShowMenuFromSignal`) has the tag lists'
//! copy, siblings, parents, open, favourites and maintenance entries, and
//! those that depend on where the list is: the search page's "search" and
//! "select" submenus, "hide" (for the views that hide tags) and, in advanced
//! mode, the tag display switch. Those are the list's own entries
//! ([`Command`], as [`Action::Owner`] ids), done by the window that holds it.
use crate::tag_list_box::{Decorators, Key, Outcome, Row, TagListBox};
use crate::write_tag_menu::{Action, Entry, favourite_entries, relationship_entries};
use hydrus_core::numbers::human_int;
use hydrus_core::search::context::{LocationContext, TagContext};
use hydrus_core::search::predicate::Predicate;
use hydrus_core::sort::human_sort;
use hydrus_core::tag::split_tag;
use hydrus_core::tag_presentation::TagDisplayType;
use hydrus_core::{Tag, tag_filter::FilterRule};
use hydrus_search::{TextContext, predicate_text};
use hydrus_store::Store;
use hydrus_store::tag_display::{TagDisplayFilters, TagView};

/// One of the list's own menu entries, done by the window holding it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Command {
    /// "add ... to current search" (only those not in it).
    Add,
    /// "remove ... from current search" (only those in it).
    Remove,
    /// "invert: add ... to current search".
    Invert,
    /// "add an OR of ... to current search".
    AddOr,
    /// "replace ... with their OR".
    ReplaceOr,
    /// "start an OR predicate with ...": the OR editor, then as `ReplaceOr`.
    StartOr,
    /// "dissolve ... into single predicates".
    DissolveOr,
    /// "add <namespace> to current search".
    Namespace,
    /// "exclude <namespace> from the current search".
    ExcludeNamespace,
    /// "select > files with (all of) ...".
    SelectAll,
    /// "select > files with any of ...".
    SelectAny,
    /// "hide > ... from here" (the tags).
    HideTags,
    /// "hide > ... tags from here" (their namespaces).
    HideNamespaces,
    /// "tag display (experimental) > switch to ...".
    Display(TagDisplayType),
}

impl Command {
    const PLAIN: [Self; 13] = [
        Self::Add,
        Self::Remove,
        Self::Invert,
        Self::AddOr,
        Self::ReplaceOr,
        Self::StartOr,
        Self::DissolveOr,
        Self::Namespace,
        Self::ExcludeNamespace,
        Self::SelectAll,
        Self::SelectAny,
        Self::HideTags,
        Self::HideNamespaces,
    ];

    /// Its menu id.
    pub fn id(self) -> i32 {
        match self {
            Self::Display(t) => 100 + i32::try_from(t.code()).unwrap_or(0),
            _ => Self::PLAIN
                .iter()
                .position(|c| *c == self)
                .and_then(|i| i32::try_from(i).ok())
                .unwrap_or(-1),
        }
    }

    /// The command with this menu id.
    pub fn from_id(id: i32) -> Option<Self> {
        if id >= 100 {
            return TagDisplayType::from_code(i64::from(id - 100)).map(Self::Display);
        }
        usize::try_from(id)
            .ok()
            .and_then(|i| Self::PLAIN.get(i).copied())
    }
}

/// What a copy entry copies of each tag.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Form {
    Tags,
    Subtags,
    /// Subtags with their spaces as underscores.
    Underscores,
    /// As drawn, with the counts.
    TagsWithCounts,
    SubtagsWithCounts,
}

/// Where the list is, which decides its menu.
#[derive(Debug, Clone)]
pub struct Place<'a> {
    /// Its rows show counts (the selection box's do, the hover's don't).
    pub counts: bool,
    /// The page's search, when the list can add to it (a search page's box).
    pub search: Option<&'a [Predicate]>,
    /// The list can select its page's files (a page's box).
    pub select: bool,
    /// The tags it shows.
    pub display: TagDisplayType,
    /// Advanced mode (the tag display switch).
    pub advanced: bool,
    /// Where pages it opens search.
    pub location: LocationContext,
    pub tags: TagContext,
}

/// The list: its rows (tag, and the text drawn) and what is selected.
#[derive(Debug, Clone)]
pub struct SelectionTags {
    list: TagListBox,
    texts: Vec<String>,
}

impl Default for SelectionTags {
    fn default() -> Self {
        Self {
            list: TagListBox::new(Decorators {
                show_parents: false,
                expand_parents: false,
                show_siblings: false,
            }),
            texts: Vec::new(),
        }
    }
}

impl SelectionTags {
    /// New rows, (tag, text drawn): the tags still listed stay selected
    /// (`_UpdateTerms`).
    pub fn set_rows(&mut self, rows: &[(String, String)]) {
        self.list.replace_rows(
            rows.iter()
                .map(|(tag, _)| Row {
                    tag: tag.clone(),
                    ..Row::default()
                })
                .collect(),
        );
        self.texts = rows.iter().map(|(_, text)| text.clone()).collect();
    }

    pub fn tags(&self) -> Vec<String> {
        self.list.tags()
    }

    /// The selected tags, in the list's order.
    pub fn selected_tags(&self) -> Vec<String> {
        self.list.selected_tags()
    }

    /// Whether each row is drawn selected.
    pub fn mask(&self) -> Vec<bool> {
        self.list.mask()
    }

    /// A press of any button on a row, or on nothing (`_HandleClick`).
    pub fn press(&mut self, row: Option<usize>, ctrl: bool, shift: bool) {
        self.list.click(row, ctrl, shift);
    }

    /// A ctrl double-click on a row the first click deselected selects it
    /// again (`eventFilter`'s "fudge").
    pub fn reselect_for_double_click(&mut self, row: usize) {
        if self.list.mask().get(row) == Some(&false) {
            self.list.click(Some(row), true, false);
        }
    }

    /// A key (`keyPressEvent`); `page` is how many rows a page holds.
    pub fn key(&mut self, key: Key, ctrl: bool, shift: bool, page: usize) -> Outcome {
        self.list.key(key, ctrl, shift, page)
    }

    /// The selected tags' search predicates (`_GetPredicatesFromTerms`).
    pub fn predicates(&self) -> Vec<Predicate> {
        self.selected_tags()
            .iter()
            .filter_map(|t| Tag::new(t))
            .map(|tag| Predicate::Tag {
                tag,
                inclusive: true,
            })
            .collect()
    }

    /// What a double-click (or enter) puts in the search
    /// (`ListBoxTagsMediaSidebar._Activate`): the selection, inverted with
    /// ctrl, as one OR with shift when there are several.
    pub fn activation(&self, ctrl: bool, shift: bool) -> Vec<Predicate> {
        let mut predicates = self.predicates();
        if ctrl {
            predicates = predicates
                .into_iter()
                .filter_map(|p| p.inverse(&|_| false))
                .collect();
        }
        if shift && predicates.len() > 1 {
            predicates = vec![Predicate::Or(predicates)];
        }
        predicates
    }

    /// The page a middle-click opens: the selection, or with shift their OR
    /// (when there are several).
    pub fn middle_click(&self, shift: bool) -> Vec<Predicate> {
        let predicates = self.predicates();
        match or(&predicates) {
            Some(or) if shift => vec![or],
            _ => predicates,
        }
    }

    /// The copyable strings (`_GetCopyableTagStrings`) of the selection
    /// (or of all) in `form`.
    fn strings(&self, selected: bool, form: Form) -> Vec<String> {
        let (counts, subtags, underscores) = match form {
            Form::Tags => (false, false, false),
            Form::Subtags => (false, true, false),
            Form::Underscores => (false, true, true),
            Form::TagsWithCounts => (true, false, false),
            Form::SubtagsWithCounts => (true, true, false),
        };
        let mask = self.list.mask();
        let tags = self.list.tags();
        let mut out: Vec<String> = Vec::new();
        for (i, tag) in tags.iter().enumerate() {
            if selected && !mask[i] {
                continue;
            }
            let text = if counts { &self.texts[i] } else { tag };
            let mut text = if subtags {
                split_tag(text).1.to_owned()
            } else {
                text.clone()
            };
            if text.is_empty() {
                continue;
            }
            if underscores {
                text = text.replace(' ', "_");
            }
            // (with counts, nothing is deduplicated)
            if counts || !out.contains(&text) {
                out.push(text);
            }
        }
        out
    }

    /// The right-click menu for the selection as it is (a right press hits
    /// its row first), for a list in `place`.
    #[allow(clippy::too_many_lines)] // (one menu, in the reference's order)
    pub fn menu(&self, store: &Store, place: &Place<'_>, text: &TextContext) -> Vec<Entry> {
        let mut entries = Vec::new();
        if self.list.rows().is_empty() {
            return entries;
        }
        let selected = self.selected_tags();
        let mut copy = Vec::new();
        let tags = self.strings(true, Form::Tags);
        let mut selection = String::new();
        if !tags.is_empty() {
            let subtags = self.strings(true, Form::Subtags);
            let underscores = self.strings(true, Form::Underscores);
            let set = |v: &[String]| v.iter().cloned().collect::<std::collections::BTreeSet<_>>();
            selection = if tags.len() == 1 {
                tags[0].clone()
            } else {
                format!("{} selected", human_int(tags.len() as u64))
            };
            copy.push(Entry::Item(
                selection.clone(),
                Action::Copy(tags.join("\n")),
            ));
            let mut sub_selection = None;
            if !subtags.is_empty() && set(&subtags) != set(&tags) {
                let label = if subtags.len() == 1 {
                    subtags[0].clone()
                } else {
                    format!("{} selected subtags", human_int(subtags.len() as u64))
                };
                copy.push(Entry::Item(label.clone(), Action::Copy(subtags.join("\n"))));
                sub_selection = Some(label);
            }
            if !underscores.is_empty() && set(&underscores) != set(&subtags) {
                let label = if underscores.len() == 1 {
                    underscores[0].clone()
                } else {
                    format!(
                        "{} selected subtags with underscores",
                        human_int(underscores.len() as u64)
                    )
                };
                copy.push(Entry::Item(label, Action::Copy(underscores.join("\n"))));
            }
            if place.counts {
                separator(&mut copy);
                copy.push(Entry::Item(
                    format!("{selection} with counts"),
                    Action::Copy(self.strings(true, Form::TagsWithCounts).join("\n")),
                ));
                if let Some(sub) = sub_selection {
                    copy.push(Entry::Item(
                        format!("{sub} with counts"),
                        Action::Copy(self.strings(true, Form::SubtagsWithCounts).join("\n")),
                    ));
                }
            }
        }
        if self.list.rows().len() > selected.len() {
            separator(&mut copy);
            copy.push(Entry::Item(
                "all tags".into(),
                Action::Copy(self.strings(false, Form::Tags).join("\n")),
            ));
            copy.push(Entry::Item(
                "all subtags".into(),
                Action::Copy(self.strings(false, Form::Subtags).join("\n")),
            ));
            if place.counts {
                copy.push(Entry::Item(
                    "all tags with counts".into(),
                    Action::Copy(self.strings(false, Form::TagsWithCounts).join("\n")),
                ));
                copy.push(Entry::Item(
                    "all subtags with counts".into(),
                    Action::Copy(self.strings(false, Form::SubtagsWithCounts).join("\n")),
                ));
            }
        }
        entries.push(Entry::Menu("copy".into(), copy));
        match selected.as_slice() {
            [] => {}
            [one] => entries.extend(relationship_entries(store, one).unwrap_or_default()),
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
                                tags: selected.clone(),
                            },
                        )],
                    ));
                }
            }
        }
        if !selected.is_empty() {
            separator(&mut entries);
            let predicates = self.predicates();
            let launch = |predicates: Vec<Predicate>, duplicate| Action::Launch {
                location: place.location.clone(),
                context: place.tags.clone(),
                predicates,
                duplicate,
            };
            let mut open = vec![Entry::Item(
                format!("open a new search page for {selection}"),
                launch(predicates.clone(), false),
            )];
            if let Some(or) = or(&predicates) {
                open.push(Entry::Item(
                    format!("open a new OR search page for {selection}"),
                    launch(vec![or], false),
                ));
            }
            if predicates.len() > 1 {
                open.push(Entry::Item(
                    "open new search pages for each in selection".into(),
                    Action::LaunchMany {
                        location: place.location.clone(),
                        context: place.tags.clone(),
                        pages: predicates.iter().cloned().map(|p| vec![p]).collect(),
                    },
                ));
            }
            separator(&mut open);
            open.push(Entry::Item(
                format!("open a new duplicate filter page for {selection}"),
                launch(predicates.clone(), true),
            ));
            entries.push(Entry::Menu("open".into(), open));
            if let Some(current) = place.search {
                let search = search_entries(&predicates, current, text);
                if !search.is_empty() {
                    entries.push(Entry::Menu("search".into(), search));
                }
            }
            if place.select {
                let shown = select_label(&selected);
                let mut select = vec![own(
                    if selected.len() == 1 {
                        format!("files with \"{shown}\"")
                    } else {
                        format!("files with all of \"{shown}\"")
                    },
                    Command::SelectAll,
                )];
                if selected.len() > 1 {
                    select.push(own(
                        format!("files with any of \"{shown}\""),
                        Command::SelectAny,
                    ));
                }
                entries.push(Entry::Menu("select".into(), select));
            }
            if matches!(
                place.display,
                TagDisplayType::SingleMedia | TagDisplayType::SelectionList
            ) {
                separator(&mut entries);
                let namespaces = namespaces(&selected);
                let namespace_label = if let [one] = namespaces.as_slice() {
                    format!("\"{}\" tags from here", render_namespace(one))
                } else {
                    format!(
                        "{} selected namespaces from here",
                        human_int(namespaces.len() as u64)
                    )
                };
                let tag_label = if let [one] = selected.as_slice() {
                    format!("\"{one}\" from here")
                } else {
                    format!(
                        "{} selected tags from here",
                        human_int(selected.len() as u64)
                    )
                };
                entries.push(Entry::Menu(
                    "hide".into(),
                    vec![
                        asked(
                            namespace_label,
                            Command::HideNamespaces,
                            hide_question(&selected, true).0,
                        ),
                        asked(
                            tag_label,
                            Command::HideTags,
                            hide_question(&selected, false).0,
                        ),
                    ],
                ));
            }
            if let [one] = selected.as_slice() {
                entries.push(Entry::Menu(
                    "favourites".into(),
                    favourite_entries(store, one),
                ));
            }
        }
        if place.advanced {
            let switches = [
                TagDisplayType::SelectionList,
                TagDisplayType::SingleMedia,
                TagDisplayType::Display,
                TagDisplayType::Storage,
            ]
            .into_iter()
            .map(|t| {
                Entry::Check(
                    format!("switch to \"{}\"", display_label(t)),
                    Action::Owner(Command::Display(t).id()),
                    t == place.display,
                )
            })
            .collect();
            entries.push(Entry::Menu("tag display (experimental)".into(), switches));
        }
        if !selected.is_empty() {
            separator(&mut entries);
            entries.push(Entry::Menu(
                "maintenance".into(),
                vec![Entry::Item(
                    "regenerate tag display".into(),
                    Action::Regenerate { tags: selected },
                )],
            ));
        }
        finished(entries)
    }
}

/// The reference's names for the tag display types (`tag_display_str_lookup`).
fn display_label(t: TagDisplayType) -> &'static str {
    match t {
        TagDisplayType::Storage => "stored tags",
        TagDisplayType::Display => "display tags",
        TagDisplayType::SingleMedia => "single media view tags",
        TagDisplayType::SelectionList => "multiple media view tags",
    }
}

fn own(label: String, command: Command) -> Entry {
    Entry::Item(label, Action::Owner(command.id()))
}

fn asked(label: String, command: Command, question: String) -> Entry {
    Entry::Item(
        label,
        Action::Asked {
            id: command.id(),
            question,
        },
    )
}

/// `AppendSeparator`: never first, never twice.
fn separator(entries: &mut Vec<Entry>) {
    if !entries.is_empty() && !matches!(entries.last(), Some(Entry::Separator)) {
        entries.push(Entry::Separator);
    }
}

/// A menu as shown: no separator last, in it or in its submenus.
fn finished(mut entries: Vec<Entry>) -> Vec<Entry> {
    while matches!(entries.last(), Some(Entry::Separator)) {
        entries.pop();
    }
    entries
        .into_iter()
        .map(|entry| match entry {
            Entry::Menu(label, children) => Entry::Menu(label, finished(children)),
            other => other,
        })
        .collect()
}

/// The OR of several predicates, none of them an OR
/// (`_GetSelectedPredicatesAndInverseCopies`).
fn or(predicates: &[Predicate]) -> Option<Predicate> {
    (predicates.len() > 1 && !predicates.iter().any(|p| matches!(p, Predicate::Or(_))))
        .then(|| Predicate::Or(predicates.to_vec()))
}

/// The namespace of tag predicates all of one namespace.
fn namespace(predicates: &[Predicate]) -> Option<Predicate> {
    let mut namespaces = predicates.iter().map(|p| match p {
        Predicate::Tag { tag, .. } => Some(tag.namespace().to_owned()),
        _ => None,
    });
    let first = namespaces.next()??;
    namespaces
        .all(|n| n.as_deref() == Some(first.as_str()))
        .then_some(Predicate::Namespace {
            namespace: first,
            inclusive: true,
        })
}

/// The search submenu, when the list can add to a search
/// (`ShowMenuFromSignal` with `_CanProvideCurrentPagePredicates`).
fn search_entries(
    predicates: &[Predicate],
    current: &[Predicate],
    text: &TextContext,
) -> Vec<Entry> {
    let mut out = Vec::new();
    let description = if let [one] = predicates {
        predicate_text(one, text)
    } else {
        "selected".into()
    };
    if predicates.iter().any(|p| !current.contains(p)) {
        out.push(own(
            format!("add {description} to current search"),
            Command::Add,
        ));
    }
    if predicates.iter().any(|p| current.contains(p)) {
        out.push(own(
            format!("remove {description} from current search"),
            Command::Remove,
        ));
    }
    let inverses: Vec<Predicate> = predicates
        .iter()
        .filter_map(|p| p.inverse(&|_| false))
        .collect();
    if !inverses.is_empty() {
        out.push(own(
            if let [one] = inverses.as_slice() {
                format!(
                    "invert: add {} to current search",
                    predicate_text(one, text)
                )
            } else {
                "invert selected and add to current search".into()
            },
            Command::Invert,
        ));
    }
    separator(&mut out);
    if let Some(or) = or(predicates)
        && !predicates.contains(&or)
    {
        if predicates.iter().all(|p| current.contains(p)) {
            out.push(own(
                format!("replace {description} with their OR"),
                Command::ReplaceOr,
            ));
        } else {
            out.push(own(
                format!("add an OR of {description} to current search"),
                Command::AddOr,
            ));
        }
    }
    if !predicates.iter().any(|p| matches!(p, Predicate::Or(_))) {
        out.push(own(
            format!("start an OR predicate with {description}"),
            Command::StartOr,
        ));
    }
    if predicates.iter().all(|p| matches!(p, Predicate::Or(_))) {
        out.push(own(
            format!("dissolve {description} into single predicates"),
            Command::DissolveOr,
        ));
    }
    separator(&mut out);
    if let Some(namespace) = namespace(predicates) {
        let label = predicate_text(&namespace, text);
        if !current.contains(&namespace) {
            out.push(own(
                format!("add {label} to current search"),
                Command::Namespace,
            ));
        }
        if let Some(inverse) = namespace.inverse(&|_| false)
            && !current.contains(&inverse)
        {
            out.push(own(
                format!("exclude {label} from the current search"),
                Command::ExcludeNamespace,
            ));
        }
    }
    out
}

/// The selection's namespaces, sorted.
fn namespaces(tags: &[String]) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for tag in tags {
        let namespace = split_tag(tag).0.to_owned();
        if !out.contains(&namespace) {
            out.push(namespace);
        }
    }
    human_sort(&mut out);
    out
}

/// `RenderNamespaceForUser`.
fn render_namespace(namespace: &str) -> &str {
    match namespace {
        "" => "unnamespaced",
        ":" => "namespaced",
        n => n,
    }
}

/// The selected tags' label for "select": sorted, joined, and cut to 64
/// characters with an ellipsis.
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

/// The predicates entered and how (`_EmitPredicates`): those that may be
/// added, those that may be removed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entered {
    pub predicates: Vec<Predicate>,
    pub add: bool,
    pub remove: bool,
}

/// What a search command enters into the page's search, in order
/// (`ListBoxTagsMediaSidebar._ProcessMenuPredicateEvent`); `StartOr` gives
/// what follows the OR editor's OK, with the OR as it was started.
pub fn search_command(command: Command, selected: &[Predicate]) -> Vec<Entered> {
    let entered = |predicates: Vec<Predicate>, add, remove| Entered {
        predicates,
        add,
        remove,
    };
    let inverses = || {
        selected
            .iter()
            .filter_map(|p| p.inverse(&|_| false))
            .collect::<Vec<_>>()
    };
    match command {
        Command::Add => vec![entered(selected.to_vec(), true, false)],
        Command::Remove => vec![entered(selected.to_vec(), false, true)],
        Command::Invert => vec![entered(inverses(), true, false)],
        Command::AddOr => or(selected)
            .map(|or| vec![entered(vec![or], true, false)])
            .unwrap_or_default(),
        Command::ReplaceOr | Command::StartOr => or(selected)
            .or_else(|| (command == Command::StartOr).then(|| Predicate::Or(selected.to_vec())))
            .map(|or| {
                vec![
                    entered(selected.to_vec(), false, true),
                    entered(vec![or], true, false),
                ]
            })
            .unwrap_or_default(),
        Command::DissolveOr => {
            let ors: Vec<Predicate> = selected
                .iter()
                .filter(|p| matches!(p, Predicate::Or(_)))
                .cloned()
                .collect();
            let mut parts: Vec<Predicate> = Vec::new();
            for p in &ors {
                if let Predicate::Or(terms) = p {
                    for term in terms {
                        if !parts.contains(term) {
                            parts.push(term.clone());
                        }
                    }
                }
            }
            vec![entered(ors, false, true), entered(parts, true, false)]
        }
        Command::Namespace => namespace(selected)
            .map(|n| vec![entered(vec![n], true, false)])
            .unwrap_or_default(),
        Command::ExcludeNamespace => namespace(selected)
            .and_then(|n| n.inverse(&|_| false))
            .map(|n| vec![entered(vec![n], true, false)])
            .unwrap_or_default(),
        _ => Vec::new(),
    }
}

/// Enter predicates into a search (`_EnterPredicates`): each one there is
/// taken out if removing is allowed, each one not there put in (and what it
/// excludes taken out) if adding is.
pub fn enter(search: &mut Vec<Predicate>, entered: &Entered, text: &TextContext) {
    let wanted: Vec<Predicate> = entered
        .predicates
        .iter()
        .filter(|p| {
            if search.contains(p) {
                entered.remove
            } else {
                entered.add
            }
        })
        .cloned()
        .collect();
    hydrus_search::enter_predicates(search, &wanted, text);
}

/// The question "hide" asks, and the slices it hides once answered yes
/// (`_ProcessMenuTagEvent`).
pub fn hide_question(tags: &[String], namespaces_too: bool) -> (String, Vec<String>) {
    if namespaces_too {
        let namespaces = namespaces(tags);
        let shown: Vec<String> = namespaces
            .iter()
            .map(|n| render_namespace(n).to_owned())
            .collect();
        let slices = namespaces
            .iter()
            .map(|n| {
                if n.is_empty() {
                    String::new()
                } else {
                    format!("{n}:")
                }
            })
            .collect();
        (format!("Hide{}tags from here?", summary(&shown)), slices)
    } else {
        (format!("Hide{}from here?", summary(tags)), tags.to_vec())
    }
}

/// Hide tag slices from a view of every tag service's tags (`HideTags` on
/// "all known tags").
pub fn hide(store: &Store, view: TagView, slices: &[String]) -> hydrus_store::Result<()> {
    let slices = slices.to_vec();
    store.write_and_refresh(move |ctx| {
        let mut filters: TagDisplayFilters = hydrus_store::settings::get(ctx.conn())?;
        let key =
            hydrus_core::ServiceKey::new(hydrus_core::service::builtin_keys::COMBINED_TAG.to_vec())
                .to_hex();
        let filter = filters.for_view_mut(view).entry(key).or_default();
        for slice in slices {
            filter.set_rule(slice, FilterRule::Blacklist);
        }
        hydrus_store::settings::set(ctx.conn(), &filters)
    })
}

/// `ConvertManyStringsToNiceInsertableHumanSummary`: one text quoted and
/// spaced, a few on their own lines, more wrapped at 64 characters.
fn summary(texts: &[String]) -> String {
    let mut texts = texts.to_vec();
    human_sort(&mut texts);
    if let [one] = texts.as_slice() {
        return format!(" \"{one}\" ");
    }
    let body = if texts.len() <= 4 {
        texts.join("\n")
    } else {
        let mut lines: Vec<String> = Vec::new();
        let mut line = String::new();
        let mut todo: std::collections::VecDeque<String> = texts.into();
        while let Some(text) = todo.pop_front() {
            if line.is_empty() {
                line = text;
            } else {
                let next = format!("{line}, {text}");
                if next.chars().count() > 64 {
                    lines.push(std::mem::take(&mut line));
                    if lines.len() >= 24 {
                        todo.push_front(text);
                        lines.push(format!("and {} others", human_int(todo.len() as u64)));
                        break;
                    }
                    line = text;
                } else {
                    line = next;
                }
            }
        }
        if !line.is_empty() {
            lines.push(line);
        }
        lines.join("\n")
    };
    format!("\n\n{body}\n\n")
}

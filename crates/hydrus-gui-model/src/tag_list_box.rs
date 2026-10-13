//! A storage tag list as the reference's suggestion lists are (its
//! `ListBoxTagsStrings`, a `ListBox`): rows selected by its own hit rules
//! (a plain click on a selected row keeps the selection, a click on nothing
//! clears it, shift ranges from where the last range started, ctrl toggles),
//! its keys (arrows that wrap, home/end, page up/down, ctrl+n/p, escape,
//! ctrl+a, ctrl+c and ctrl+shift+c, enter), sibling and parent decorators
//! the list's own menu turns on and off, and its right-click menu.
//!
//! Rows are logical (one per tag); what is drawn is positional (a tag, then
//! its parents' rows when they are expanded).
use crate::write_tag_menu::{Action, Decoration, Entry, favourite_entries, relationship_entries};
use hydrus_core::search::{
    context::{LocationContext, TagContext},
    predicate::Predicate,
};
use hydrus_core::{ServiceId, Tag, tag::split_tag};
use hydrus_store::{Store, settings};
use std::collections::BTreeSet;

/// A tag, with what the service's siblings and parents say of it.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Row {
    pub tag: String,
    /// The tag it displays as, when that is another.
    pub ideal: Option<String>,
    /// Its parents, sorted.
    pub parents: Vec<String>,
}

/// Which decorators the list draws (the storage taglist options, then the
/// list's own menu).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Decorators {
    pub show_parents: bool,
    pub expand_parents: bool,
    pub show_siblings: bool,
}
impl Default for Decorators {
    fn default() -> Self {
        Self {
            show_parents: true,
            expand_parents: true,
            show_siblings: true,
        }
    }
}
impl Decorators {
    /// The storage taglist options (Options > tag editing).
    pub fn from_settings(settings: &hydrus_store::tag_editing::TagEditingSettings) -> Self {
        Self {
            show_parents: settings.tag_list_show_parents,
            expand_parents: settings.tag_list_expand_parents,
            show_siblings: settings.tag_list_show_siblings,
        }
    }
}

/// A key the list may take.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Key {
    Up,
    Down,
    Home,
    End,
    PageUp,
    PageDown,
    Escape,
    Enter,
    Delete,
    Insert,
    Char(char),
}
impl Key {
    /// A key as Slint gives its text (the special keys are private-use
    /// characters).
    pub fn from_text(text: &str) -> Option<Self> {
        let mut chars = text.chars();
        let c = chars.next()?;
        if chars.next().is_some() {
            return None;
        }
        Some(match c {
            '\u{F700}' => Self::Up,
            '\u{F701}' => Self::Down,
            '\u{F729}' => Self::Home,
            '\u{F72B}' => Self::End,
            '\u{F72C}' => Self::PageUp,
            '\u{F72D}' => Self::PageDown,
            '\u{1b}' => Self::Escape,
            '\n' | '\r' => Self::Enter,
            '\u{7f}' => Self::Delete,
            '\u{F727}' => Self::Insert,
            c => Self::Char(c.to_ascii_lowercase()),
        })
    }
}

/// What a key did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    /// Not the list's key: it goes on to the window.
    Ignored,
    /// Taken (the selection may have changed).
    Taken,
    /// Copy this text.
    Copy(String),
    /// Activate the selection (add the selected tags).
    Activate,
}

/// The list.
#[derive(Debug, Clone, Default)]
pub struct TagListBox {
    rows: Vec<Row>,
    selected: BTreeSet<usize>,
    last_hit: Option<usize>,
    shift_start: Option<usize>,
    selected_this_shift: BTreeSet<usize>,
    deselected_this_shift: BTreeSet<usize>,
    pub decorators: Decorators,
}

impl TagListBox {
    pub fn new(decorators: Decorators) -> Self {
        Self {
            decorators,
            ..Self::default()
        }
    }

    pub fn rows(&self) -> &[Row] {
        &self.rows
    }

    pub fn tags(&self) -> Vec<String> {
        self.rows.iter().map(|r| r.tag.clone()).collect()
    }

    /// New rows (`SetTags`): the tags still listed stay selected, and the
    /// first of them is hit.
    pub fn set_rows(&mut self, rows: Vec<Row>) {
        let selected = self.selected_tags();
        self.rows = rows;
        self.selected.clear();
        self.last_hit = None;
        self.shift_start = None;
        self.selected_this_shift.clear();
        self.deselected_this_shift.clear();
        for (i, row) in self.rows.iter().enumerate() {
            if selected.contains(&row.tag) {
                self.selected.insert(i);
            }
        }
        if let Some(&first) = self.selected.iter().next() {
            self.hit(false, false, Some(first));
        }
    }

    /// The selected tags, in the list's order.
    pub fn selected_tags(&self) -> Vec<String> {
        self.selected
            .iter()
            .map(|&i| self.rows[i].tag.clone())
            .collect()
    }

    /// Forget the selection (as when activated tags leave the list).
    pub fn deselect_all(&mut self) {
        self.selected.clear();
    }

    /// The positional rows drawn: (the logical row, whether it is a parent
    /// row, its text).
    pub fn drawn(&self, connector: &str) -> Vec<(usize, bool, String)> {
        let d = self.decorators;
        let mut out = Vec::new();
        for (i, row) in self.rows.iter().enumerate() {
            let mut text = row.tag.clone();
            if d.show_siblings
                && let Some(ideal) = &row.ideal
            {
                text.push_str(connector);
                text.push_str(ideal);
            }
            if d.show_parents && !d.expand_parents && !row.parents.is_empty() {
                text.push_str(&format!(
                    " ({} parents)",
                    hydrus_core::numbers::human_int(row.parents.len() as u64)
                ));
            }
            out.push((i, false, text));
            if d.show_parents && d.expand_parents {
                for parent in &row.parents {
                    out.push((i, true, format!("    {parent}")));
                }
            }
        }
        out
    }

    fn positional_rows(&self) -> usize {
        let d = self.decorators;
        self.rows
            .iter()
            .map(|r| {
                1 + if d.show_parents && d.expand_parents {
                    r.parents.len()
                } else {
                    0
                }
            })
            .sum()
    }

    fn positional_of(&self, logical: usize) -> usize {
        let d = self.decorators;
        self.rows[..logical]
            .iter()
            .map(|r| {
                1 + if d.show_parents && d.expand_parents {
                    r.parents.len()
                } else {
                    0
                }
            })
            .sum()
    }

    /// The logical row at a positional row (a parent row belongs to its tag).
    pub fn logical_at(&self, positional: usize) -> Option<usize> {
        if positional >= self.positional_rows() {
            return None;
        }
        (0..self.rows.len())
            .rev()
            .find(|&i| self.positional_of(i) <= positional)
    }

    /// Whether each positional row is drawn selected.
    pub fn mask(&self) -> Vec<bool> {
        self.drawn("")
            .into_iter()
            .map(|(i, _, _)| self.selected.contains(&i))
            .collect()
    }

    /// A click (or right or middle press) on a positional row, or on nothing
    /// (`_Hit`).
    pub fn click(&mut self, positional: Option<usize>, ctrl: bool, shift: bool) {
        let logical = positional.and_then(|p| self.logical_at(p));
        self.hit(shift, ctrl, logical);
    }

    #[allow(clippy::similar_names)] // (to and from selected, as the reference names them)
    fn hit(&mut self, shift: bool, ctrl: bool, hit: Option<usize>) {
        let mut hit = hit.filter(|&h| h < self.rows.len());
        let mut to_select = BTreeSet::new();
        let mut to_deselect = BTreeSet::new();
        let mut deselect_all = false;
        if !shift {
            self.shift_start = hit;
            self.selected_this_shift.clear();
            self.deselected_this_shift.clear();
        }
        if shift {
            if hit.is_none() && self.shift_start.is_some() {
                hit = self.rows.len().checked_sub(1);
            }
            if let Some(h) = hit {
                if self.shift_start.is_none() || self.last_hit.is_none() {
                    if self.last_hit.is_none() {
                        self.last_hit = Some(h);
                    }
                    self.shift_start = self.last_hit;
                }
                if ctrl {
                    if !self.selected_this_shift.is_empty() {
                        self.shift_start = self.last_hit;
                        self.selected_this_shift.clear();
                    }
                } else if !self.deselected_this_shift.is_empty() {
                    self.shift_start = self.last_hit;
                    self.deselected_this_shift.clear();
                }
                let start = self.shift_start.unwrap_or(h);
                let range: BTreeSet<usize> = (start.min(h)..=start.max(h)).collect();
                if ctrl {
                    to_deselect = range
                        .iter()
                        .copied()
                        .filter(|i| self.selected.contains(i))
                        .collect();
                    to_select = self
                        .deselected_this_shift
                        .iter()
                        .copied()
                        .filter(|i| !range.contains(i))
                        .collect();
                    self.deselected_this_shift
                        .extend(to_deselect.iter().copied());
                    self.deselected_this_shift
                        .retain(|i| !to_select.contains(i));
                } else {
                    to_select = range
                        .iter()
                        .copied()
                        .filter(|i| !self.selected.contains(i))
                        .collect();
                    to_deselect = self
                        .selected_this_shift
                        .iter()
                        .copied()
                        .filter(|i| !range.contains(i))
                        .collect();
                    self.selected_this_shift.extend(to_select.iter().copied());
                    self.selected_this_shift
                        .retain(|i| !to_deselect.contains(i));
                }
            }
        } else if ctrl {
            if let Some(h) = hit {
                if self.selected.contains(&h) {
                    to_deselect.insert(h);
                } else {
                    to_select.insert(h);
                }
            }
        } else {
            match hit {
                None => deselect_all = true,
                Some(h) if !self.selected.contains(&h) => {
                    deselect_all = true;
                    to_select.insert(h);
                }
                Some(_) => {}
            }
        }
        if deselect_all {
            self.selected.clear();
        }
        self.selected.extend(to_select);
        for i in to_deselect {
            self.selected.remove(&i);
        }
        self.last_hit = hit;
    }

    /// Select every row (ctrl+a).
    pub fn select_all(&mut self) {
        self.selected = (0..self.rows.len()).collect();
    }

    /// The text ctrl+c copies: the selected tags (or all, with none
    /// selected), with their parents when `parents`.
    pub fn copy_text(&self, parents: bool) -> String {
        let rows: Vec<&Row> = if self.selected.is_empty() {
            self.rows.iter().collect()
        } else {
            self.selected.iter().map(|&i| &self.rows[i]).collect()
        };
        strings(&rows, parents, false, false).join("\n")
    }

    /// A key, with ctrl and shift held as said; `page` is how many rows a
    /// page holds.
    pub fn key(&mut self, key: Key, ctrl: bool, shift: bool, page: usize) -> Outcome {
        match key {
            Key::Delete => return Outcome::Taken,
            Key::Escape => {
                if self.selected.is_empty() {
                    return Outcome::Ignored;
                }
                self.selected.clear();
                return Outcome::Taken;
            }
            Key::Enter => return Outcome::Activate,
            Key::Char('a') if ctrl => {
                self.select_all();
                return Outcome::Taken;
            }
            Key::Char('c') | Key::Insert if ctrl => {
                return Outcome::Copy(self.copy_text(shift));
            }
            _ => {}
        }
        let n = self.rows.len();
        if n <= 1 {
            return Outcome::Ignored;
        }
        let mut ctrl = ctrl;
        let hit = match (key, self.last_hit) {
            (Key::Home, _) => Some(0),
            (Key::End, _) => Some(n - 1),
            (_, None) => None,
            (Key::Char('p'), Some(last)) if ctrl => {
                ctrl = false;
                Some((last + n - 1) % n)
            }
            (Key::Char('n'), Some(last)) if ctrl => {
                ctrl = false;
                Some((last + 1) % n)
            }
            (Key::Up, Some(last)) => Some((last + n - 1) % n),
            (Key::Down, Some(last)) => Some((last + 1) % n),
            (Key::PageUp | Key::PageDown, Some(last)) => {
                let at = self.positional_of(last);
                let to = if key == Key::PageUp {
                    at.saturating_sub(page)
                } else {
                    (at + page).min(self.positional_rows() - 1)
                };
                self.logical_at(to)
            }
            _ => None,
        };
        match hit {
            Some(hit) => {
                self.hit(shift, ctrl, Some(hit));
                Outcome::Taken
            }
            None => Outcome::Ignored,
        }
    }

    /// After the selection was activated and left the list (enter): hit
    /// the row where the first of them was, or the one before, or the top
    /// (`_ActivateFromKeyboard`).
    pub fn rehit_after_activation(&mut self, first_index: usize) {
        for ideal in [Some(first_index), first_index.checked_sub(1), Some(0)]
            .into_iter()
            .flatten()
        {
            if ideal < self.rows.len() {
                self.hit(false, false, Some(ideal));
                break;
            }
        }
    }

    /// The first selected row's index, if any.
    pub fn first_selected(&self) -> Option<usize> {
        self.selected.iter().next().copied()
    }

    /// The pages a middle click opens: the selection, or with shift their
    /// OR (when there are several).
    pub fn middle_click_predicates(&self, shift: bool) -> Vec<Predicate> {
        let predicates = self.selected_predicates();
        if shift && predicates.len() > 1 {
            vec![Predicate::Or(predicates)]
        } else {
            predicates
        }
    }

    fn selected_predicates(&self) -> Vec<Predicate> {
        self.selected
            .iter()
            .filter_map(|&i| Tag::new(&self.rows[i].tag))
            .map(|tag| Predicate::Tag {
                tag,
                inclusive: true,
            })
            .collect()
    }

    /// The right-click menu (`ShowMenuFromSignal`), for the selection as it
    /// is (a right press hits its row first).
    pub fn menu(&self, store: &Store, list: u8) -> Vec<Entry> {
        let mut entries = Vec::new();
        if self.rows.is_empty() {
            return entries;
        }
        let d = self.decorators;
        let decorate = |label: &str, kind, value| {
            Entry::Item(label.into(), Action::ListDecorate { list, kind, value })
        };
        if d.show_parents {
            if d.expand_parents {
                if self.positional_rows() != self.rows.len() {
                    entries.push(decorate(
                        "collapse parent rows",
                        Decoration::Expanded,
                        false,
                    ));
                }
            } else {
                entries.push(decorate("expand parent rows", Decoration::Expanded, true));
            }
            entries.push(decorate(
                "hide parent decorators",
                Decoration::Parents,
                false,
            ));
        } else {
            entries.push(decorate(
                "show parent decorators",
                Decoration::Parents,
                true,
            ));
        }
        if d.show_siblings {
            entries.push(decorate(
                "hide sibling decorators",
                Decoration::Siblings,
                false,
            ));
        } else {
            entries.push(decorate(
                "show sibling decorators",
                Decoration::Siblings,
                true,
            ));
        }
        separator(&mut entries);
        let selected: Vec<&Row> = self.selected.iter().map(|&i| &self.rows[i]).collect();
        let all: Vec<&Row> = self.rows.iter().collect();
        let mut copy = Vec::new();
        let tags = strings(&selected, false, false, false);
        let mut selection = String::new();
        if !tags.is_empty() {
            let subtags = strings(&selected, false, true, false);
            let underscores = strings(&selected, false, true, true);
            selection = if tags.len() == 1 {
                tags[0].clone()
            } else {
                format!("{} selected", tags.len())
            };
            copy.push(Entry::Item(
                selection.clone(),
                Action::Copy(tags.join("\n")),
            ));
            let set = |v: &[String]| v.iter().cloned().collect::<BTreeSet<_>>();
            if !subtags.is_empty() && set(&subtags) != set(&tags) {
                let label = if subtags.len() == 1 {
                    subtags[0].clone()
                } else {
                    format!("{} selected subtags", subtags.len())
                };
                copy.push(Entry::Item(label, Action::Copy(subtags.join("\n"))));
            }
            if !underscores.is_empty() && set(&underscores) != set(&subtags) {
                let label = if underscores.len() == 1 {
                    underscores[0].clone()
                } else {
                    format!("{} selected subtags with underscores", underscores.len())
                };
                copy.push(Entry::Item(label, Action::Copy(underscores.join("\n"))));
            }
            let with_parents = strings(&selected, true, false, false);
            let parents = with_parents.len() - tags.len();
            if parents > 0 {
                separator(&mut copy);
                selection = format!(
                    "{selection} and {} parents",
                    hydrus_core::numbers::human_int(parents as u64)
                );
                copy.push(Entry::Item(
                    selection.clone(),
                    Action::Copy(with_parents.join("\n")),
                ));
            }
        }
        if self.rows.len() > self.selected.len() {
            separator(&mut copy);
            copy.push(Entry::Item(
                "all tags".into(),
                Action::Copy(strings(&all, false, false, false).join("\n")),
            ));
            copy.push(Entry::Item(
                "all subtags".into(),
                Action::Copy(strings(&all, false, true, false).join("\n")),
            ));
        }
        entries.push(Entry::Menu("copy".into(), copy));
        if selected.len() == 1 {
            entries.extend(relationship_entries(store, &selected[0].tag).unwrap_or_default());
        } else if !selected.is_empty() {
            let tags: Vec<String> = selected.iter().map(|r| r.tag.clone()).collect();
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
        if !selected.is_empty() {
            separator(&mut entries);
            let predicates = self.selected_predicates();
            let defaults: settings::SearchDefaults = store.read(settings::get).unwrap_or_default();
            let context = TagContext::new(defaults.tag_service, true, true);
            let launch = |predicates: Vec<Predicate>, duplicate| Action::Launch {
                location: LocationContext::default(),
                context: context.clone(),
                predicates,
                duplicate,
            };
            let mut open = vec![Entry::Item(
                format!("open a new search page for {selection}"),
                launch(predicates.clone(), false),
            )];
            if predicates.len() > 1 {
                open.push(Entry::Item(
                    format!("open a new OR search page for {selection}"),
                    launch(vec![Predicate::Or(predicates.clone())], false),
                ));
                open.push(Entry::Item(
                    "open new search pages for each in selection".into(),
                    Action::LaunchMany {
                        location: LocationContext::default(),
                        context: context.clone(),
                        pages: predicates.iter().cloned().map(|p| vec![p]).collect(),
                    },
                ));
            }
            separator(&mut open);
            open.push(Entry::Item(
                format!("open a new duplicate filter page for {selection}"),
                launch(predicates, true),
            ));
            entries.push(Entry::Menu("open".into(), open));
            if let [one] = selected.as_slice() {
                entries.push(Entry::Menu(
                    "favourites".into(),
                    favourite_entries(store, &one.tag),
                ));
            }
            separator(&mut entries);
            entries.push(Entry::Menu(
                "maintenance".into(),
                vec![Entry::Item(
                    "regenerate tag display".into(),
                    Action::Regenerate {
                        tags: selected.iter().map(|r| r.tag.clone()).collect(),
                    },
                )],
            ));
        }
        entries
    }

    /// A decorator the list's menu turned on or off.
    pub fn decorate(&mut self, kind: Decoration, value: bool) {
        match kind {
            Decoration::Parents => self.decorators.show_parents = value,
            Decoration::Expanded => self.decorators.expand_parents = value,
            Decoration::Siblings => self.decorators.show_siblings = value,
        }
    }
}

/// `AppendSeparator`: never first, never twice.
fn separator(entries: &mut Vec<Entry>) {
    if !entries.is_empty() && !matches!(entries.last(), Some(Entry::Separator)) {
        entries.push(Entry::Separator);
    }
}

/// The copyable strings of these rows (`_GetCopyableTagStrings`): with
/// parents after each tag, as subtags, with underscores; deduplicated.
fn strings(rows: &[&Row], parents: bool, subtags: bool, underscores: bool) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for row in rows {
        let mut texts = vec![row.tag.clone()];
        if parents {
            texts.extend(row.parents.iter().cloned());
        }
        for text in texts {
            let mut text = if subtags {
                split_tag(&text).1.to_owned()
            } else {
                text
            };
            if text.is_empty() {
                continue;
            }
            if underscores {
                text = text.replace(' ', "_");
            }
            if !out.contains(&text) {
                out.push(text);
            }
        }
    }
    out
}

/// The rows for these tags on a tag service: what each displays as and its
/// parents (`tag_display_decorators`).
pub fn decorated(store: &Store, service: ServiceId, tags: &[String]) -> Vec<Row> {
    let snapshot = store.snapshot();
    let graph = snapshot.display.get(service);
    store
        .read(|conn| {
            let mut rows = Vec::new();
            for tag in tags {
                let mut row = Row {
                    tag: tag.clone(),
                    ..Row::default()
                };
                if let Some(cleaned) = Tag::new(tag)
                    && let Some(id) = hydrus_store::master::tag_id(conn, &cleaned)?
                {
                    let ideal = graph.ideal(id);
                    let mut ids = graph.ancestors(id).to_vec();
                    ids.push(ideal);
                    let texts = hydrus_store::master::tags(conn, &ids)?;
                    row.ideal = (ideal != id).then(|| texts[&ideal].as_str().to_owned());
                    row.parents = graph
                        .ancestors(id)
                        .iter()
                        .map(|id| texts[id].as_str().to_owned())
                        .collect();
                    hydrus_core::sort::human_sort(&mut row.parents);
                }
                rows.push(row);
            }
            Ok(rows)
        })
        .unwrap_or_else(|_| {
            tags.iter()
                .map(|tag| Row {
                    tag: tag.clone(),
                    ..Row::default()
                })
                .collect()
        })
}

//! Snapshot-based command palette queries. Workers never touch GUI owners; the
//! owner accepts results only for its current query generation and provider set.

use hydrus_core::casefold::casefold;
use hydrus_core::pages::{FavouriteSearch, PageKey};
pub use hydrus_store::command_palette::{CommandPaletteSettings, Provider};

use crate::main_menu::Command;

/// A frozen launch payload, independent of a result's current display position.
#[derive(Debug, Clone, PartialEq)]
pub enum Action {
    Page(PageKey),
    Favourite(Box<FavouriteSearch>),
    MainMenu(Command),
    /// Index in the native owner's frozen thumbnail-menu action snapshot.
    MediaMenu(usize),
    /// Calculator results stay open and do not run an application command.
    Calculator,
}

/// A page's current visible menu name, collected on the GUI thread in tree order.
#[derive(Debug, Clone)]
pub struct OpenPage {
    pub key: PageKey,
    pub name: String,
    pub parent_name: Option<String>,
    pub notebook: bool,
}

/// A leaf from a real native menu, with its launch payload and submenu path.
#[derive(Debug, Clone)]
pub struct MenuItem {
    pub label: String,
    pub parent: String,
    pub checked: Option<bool>,
    pub action: Option<Action>,
}

/// Immutable GUI-thread data sent to provider workers. History is oldest first.
#[derive(Debug, Clone, Default)]
pub struct Snapshot {
    pub pages: Vec<OpenPage>,
    pub history: Vec<(PageKey, String)>,
    pub favourites: Vec<FavouriteSearch>,
    pub main_menu: Vec<MenuItem>,
    pub media_menu: Vec<MenuItem>,
}

/// A provider result. An unavailable native menu command can still be searched.
#[derive(Debug, Clone, PartialEq)]
pub struct Suggestion {
    pub primary: String,
    pub secondary: String,
    pub checked: Option<bool>,
    pub action: Option<Action>,
}

fn matches(label: &str, folded: &str) -> bool {
    casefold(label).contains(folded)
}

fn typed_enough(query: &str, settings: &CommandPaletteSettings) -> bool {
    let length = query.trim().chars().count();
    length == 0 || length >= settings.threshold
}

fn limit(rows: &mut Vec<Suggestion>, maximum: Option<usize>) {
    if let Some(maximum) = maximum {
        rows.truncate(maximum);
    }
}

fn history_ordinal(number: usize) -> String {
    let suffix = if (11..=13).contains(&(number % 100)) {
        "th"
    } else {
        match number % 10 {
            1 => "st",
            2 => "nd",
            3 => "rd",
            _ => "th",
        }
    };
    format!("{number}{suffix} result in history")
}

/// Filter one provider against a frozen snapshot; it is safe to run off-thread.
pub fn query(
    provider: Provider,
    text: &str,
    settings: &CommandPaletteSettings,
    data: &Snapshot,
) -> Vec<Suggestion> {
    if !settings.provider_order.contains(&provider) || !typed_enough(text, settings) {
        return Vec::new();
    }
    let folded = casefold(text);
    let mut rows = Vec::new();
    match provider {
        Provider::Pages => {
            if text.is_empty() && !settings.initially_show_pages {
                return rows;
            }
            for page in &data.pages {
                if (!page.notebook || settings.show_notebooks) && matches(&page.name, &folded) {
                    rows.push(Suggestion {
                        primary: page.name.clone(),
                        secondary: page.parent_name.as_ref().map_or_else(
                            || "top level page".to_owned(),
                            |name| format!("child of '{name}'"),
                        ),
                        checked: None,
                        action: Some(Action::Page(page.key)),
                    });
                }
            }
            limit(&mut rows, settings.page_limit);
        }
        Provider::History => {
            if text.is_empty() && !settings.initially_show_history {
                return rows;
            }
            for (key, name) in data.history.iter().rev() {
                if matches(name, &folded) {
                    rows.push(Suggestion {
                        primary: name.clone(),
                        secondary: history_ordinal(rows.len() + 1),
                        checked: None,
                        action: Some(Action::Page(*key)),
                    });
                }
            }
            limit(&mut rows, settings.history_limit);
        }
        Provider::Favourites => {
            if text.is_empty() && !settings.initially_show_favourites {
                return rows;
            }
            for favourite in &data.favourites {
                if matches(&favourite.name, &folded)
                    || favourite
                        .folder
                        .as_ref()
                        .is_some_and(|folder| matches(folder, &folded))
                {
                    rows.push(Suggestion {
                        primary: favourite.name.clone(),
                        secondary: favourite
                            .folder
                            .as_ref()
                            .map_or_else(String::new, |folder| {
                                format!("{folder} - favourite search")
                            }),
                        checked: None,
                        action: Some(Action::Favourite(Box::new(favourite.clone()))),
                    });
                }
            }
            limit(&mut rows, settings.favourite_limit);
        }
        Provider::MainMenu | Provider::MediaMenu => {
            let enabled = if provider == Provider::MainMenu {
                settings.show_main_menu
            } else {
                settings.show_media_menu
            };
            if !enabled || data.history.is_empty() || text.chars().count() < 3 {
                return rows;
            }
            let menu = if provider == Provider::MainMenu {
                &data.main_menu
            } else {
                &data.media_menu
            };
            for item in menu {
                let label = item.label.replace('&', "");
                if matches(&item.label, &folded) || matches(&label, &folded) {
                    rows.push(Suggestion {
                        primary: label,
                        secondary: item.parent.clone(),
                        checked: item.checked,
                        action: item.action.clone(),
                    });
                }
            }
        }
        // Calculator parsing is independent of application contexts and is added separately.
        Provider::Calculator => {}
    }
    rows
}

/// Flatten the actual menubar without matching submenu headings as leaf names.
pub fn main_menu_items(entries: &[crate::main_menu::Entry]) -> Vec<MenuItem> {
    fn walk(entries: &[crate::main_menu::Entry], parent: &str, out: &mut Vec<MenuItem>) {
        use crate::main_menu::Entry;
        for entry in entries {
            match entry {
                Entry::Menu { label, entries, .. } => {
                    let label = label.replace('&', "");
                    let path = if parent.is_empty() {
                        label
                    } else {
                        format!("{parent} | {label}")
                    };
                    walk(entries, &path, out);
                }
                Entry::Item {
                    label,
                    command,
                    enabled,
                } => out.push(MenuItem {
                    label: label.clone(),
                    parent: parent.to_owned(),
                    checked: None,
                    action: command.clone().filter(|_| *enabled).map(Action::MainMenu),
                }),
                Entry::Check {
                    label,
                    command,
                    checked,
                } => out.push(MenuItem {
                    label: label.clone(),
                    parent: parent.to_owned(),
                    checked: Some(*checked),
                    action: command.clone().map(Action::MainMenu),
                }),
                Entry::Separator => {}
            }
        }
    }
    let mut out = Vec::new();
    walk(entries, "", &mut out);
    out
}

/// An opaque request identity. A closed owner rejects even its last request.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Ticket(u64);

/// Current query and accumulated provider results. Provider arrival order never
/// changes the user's configured group order or revives a cancelled query.
#[derive(Debug, Default)]
pub struct Results {
    generation: u64,
    open: bool,
    order: Vec<Provider>,
    groups: Vec<(Provider, Vec<Suggestion>)>,
}

impl Results {
    /// Clear prior results and start a query for this provider order.
    pub fn begin(&mut self, order: &[Provider]) -> Ticket {
        self.generation = self
            .generation
            .checked_add(1)
            .expect("palette request generation exhausted");
        self.open = true;
        self.order = order.to_vec();
        self.groups.clear();
        Ticket(self.generation)
    }

    /// Accept one worker's result only if its owner and query are still current.
    pub fn accept(&mut self, ticket: Ticket, provider: Provider, rows: Vec<Suggestion>) -> bool {
        if !self.open || ticket.0 != self.generation || !self.order.contains(&provider) {
            return false;
        }
        self.groups.retain(|(p, _)| *p != provider);
        self.groups.push((provider, rows));
        true
    }

    /// Flatten nonempty groups in persisted provider order.
    pub fn ordered(&self) -> Vec<(Provider, &Suggestion)> {
        self.order
            .iter()
            .flat_map(|provider| {
                self.groups
                    .iter()
                    .find(|(p, _)| p == provider)
                    .into_iter()
                    .flat_map(move |(_, rows)| rows.iter().map(move |row| (*provider, row)))
            })
            .collect()
    }

    /// Invalidate worker results and launch payloads when the owning palette closes.
    pub fn close(&mut self) {
        self.open = false;
        self.groups.clear();
    }
}

/// Staged provider queue with Qt extended selection. Removal is applied only
/// after the owner's confirmation; a cancelled add leaves the queue untouched.
#[derive(Debug, Clone)]
pub struct ProviderOrder {
    pub order: Vec<Provider>,
    pub selection: crate::list_selection::ListSelection<Provider>,
}

impl ProviderOrder {
    /// Edit an owned copy so the options owner's cancel can discard the draft.
    pub fn new(order: Vec<Provider>) -> Self {
        Self {
            order,
            selection: crate::list_selection::ListSelection::default(),
        }
    }

    /// Select a provider using plain, control and shift click semantics.
    pub fn click(&mut self, row: usize, ctrl: bool, shift: bool) {
        self.selection.click(&self.order, row, ctrl, shift);
    }

    /// Move selected rows by one position, as the reference's queue buttons do.
    pub fn move_selected(&mut self, down: bool) {
        let mut indices: Vec<_> = self
            .order
            .iter()
            .enumerate()
            .filter_map(|(index, provider)| self.selection.is_selected(*provider).then_some(index))
            .collect();
        if down {
            indices.reverse();
        }
        for index in indices {
            let destination = if down {
                (index + 1).min(self.order.len() - 1)
            } else {
                index.saturating_sub(1)
            };
            self.order.swap(index, destination);
        }
    }

    /// The reference's confirmation prompt, or none if nothing is selected.
    pub fn removal_question(&self) -> Option<String> {
        let count = self.selection.in_order(&self.order).len();
        (count > 0).then(|| format!("Remove {count} selected?"))
    }

    /// Apply a confirmed removal while retaining the order of other providers.
    pub fn remove_selected(&mut self) {
        self.order
            .retain(|provider| !self.selection.is_selected(*provider));
        self.selection = crate::list_selection::ListSelection::default();
    }

    /// Providers available in the add dialog, in registration order.
    pub fn missing(&self) -> Vec<Provider> {
        Provider::ALL
            .into_iter()
            .filter(|provider| !self.order.contains(provider))
            .collect()
    }

    /// Append a missing provider. None represents cancelling the selection dialog.
    pub fn add(&mut self, provider: Option<Provider>) -> bool {
        let Some(provider) = provider.filter(|p| !self.order.contains(p)) else {
            return false;
        };
        self.order.push(provider);
        true
    }
}

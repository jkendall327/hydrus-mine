//! The Options import-options page: three lists over one isolated manager draft.
use std::{cell::RefCell, collections::BTreeMap, rc::Rc};

use hydrus_core::{
    import_options::{CallerType, ImportOptionsManager, ImportOptionsSlice},
    url::{UrlClass, UrlType},
};
use hydrus_store::settings::ImportOptionsUiSettings;

use crate::{import_options_editor::container_summary, list_selection::ListSelection};

pub const WARNING: &str = "This panel is advanced! The default settings are fine, so if you are not sure what is going on, hold off or check the help.";
pub const TLDR: &str = "tl;dr: Go into \"gallery/post urls\" and make sure tags are going where you want. Never touch this again, and, on occasion, set a \"custom\" import options override on a specific downloader.";
pub const GLOBAL_STACK: &str = "The \"global\" set is the fallback default that all the other options will eventually use as the options-of-last-resort. No importer uses \"global\" as its primary import context.";
pub const GLOBAL_CLEAR: &str = "You cannot clear the \"global\" set! It has to have an entry for everything, to be the fallback default.";
pub const CALLERS: [CallerType; 7] = [
    CallerType::Global,
    CallerType::ClientApi,
    CallerType::LocalImport,
    CallerType::LocalImportFolder,
    CallerType::WatcherUrls,
    CallerType::PostUrls,
    CallerType::Subscription,
];

#[derive(Debug, Clone, PartialEq)]
pub struct Value {
    pub manager: ImportOptionsManager,
    pub ui: ImportOptionsUiSettings,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum List {
    Defaults,
    UrlClasses,
    Favourites,
}
impl List {
    pub const ALL: [Self; 3] = [Self::Defaults, Self::UrlClasses, Self::Favourites];
    pub fn index(self) -> usize {
        match self {
            Self::Defaults => 0,
            Self::UrlClasses => 1,
            Self::Favourites => 2,
        }
    }
    pub fn from_index(index: usize) -> Option<Self> {
        Self::ALL.get(index).copied()
    }
}

/// A clicked row, frozen independently of subsequent list selection or sorting.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Target {
    Caller(CallerType),
    Url(String),
    Favourite(String),
}
impl Target {
    pub fn caller(&self) -> CallerType {
        match self {
            Self::Caller(caller) => *caller,
            Self::Url(_) => CallerType::UrlClass,
            Self::Favourite(_) => CallerType::Favourites,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Row {
    pub id: usize,
    pub cells: Vec<String>,
    pub target: Target,
    pub selected: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Request {
    pub message: String,
    pub targets: Vec<Target>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Reset {
    Defaults,
    UrlClasses,
    Favourites,
}
impl Reset {
    pub const ALL: [Self; 3] = [Self::Defaults, Self::UrlClasses, Self::Favourites];
    pub fn key(self) -> &'static str {
        match self {
            Self::Defaults => "defaults",
            Self::UrlClasses => "url_classes",
            Self::Favourites => "favourites",
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            Self::Defaults => "reset top defaults",
            Self::UrlClasses => "reset url classes",
            Self::Favourites => "reset favourites/templates",
        }
    }
    pub fn description(self) -> &'static str {
        match self {
            Self::Defaults => {
                "Clear the default import options list and set everything back to how a new client database has it."
            }
            Self::UrlClasses => "Clear all the default URL Class entries.",
            Self::Favourites => "Clear the favourites/templates under the star icon button.",
        }
    }
    pub fn question(self) -> String {
        format!(
            "Hey, you selected to reset \"{}\". I am going to clear everything and reset to defaults, ok? If it does not go how you want, cancel out of the options dialog.",
            self.key()
        )
    }
}

#[derive(Debug, Clone)]
struct Selection {
    selected: ListSelection<usize>,
    column: Option<usize>,
    ascending: bool,
}
impl Default for Selection {
    fn default() -> Self {
        Self {
            selected: ListSelection::default(),
            column: Some(0),
            ascending: true,
        }
    }
}

#[derive(Debug, Clone)]
pub struct Editor {
    pub manager: Rc<RefCell<ImportOptionsManager>>,
    pub ui: ImportOptionsUiSettings,
    pub classes: Vec<UrlClass>,
    selections: [Selection; 3],
    favourite_ids: BTreeMap<String, usize>,
    next_id: usize,
}
impl Editor {
    pub fn new(value: &Value, classes: &[UrlClass]) -> Self {
        let classes = classes
            .iter()
            .filter(|class| {
                matches!(
                    class.url_type,
                    UrlType::Post | UrlType::Watchable | UrlType::Gallery
                )
            })
            .cloned()
            .collect::<Vec<_>>();
        let mut editor = Self {
            manager: Rc::new(RefCell::new(value.manager.clone())),
            ui: value.ui.clone(),
            next_id: CALLERS.len() + classes.len(),
            classes,
            selections: std::array::from_fn(|_| Selection::default()),
            favourite_ids: BTreeMap::new(),
        };
        editor.selections[2].column = None;
        editor.sync();
        editor
    }
    /// Favourites can also change through the shared star menu. Keep stable row IDs.
    pub fn sync(&mut self) {
        let names = self
            .manager
            .borrow()
            .favourites
            .iter()
            .map(|(name, _)| name.clone())
            .collect::<Vec<_>>();
        self.favourite_ids.retain(|name, id| {
            let keep = names.contains(name);
            if !keep {
                self.selections[2].selected.forget(*id);
            }
            keep
        });
        for name in names {
            if !self.favourite_ids.contains_key(&name) {
                self.favourite_ids.insert(name, self.next_id);
                self.next_id += 1;
            }
        }
    }
    pub fn value(&self) -> Value {
        Value {
            manager: self.manager.borrow().clone(),
            ui: self.ui.clone(),
        }
    }
    pub fn rows(&self, list: List, name: &dyn Fn(&str) -> String) -> Vec<Row> {
        let manager = self.manager.borrow();
        let selection = &self.selections[list.index()];
        let mut rows = match list {
            List::Defaults => CALLERS
                .iter()
                .enumerate()
                .map(|(id, &caller)| Row {
                    id,
                    cells: vec![
                        caller.name().into(),
                        manager
                            .caller_default(caller)
                            .map_or_else(String::new, |slice| container_summary(slice, name)),
                    ],
                    target: Target::Caller(caller),
                    selected: selection.selected.is_selected(id),
                })
                .collect::<Vec<_>>(),
            List::UrlClasses => self
                .classes
                .iter()
                .enumerate()
                .map(|(index, class)| {
                    let key = hex::encode(&class.key);
                    let id = CALLERS.len() + index;
                    Row {
                        id,
                        cells: vec![
                            class.name.clone(),
                            class.url_type.name().unwrap_or_default().into(),
                            manager
                                .url_class_default(&key)
                                .map_or_else(String::new, |slice| container_summary(slice, name)),
                        ],
                        target: Target::Url(key),
                        selected: selection.selected.is_selected(id),
                    }
                })
                .collect(),
            List::Favourites => manager
                .favourites
                .iter()
                .filter_map(|(title, slice)| {
                    self.favourite_ids.get(title).map(|&id| Row {
                        id,
                        cells: vec![title.clone(), container_summary(slice, name)],
                        target: Target::Favourite(title.clone()),
                        selected: selection.selected.is_selected(id),
                    })
                })
                .collect(),
        };
        if let Some(column) = selection.column {
            rows.sort_by(|a, b| {
                let order = if list == List::Defaults && column == 0 {
                    b.id.cmp(&a.id)
                } else {
                    a.cells
                        .get(column)
                        .cmp(&b.cells.get(column))
                        .then_with(|| a.id.cmp(&b.id))
                };
                if selection.ascending {
                    order
                } else {
                    order.reverse()
                }
            });
        }
        if list == List::Favourites && selection.column.is_none() {
            rows.sort_by_key(|row| row.id);
        }
        rows
    }
    pub fn sort(&mut self, list: List, column: usize, ascending: bool) {
        if column < if list == List::UrlClasses { 3 } else { 2 } {
            self.selections[list.index()].column = Some(column);
            self.selections[list.index()].ascending = ascending;
        }
    }
    pub fn sort_state(&self, list: List) -> (Option<usize>, bool) {
        let s = &self.selections[list.index()];
        (s.column, s.ascending)
    }
    pub fn click(
        &mut self,
        list: List,
        index: usize,
        ctrl: bool,
        shift: bool,
        name: &dyn Fn(&str) -> String,
    ) {
        let order = self
            .rows(list, name)
            .iter()
            .map(|row| row.id)
            .collect::<Vec<_>>();
        self.selections[list.index()]
            .selected
            .click(&order, index, ctrl, shift);
    }
    pub fn selected(&self, list: List, name: &dyn Fn(&str) -> String) -> Vec<Target> {
        self.rows(list, name)
            .into_iter()
            .filter(|row| row.selected)
            .map(|row| row.target)
            .collect()
    }
    pub fn one(&self, list: List, name: &dyn Fn(&str) -> String) -> Option<Target> {
        let selected = self.selected(list, name);
        if selected.len() == 1 {
            selected.into_iter().next()
        } else {
            None
        }
    }
    pub fn own(&self, target: &Target) -> Option<ImportOptionsSlice> {
        let manager = self.manager.borrow();
        match target {
            Target::Caller(caller) => manager.caller_default(*caller).cloned(),
            Target::Url(key) => manager.url_class_default(key).cloned(),
            Target::Favourite(title) => manager
                .favourites
                .iter()
                .find(|(name, _)| name == title)
                .map(|(_, slice)| slice.clone()),
        }
    }
    pub fn set(&mut self, target: &Target, value: ImportOptionsSlice) -> bool {
        let mut manager = self.manager.borrow_mut();
        match target {
            Target::Caller(caller) => {
                if let Some((_, slice)) = manager
                    .caller_defaults
                    .iter_mut()
                    .find(|(c, _)| c == caller)
                {
                    *slice = value;
                } else {
                    manager.caller_defaults.push((*caller, value));
                }
            }
            Target::Url(key) => {
                if !self
                    .classes
                    .iter()
                    .any(|class| hex::encode(&class.key) == *key)
                {
                    return false;
                }
                if let Some((_, slice)) = manager
                    .url_class_defaults
                    .iter_mut()
                    .find(|(k, _)| k == key)
                {
                    *slice = value;
                } else {
                    manager.url_class_defaults.push((key.clone(), value));
                }
            }
            Target::Favourite(title) => {
                let Some((_, slice)) = manager
                    .favourites
                    .iter_mut()
                    .find(|(name, _)| name == title)
                else {
                    return false;
                };
                *slice = value;
            }
        }
        true
    }
    pub fn save_favourite(
        &mut self,
        original: Option<&str>,
        name: &str,
        value: ImportOptionsSlice,
    ) -> String {
        let original_id = original
            .and_then(|name| self.favourite_ids.get(name))
            .copied();
        let actual = crate::import_options_overwrite::save_favourite(
            &mut self.manager.borrow_mut(),
            original,
            name,
            value,
        );
        if let Some(id) = original_id {
            if let Some(original) = original {
                self.favourite_ids.remove(original);
            }
            self.favourite_ids.insert(actual.clone(), id);
        }
        self.sync();
        actual
    }
    pub fn clear_request(
        &self,
        list: List,
        name: &dyn Fn(&str) -> String,
    ) -> Result<Option<Request>, String> {
        let mut targets = self.selected(list, name);
        if targets.is_empty() || list == List::Favourites {
            return Ok(None);
        }
        targets.retain(|target| *target != Target::Caller(CallerType::Global));
        if targets.is_empty() {
            return Err(GLOBAL_CLEAR.into());
        }
        let message = if list == List::Defaults {
            format!(
                "Clear all custom import options from {}?",
                targets
                    .iter()
                    .map(|target| target.caller().name())
                    .collect::<Vec<_>>()
                    .join(", ")
            )
        } else if targets.len() == 1 {
            "Clear all custom import options for this entry?".into()
        } else {
            format!(
                "Clear all custom import options for {} url class entries?",
                hydrus_core::numbers::human_int(targets.len() as u64)
            )
        };
        Ok(Some(Request { message, targets }))
    }
    pub fn clear(&mut self, targets: &[Target]) {
        for target in targets {
            if *target != Target::Caller(CallerType::Global) {
                self.set(target, ImportOptionsSlice::default());
            }
        }
    }
    pub fn delete_request(&self, name: &dyn Fn(&str) -> String) -> Option<Request> {
        let targets = self.selected(List::Favourites, name);
        let message = match targets.as_slice() {
            [] => return None,
            [Target::Favourite(title)] => {
                format!("Delete the favourite/profile named \"{title}\"?")
            }
            _ => format!(
                "Delete the {} selected favourites/profiles?",
                hydrus_core::numbers::human_int(targets.len() as u64)
            ),
        };
        Some(Request { message, targets })
    }
    pub fn delete(&mut self, targets: &[Target]) {
        self.manager
            .borrow_mut()
            .favourites
            .retain(|(name, _)| !targets.contains(&Target::Favourite(name.clone())));
        self.sync();
    }
    pub fn reset(&mut self, reset: Reset) {
        let defaults = ImportOptionsManager::default();
        match reset {
            Reset::Defaults => {
                for (caller, slice) in defaults.caller_defaults {
                    self.set(&Target::Caller(caller), slice);
                }
            }
            Reset::UrlClasses => {
                self.manager.borrow_mut().url_class_defaults = defaults.url_class_defaults
            }
            Reset::Favourites => self.manager.borrow_mut().favourites = defaults.favourites,
        }
        self.sync();
    }
    pub fn stack(&self, target: &Target) -> Option<String> {
        let (intro, lines) = match target {
            Target::Caller(CallerType::Global) => return Some(GLOBAL_STACK.into()),
            Target::Caller(caller) => {
                let tail = match caller {
                    CallerType::Subscription => vec![
                        "any custom import options for the particular subscription",
                        "any matching URL Class",
                        "subscription",
                        "gallery/post urls",
                        "global",
                    ],
                    CallerType::PostUrls => vec![
                        "any custom import options for the particular downloader page or subscription",
                        "any matching URL Class",
                        "maybe \"subscription\"",
                        "gallery/post urls",
                        "global",
                    ],
                    CallerType::WatcherUrls => vec![
                        "any custom import options for the particular watcher page",
                        "any matching URL Class",
                        "watchable urls",
                        "global",
                    ],
                    CallerType::LocalImportFolder => vec![
                        "any custom import options for the particular import folder",
                        "import folder",
                        "local hard drive import",
                        "global",
                    ],
                    CallerType::LocalImport => vec![
                        "any custom import options for the particular local import page",
                        "local hard drive import",
                        "global",
                    ],
                    CallerType::ClientApi => vec!["client api", "global"],
                    _ => return None,
                };
                (
                    format!(
                        "The stack for {}, from first- to last-checked, is:",
                        caller.name()
                    ),
                    tail.into_iter().map(str::to_owned).collect::<Vec<_>>(),
                )
            }
            Target::Url(key) => {
                let class = self
                    .classes
                    .iter()
                    .find(|class| hex::encode(&class.key) == *key)?;
                let mut lines = if class.url_type == UrlType::Watchable {
                    vec![
                        "any custom import options for the particular watcher page".into(),
                        format!("urls of class \"{}\"", class.name),
                        "watchable urls".into(),
                    ]
                } else {
                    vec!["any custom import options for the particular downloader page or subscription".into(),format!("urls of class \"{}\"",class.name),"maybe \"subscription\"".into(),"gallery/post urls".into()]
                };
                lines.push("global".into());
                ("The stack for when this url class is encountered, from first- to last-checked, is:".into(),lines)
            }
            Target::Favourite(_) => return None,
        };
        Some(format!("{intro}\n\n{}", lines.join("\n")))
    }
}

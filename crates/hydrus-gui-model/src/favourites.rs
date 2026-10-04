//! Favourite searches, as the reference keeps them: the star button's menu
//! (`_FavouriteSearchesMenu` over `GetNestedFoldersToNames`), and the
//! dialogs that manage them (`EditFavouriteSearchesPanel`) and edit one
//! (`EditFavouriteSearchPanel`).

use std::collections::BTreeMap;

use hydrus_core::pages::{FavouriteSearch, PageCollect, PageSort};

use crate::main_menu::{Command, Entry};

/// What the star button's menu does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    /// Open the manage dialog.
    Manage,
    /// Open it to save the page's search.
    Save,
    /// Load favourite `n` (among those the menu was made from).
    Load(usize),
}

#[derive(Default)]
struct Folder<'a> {
    /// (name, index) of the searches here.
    searches: Vec<(&'a str, usize)>,
    folders: BTreeMap<&'a str, Folder<'a>>,
}

/// The star button's menu: managing the favourites and saving the page's
/// search, then, if there are any, the favourites, each loading its search.
/// A folder name with `/` in it nests; at each level its searches come
/// first, by name, then its subfolders, by name (in code point order, as
/// Python sorts: capitals first).
pub fn menu(favourites: &[FavouriteSearch]) -> Vec<Entry> {
    let item = |label: &str, action| Entry::Item {
        label: label.to_owned(),
        command: Some(Command::Favourite(action)),
        enabled: true,
    };
    let mut entries = vec![
        item("manage favourite searches", Action::Manage),
        Entry::Separator,
        item("save this search", Action::Save),
    ];
    if favourites.is_empty() {
        return entries;
    }
    let mut top = Folder::default();
    for (index, favourite) in favourites.iter().enumerate() {
        let mut folder = &mut top;
        for part in favourite
            .folder
            .as_deref()
            .unwrap_or_default()
            .split('/')
            .filter(|p| !p.is_empty())
        {
            folder = folder.folders.entry(part).or_default();
        }
        folder.searches.push((&favourite.name, index));
    }
    entries.push(Entry::Separator);
    entries.extend(folder_entries(&mut top));
    entries
}

fn folder_entries(folder: &mut Folder<'_>) -> Vec<Entry> {
    // (a stable sort, as Python's, keeping stored order among equal names)
    folder.searches.sort_by_key(|&(name, _)| name);
    let mut entries: Vec<Entry> = folder
        .searches
        .iter()
        .map(|&(name, index)| Entry::Item {
            label: name.to_owned(),
            command: Some(Command::Favourite(Action::Load(index))),
            enabled: true,
        })
        .collect();
    for (name, sub) in &mut folder.folders {
        entries.push(Entry::Menu {
            label: (*name).to_owned(),
            entries: folder_entries(sub),
            enabled: true,
        });
    }
    entries
}

/// The manage dialog's columns.
pub const COLUMNS: [&str; 5] = ["folder", "name", "search", "sort", "collect"];

/// How the manage dialog writes a favourite's search, sort and collect.
pub struct Describe<'a> {
    pub predicates: &'a dyn Fn(&FavouriteSearch) -> Vec<String>,
    pub sort: &'a dyn Fn(&PageSort) -> String,
    pub collect: &'a dyn Fn(&PageCollect) -> String,
}

impl std::fmt::Debug for Describe<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Describe").finish_non_exhaustive()
    }
}

/// A favourite as the manage dialog lists it (`_ConvertRowToDisplayTuple`):
/// its folder, name, predicates, and sort and collect if it keeps them.
pub fn display_row(favourite: &FavouriteSearch, describe: &Describe<'_>) -> [String; 5] {
    [
        favourite.folder.clone().unwrap_or_default(),
        favourite.name.clone(),
        (describe.predicates)(favourite).join(", "),
        favourite
            .sort
            .as_ref()
            .map(|s| (describe.sort)(s))
            .unwrap_or_default(),
        favourite
            .collect
            .as_ref()
            .map(|c| (describe.collect)(c))
            .unwrap_or_default(),
    ]
}

/// A collect as the reference writes it (`MediaCollect.ToString`): its
/// namespaces, then its rating services' names (those that still exist),
/// or "no collections".
pub fn collect_text(
    collect: &PageCollect,
    service_name: &dyn Fn(&hydrus_core::ServiceKey) -> Option<String>,
) -> String {
    let mut parts = collect.namespaces.clone();
    parts.extend(collect.ratings.iter().filter_map(service_name));
    if parts.is_empty() {
        "no collections".to_owned()
    } else {
        parts.join(", ")
    }
}

/// The order the manage dialog lists `rows` in, sorted on `column`
/// (`HydrusListItemModel.sort`): by that column's text casefolded, then by
/// the whole row's; descending reverses it, stably.
pub fn list_order(rows: &[[String; 5]], column: usize, ascending: bool) -> Vec<usize> {
    let keys: Vec<(String, Vec<String>)> = rows
        .iter()
        .map(|row| {
            let folded: Vec<String> = row.iter().map(|t| casefold(t)).collect();
            (folded.get(column).cloned().unwrap_or_default(), folded)
        })
        .collect();
    let mut order: Vec<usize> = (0..rows.len()).collect();
    if ascending {
        order.sort_by(|&a, &b| keys[a].cmp(&keys[b]));
    } else {
        order.sort_by(|&a, &b| keys[b].cmp(&keys[a]));
    }
    order
}

/// Python's `str.casefold`, near enough: lowercase, with "ß" as "ss".
fn casefold(text: &str) -> String {
    text.to_lowercase().replace('ß', "ss")
}

/// `name`, or with " (1)", " (2)" and so on after it, the first that isn't
/// `taken` (`GetNonDupeName`).
pub fn non_dupe_name(name: &str, taken: &dyn Fn(&str) -> bool) -> String {
    let mut candidate = name.to_owned();
    let mut i = 1;
    while taken(&candidate) {
        candidate = format!("{name} ({i})");
        i += 1;
    }
    candidate
}

/// The search "add" starts the edit dialog with (`_AddNewFavouriteSearch`
/// with no row): "new favourite search", in no folder, on `location` (the
/// default local file domain), searching as it changes, with no sort or
/// collect.
pub fn new_search(location: hydrus_core::search::context::LocationContext) -> FavouriteSearch {
    FavouriteSearch {
        folder: None,
        name: "new favourite search".into(),
        search: hydrus_core::search::context::FileSearchContext {
            location,
            ..Default::default()
        },
        synchronised: true,
        sort: None,
        collect: None,
    }
}

/// The favourites in the manage dialog, as it changes them before "apply"
/// keeps them.
#[derive(Debug, Clone)]
pub struct Manager {
    pub rows: Vec<FavouriteSearch>,
}

impl Manager {
    pub fn new(rows: Vec<FavouriteSearch>) -> Self {
        Self { rows }
    }

    /// Each folder's searches' names (`_GetExistingFoldersToNames`).
    pub fn existing(&self) -> BTreeMap<Option<String>, Vec<String>> {
        let mut out: BTreeMap<Option<String>, Vec<String>> = BTreeMap::new();
        for row in &self.rows {
            out.entry(row.folder.clone())
                .or_default()
                .push(row.name.clone());
        }
        out
    }

    /// What the edit dialog starts with to add `row` (`_AddNewFavouriteSearch`):
    /// its name made one no search in any folder has.
    pub fn to_add(&self, mut row: FavouriteSearch) -> FavouriteSearch {
        row.name = non_dupe_name(&row.name, &|n| self.rows.iter().any(|r| r.name == n));
        row
    }

    /// The first search in `folder` named `name` gone (`_DeleteRow`).
    fn delete_row(&mut self, folder: Option<&str>, name: &str) {
        if let Some(i) = self
            .rows
            .iter()
            .position(|r| r.folder.as_deref() == folder && r.name == name)
        {
            self.rows.remove(i);
        }
    }

    /// The edit dialog kept a new search: it replaces any of its folder and
    /// name.
    pub fn added(&mut self, row: FavouriteSearch) {
        self.delete_row(row.folder.as_deref(), &row.name);
        self.rows.push(row);
    }

    /// The edit dialog kept search `index` as `row` (`_EditFavouriteSearch`):
    /// the old one goes, and any other of the new folder and name.
    pub fn edited(&mut self, index: usize, row: FavouriteSearch) {
        let Some(old) = self.rows.get(index).cloned() else {
            return;
        };
        self.delete_row(old.folder.as_deref(), &old.name);
        self.delete_row(row.folder.as_deref(), &row.name);
        self.rows.push(row);
    }

    /// The searches at `indices` gone.
    pub fn delete(&mut self, indices: &[usize]) {
        let mut index = 0;
        self.rows.retain(|_| {
            let keep = !indices.contains(&index);
            index += 1;
            keep
        });
    }
}

/// A search in the edit dialog: its folder and name as typed, its search,
/// and its sort and collect with whether to keep them.
#[derive(Debug, Clone)]
pub struct Edit {
    pub folder: String,
    pub name: String,
    pub search: hydrus_core::search::context::FileSearchContext,
    pub synchronised: bool,
    pub sort: PageSort,
    pub save_sort: bool,
    pub collect: PageCollect,
    pub save_collect: bool,
    original: (Option<String>, String),
}

impl Edit {
    /// The dialog on `row`; a sort or collect it doesn't keep shows as
    /// `default_sort` or `default_collect`, as the reference's controls
    /// start on the options' defaults.
    pub fn new(
        row: &FavouriteSearch,
        default_sort: &PageSort,
        default_collect: &PageCollect,
    ) -> Self {
        Self {
            folder: row.folder.clone().unwrap_or_default(),
            name: row.name.clone(),
            search: row.search.clone(),
            synchronised: row.synchronised,
            sort: row.sort.clone().unwrap_or_else(|| default_sort.clone()),
            save_sort: row.sort.is_some(),
            collect: row
                .collect
                .clone()
                .unwrap_or_else(|| default_collect.clone()),
            save_collect: row.collect.is_some(),
            original: (row.folder.clone(), row.name.clone()),
        }
    }

    /// Choose a file domain with the shared search-domain interlocks.
    pub fn choose_location(
        &mut self,
        services: &hydrus_store::services::ServiceRegistry,
        location: hydrus_search::LocationContext,
    ) {
        let mut domains = crate::domains::Domains {
            location: self.search.location.clone(),
            tags: self.search.tags.clone(),
        };
        domains.choose_location(services, location);
        self.search.location = domains.location;
        self.search.tags = domains.tags;
    }

    /// Choose a tag domain, restoring local files when combined tags cannot search all files.
    pub fn choose_tags(
        &mut self,
        service: hydrus_core::ServiceKey,
        default_location: &hydrus_search::LocationContext,
    ) {
        let mut domains = crate::domains::Domains {
            location: self.search.location.clone(),
            tags: self.search.tags.clone(),
        };
        domains.choose_tags(service, default_location);
        self.search.location = domains.location;
        self.search.tags = domains.tags;
    }

    /// Choose a sort type using its reference default direction.
    pub fn choose_sort(&mut self, choice: &crate::sort::PageChoice) {
        self.sort = PageSort {
            by: choice.by.clone(),
            ascending: choice.default_ascending,
        };
    }

    /// What the dialog gives back (`GetValue`): no folder for a blank one.
    pub fn value(&self) -> FavouriteSearch {
        FavouriteSearch {
            folder: Some(self.folder.clone()).filter(|f| !f.is_empty()),
            name: self.name.clone(),
            search: self.search.clone(),
            synchronised: self.synchronised,
            sort: self.save_sort.then(|| self.sort.clone()),
            collect: self.save_collect.then(|| self.collect.clone()),
        }
    }

    /// What to ask before "apply", if this would overwrite another search
    /// (`UserIsOKToOK`): renamed onto a folder and name that `existing` has.
    pub fn overwrite_question(
        &self,
        existing: &BTreeMap<Option<String>, Vec<String>>,
    ) -> Option<String> {
        let value = self.value();
        if (&value.folder, &value.name) == (&self.original.0, &self.original.1) {
            return None;
        }
        if !existing
            .get(&value.folder)
            .is_some_and(|names| names.contains(&value.name))
        {
            return None;
        }
        let folder = value
            .folder
            .map(|f| format!(" under folder \"{f}\""))
            .unwrap_or_default();
        Some(format!(
            "The search \"{}\"{folder} already exists! Do you want to overwrite it?",
            value.name
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_are_made_unique_as_the_reference_makes_them() {
        let taken = ["a", "a (1)", "b (1)"];
        let taken = |n: &str| taken.contains(&n);
        assert_eq!(non_dupe_name("a", &taken), "a (2)");
        assert_eq!(non_dupe_name("b", &taken), "b");
        assert_eq!(non_dupe_name("b (1)", &taken), "b (1) (1)");
    }
}

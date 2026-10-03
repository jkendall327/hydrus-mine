//! The manage import folders and manage export folders dialogs (file >
//! import/export folders), as the reference's `EditImportFoldersPanel`
//! and `EditExportFoldersPanel` with their edit dialogs
//! (`EditImportFolderPanel`, `EditExportFolderPanel`): the lists' rows, a
//! list of named things held until "apply" ([`Named`]), and what the edit
//! dialogs refuse, warn of and ask on "apply". Recorded by
//! `oracle/record_folders_lists.py` and `oracle/record_folders_dialogs.py`.

use hydrus_core::time::pretty_time_delta;
use hydrus_parse::folders::{ExportFolder, ExportType, FolderAction, ImportFolderSettings};

use crate::list_selection::ListSelection;

/// The import folders list's column titles.
pub const IMPORT_COLUMNS: [&str; 4] = ["name", "path", "paused", "check period"];

/// The export folders list's column titles.
pub const EXPORT_COLUMNS: [&str; 7] = [
    "name",
    "path",
    "type",
    "query",
    "period",
    "phrase",
    "recent error?",
];

/// What the import folders dialog says above its list.
pub const IMPORT_INTRO: &str =
    "Here you can set the client to regularly check certain folders for new files to import.";

/// The import folders dialog's warning.
pub const IMPORT_WARNING: &str = "WARNING: Import folders check (and potentially move/delete!) the contents of all subdirectories as well as the base directory!";

/// What the export folders dialog says above its list.
pub const EXPORT_INTRO: &str =
    "Here you can set the client to regularly export a certain query to a particular location.";

/// An import folder's row (`_ConvertImportFolderToDisplayTuple`).
pub fn import_folder_row(name: &str, paused: bool, settings: &ImportFolderSettings) -> Vec<String> {
    vec![
        name.to_owned(),
        settings.path.clone(),
        if paused { "yes" } else { "" }.to_owned(),
        if settings.check_regularly {
            pretty_time_delta(settings.period, false)
        } else {
            "not checking regularly".to_owned()
        },
    ]
}

/// An export folder's row (`_ConvertExportFolderToDisplayTuple`), its
/// query's predicates as written.
pub fn export_folder_row(folder: &ExportFolder, predicates: &[String]) -> Vec<String> {
    let mut export_type = match folder.export_type {
        ExportType::Regular => "regular",
        ExportType::Synchronise => "synchronise",
    }
    .to_owned();
    if folder.delete_from_client_after_export {
        export_type.push_str(" and deleting from the client!");
    }
    let mut period = if folder.run_regularly {
        pretty_time_delta(folder.period, false)
    } else {
        "not running regularly".to_owned()
    };
    if folder.run_now {
        period.push_str(" (running after dialog ok)");
    }
    vec![
        folder.name.clone(),
        folder.path.clone(),
        export_type,
        predicates.join(", "),
        period,
        folder.phrase.clone(),
        folder.last_error.clone(),
    ]
}

/// Something a list holds by a name no other has, casefolded.
pub trait Name {
    fn name(&self) -> &str;
    fn set_name(&mut self, name: String);
}

impl Name for ExportFolder {
    fn name(&self) -> &str {
        &self.name
    }

    fn set_name(&mut self, name: String) {
        self.name = name;
    }
}

/// An import folder as the dialogs hold it: its name, paused state and
/// settings, and its id in the store if it is there yet.
#[derive(Debug, Clone, PartialEq)]
pub struct ImportFolderEdit {
    pub id: Option<i64>,
    pub name: String,
    pub paused: bool,
    pub settings: ImportFolderSettings,
    /// Its own import options.
    pub options: hydrus_core::import_options::ImportOptionsSlice,
}

impl ImportFolderEdit {
    /// A new folder, as "add" starts it (`ImportFolder('import folder')`).
    pub fn new_folder() -> Self {
        Self {
            id: None,
            name: "import folder".into(),
            paused: false,
            settings: ImportFolderSettings::default(),
            options: hydrus_core::import_options::ImportOptionsSlice::default(),
        }
    }
}

impl Name for ImportFolderEdit {
    fn name(&self) -> &str {
        &self.name
    }

    fn set_name(&mut self, name: String) {
        self.name = name;
    }
}

/// A list dialog's things, held until "apply", sorted by name and
/// selected as the reference's lists select; added and edited ones are
/// named so no other has their name (`SetNonDupeName`, casefolded).
#[derive(Debug, Clone)]
pub struct Named<T> {
    items: Vec<(u64, T)>,
    pub selection: ListSelection<u64>,
    next_key: u64,
}

impl<T: Name> Named<T> {
    pub fn new(items: Vec<T>) -> Self {
        let mut list = Self {
            items: Vec::new(),
            selection: ListSelection::default(),
            next_key: 0,
        };
        for item in items {
            list.push(item);
        }
        list
    }

    fn push(&mut self, item: T) -> u64 {
        let key = self.next_key;
        self.next_key += 1;
        self.items.push((key, item));
        key
    }

    pub fn get(&self, key: u64) -> Option<&T> {
        self.items.iter().find(|(k, _)| *k == key).map(|(_, t)| t)
    }

    /// The keys, by name (casefolded).
    pub fn order(&self) -> Vec<u64> {
        let mut keyed: Vec<(String, u64)> = self
            .items
            .iter()
            .map(|(k, t)| (hydrus_core::casefold::casefold(t.name()), *k))
            .collect();
        keyed.sort();
        keyed.into_iter().map(|(_, k)| k).collect()
    }

    /// The things, in the list's order.
    pub fn in_order(&self) -> Vec<(u64, &T)> {
        self.order()
            .into_iter()
            .filter_map(|k| self.get(k).map(|t| (k, t)))
            .collect()
    }

    pub fn click(&mut self, row: usize, ctrl: bool, shift: bool) {
        let order = self.order();
        self.selection.click(&order, row, ctrl, shift);
    }

    /// The one selected, for "edit".
    pub fn one_selected(&self) -> Option<u64> {
        self.selection.one()
    }

    fn non_dupe_name(&self, name: &str, except: Option<u64>) -> String {
        let taken: Vec<String> = self
            .items
            .iter()
            .filter(|(k, _)| Some(*k) != except)
            .map(|(_, t)| hydrus_core::casefold::casefold(t.name()))
            .collect();
        crate::favourites::non_dupe_name(name, &|n| {
            taken.contains(&hydrus_core::casefold::casefold(n))
        })
    }

    /// Add one ("add"), renamed if its name is taken; it is selected.
    pub fn add(&mut self, mut item: T) -> u64 {
        let name = self.non_dupe_name(item.name(), None);
        item.set_name(name);
        let key = self.push(item);
        self.selection.select_many(&[key]);
        key
    }

    /// Replace one with its edit ("edit"): a changed name is renamed if
    /// another has it.
    pub fn replace(&mut self, key: u64, mut item: T) {
        let unchanged = self.get(key).is_some_and(|t| t.name() == item.name());
        if !unchanged {
            let name = self.non_dupe_name(item.name(), Some(key));
            item.set_name(name);
        }
        if let Some((_, t)) = self.items.iter_mut().find(|(k, _)| *k == key) {
            *t = item;
        }
    }

    /// Delete the selected (after "Remove all selected?"); what was
    /// deleted.
    pub fn delete_selected(&mut self) -> Vec<T> {
        let doomed = self.selection.in_order(&self.order());
        let mut deleted = Vec::new();
        for key in doomed {
            self.selection.forget(key);
            if let Some(i) = self.items.iter().position(|(k, _)| *k == key) {
                deleted.push(self.items.remove(i).1);
            }
        }
        deleted
    }

    /// Everything, as "apply" gives it back.
    pub fn into_items(self) -> Vec<T> {
        self.items.into_iter().map(|(_, t)| t).collect()
    }
}

/// The import folder edit dialog's answers for what to do with a file
/// (`import_folder_string_lookup`), in its order.
pub const ACTION_CHOICES: [&str; 3] = [
    "delete the source file",
    "leave the source file alone, do not reattempt it",
    "move the source file",
];

/// Which of [`ACTION_CHOICES`] an action is.
pub fn action_index(action: &FolderAction) -> usize {
    match action {
        FolderAction::Delete => 0,
        FolderAction::Ignore => 1,
        FolderAction::Move(_) => 2,
    }
}

/// The outcomes an import folder acts on, as its edit dialog words them
/// and as its move-location errors name them.
pub const OUTCOMES: [(&str, &str); 4] = [
    ("when a file imports successfully: ", "successful"),
    ("when a file is already in the db: ", "redundant"),
    (
        "when a file has previously been deleted from the db: ",
        "deleted",
    ),
    ("when a file fails to import: ", "failed"),
];

/// Where hydrus keeps its own files, which an import folder mustn't
/// read from (`GetImportSensitiveDirectories`): the store's directory (an
/// import folder may be inside it) and the file storage locations (it may
/// be neither inside nor around them).
#[derive(Debug, Clone, Default)]
pub struct Sensitive {
    pub allow_inside: Vec<String>,
    pub untouchable: Vec<String>,
}

/// What "apply" on the import folder edit dialog finds (`_CheckValid`):
/// the error that refuses it, or the warnings to show (missing
/// directories), given whether a path exists.
pub fn check_import_folder(
    folder: &ImportFolderEdit,
    sensitive: &Sensitive,
    exists: &dyn Fn(&str) -> bool,
) -> Result<Vec<String>, String> {
    let path = &folder.settings.path;
    if path.is_empty() {
        return Err("You must enter a path to import from!".into());
    }
    let mut warnings = Vec::new();
    if !exists(path) {
        warnings.push(format!("The path you have entered--\"{path}\"--does not exist! The dialog will not force you to correct it, but this import folder will do no work as long as the location is missing!"));
    }
    for sensitive_path in sensitive.allow_inside.iter().chain(&sensitive.untouchable) {
        if sensitive_path.starts_with(path.as_str()) {
            return Err(format!(
                "You cannot set an import path that includes certain sensitive directories. The problem directory in this case was \"{sensitive_path}\". Please choose another location."
            ));
        }
        if !sensitive.allow_inside.contains(sensitive_path)
            && path.starts_with(sensitive_path.as_str())
        {
            return Err(format!(
                "You cannot set an import path that is inside certain sensitive directories. The problem directory in this case was \"{sensitive_path}\". Please choose another location."
            ));
        }
    }
    let actions = &folder.settings.actions;
    for (action, (_, which)) in [
        &actions.successful_and_new,
        &actions.successful_but_redundant,
        &actions.deleted,
        &actions.error,
    ]
    .into_iter()
    .zip(OUTCOMES)
    {
        let FolderAction::Move(location) = action else {
            continue;
        };
        if location.is_empty() {
            return Err(format!(
                "You must enter a path for your {which} file move location!"
            ));
        }
        if !exists(location) {
            warnings.push(format!("The path you have entered for your {which} file move location--\"{location}\"--does not exist! The dialog will not force you to correct it, but you should not let this import folder run until you have corrected or created it!"));
        }
    }
    Ok(warnings)
}

/// The export folder edit dialog's text on its types.
pub const EXPORT_TYPES_TEXT: &str = "regular - try to export the files to the directory, overwriting if the filesize if different\nsynchronise - try to export the files to the directory, overwriting if the filesize if different, and delete anything else in the directory\nIf you select synchronise, be careful!";

/// The export folder edit dialog's text on sidecars.
pub const SIDECARS_TEXT: &str = "By default, an export folder will not update pre-existing sidecar files. If you change the sidecar actions here, or if the metadata has changed and you want those updates, hit the \"overwrite all sidecars on next run\" checkbox.\n\nYou can force the export folder to regenerate all your sidecars on every run, but this is an expensive operation. It is only appropriate for an export folder that runs manually or rarely.\n\nDO NOT SET YOUR EXPORT FOLDER TO REGEN ALL YOUR SIDECARS EVERY THIRTY MINUTES.";

/// What ticking "trash files in hydrus client after export" warns.
pub const DELETE_WARNING: &str = "This will delete the exported files from your client (send them to trash) after the export! If you do not know what this means, uncheck it!";

/// What "apply" asks of an export folder that deletes from the client.
pub const DELETE_QUESTION: &str = "You have set this export folder to delete the files from the client (send them to trash) after export! Are you absolutely sure this is what you want?";

/// What "apply" on the export folder edit dialog refuses, if anything
/// (`GetValue`): no path, or a phrase that doesn't parse.
pub fn check_export_folder(folder: &ExportFolder) -> Result<(), String> {
    if folder.path.is_empty() {
        return Err("You must enter a folder path to export to!".into());
    }
    hydrus_parse::folders::parse_export_phrase(&folder.phrase)
        .map(|_| ())
        .map_err(|e| format!("Could not parse that export phrase! {e}"))
}

/// A new export folder, as "add" starts it: named "export folder", every
/// day, the client's export phrase, searching `search`.
pub fn new_export_folder(
    phrase: String,
    search: hydrus_core::search::context::FileSearchContext,
) -> ExportFolder {
    ExportFolder {
        name: "export folder".into(),
        path: String::new(),
        export_type: ExportType::Regular,
        delete_from_client_after_export: false,
        export_symlinks: false,
        search,
        routers: Vec::new(),
        run_regularly: true,
        period: 86400,
        phrase,
        last_checked: 0,
        run_now: false,
        last_error: String::new(),
        show_working_popup: true,
        overwrite_sidecars_on_next_run: false,
        always_overwrite_sidecars: false,
    }
}

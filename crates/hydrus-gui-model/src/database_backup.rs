//! Database > backup's entries (`_SetupBackupPath`, `_BackupDatabase`,
//! `RestoreDatabase`): what they say and ask, and the backup's menu label.

use hydrus_core::time::timestamp_to_pretty_time_delta;

/// A backup entry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    /// "set up a database backup location" or "change database backup
    /// location".
    SetUp,
    Update,
    Restore,
    /// "database is stored in multiple locations": the note.
    Multiple,
}

/// What "set up a database backup location" says first.
pub fn intro(existing: Option<&str>) -> String {
    match existing {
        None => "Everything in your client is stored in the 'database', which consists of a handful of .db files and a single subdirectory that contains all your media files. It is a very good idea to maintain a regular backup schedule--to save from hard drive failure, serious software fault, accidental deletion, or any other unexpected problem. It sucks to lose all your work, so make sure it can't happen!\n\nIf you prefer to create a manual backup with an external program like FreeFileSync, then please cancel out of the dialog after this and set up whatever you like, but if you would rather a simple solution, simply select a directory and the client will remember it as the designated backup location. Creating or updating your backup can be triggered at any time from the database menu.\n\nAn ideal backup location is initially empty and on a different hard drive.\n\nIf you have a large database (100,000+ files) or a slow hard drive, creating the initial backup may take a long time--perhaps an hour or more--but updating an existing backup should only take a couple of minutes (since the client only has to copy new or modified files). Try to update your backup every week!\n\nIf you would like some more info on making or restoring backups, please consult the help's 'installing and updating' page.".to_owned(),
        Some(path) => format!(
            "Your current backup location is \"{path}\".\n\nIf your client is getting large and/or complicated, I recommend you start backing up with a proper external program like FreeFileSync. If you would like some more info on making or restoring backups, please consult the help's 'installing and updating' page."
        ),
    }
}

/// The directory picker's title.
pub const PICK_TITLE: &str = "Select backup location.";

pub const SAME_AS_DATABASE: &str = "That directory is your current database directory! You cannot backup to the same location you are backing up from!";

pub const UNCHANGED: &str =
    "The path you chose is your current saved backup path. No changes have been made.";

/// What a chosen directory holds, for the question about it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Chosen {
    Missing,
    Empty,
    HasDatabase,
    HasFiles,
}

impl Chosen {
    /// Look at `path`; `database_file` is a database's file name there.
    pub fn of(path: &std::path::Path, database_file: &str) -> Self {
        match std::fs::read_dir(path) {
            Err(_) => Self::Missing,
            Ok(entries) => {
                let names: Vec<_> = entries
                    .filter_map(|e| e.ok().map(|e| e.file_name()))
                    .collect();
                if names.is_empty() {
                    Self::Empty
                } else if names.iter().any(|n| n == database_file || n == "client.db") {
                    Self::HasDatabase
                } else {
                    Self::HasFiles
                }
            }
        }
    }
}

/// "Are you sure this is the correct directory?"
pub fn chosen_question(path: &str, chosen: Chosen) -> String {
    let extra = match chosen {
        Chosen::Empty => {
            "It looks currently empty, which is great--there is no danger of anything being overwritten."
        }
        Chosen::HasDatabase => {
            "It looks like a client database already exists in the location--be certain that it is ok to overwrite it."
        }
        Chosen::HasFiles => {
            "It seems to have some files already in it--be careful and make sure you chose the correct location."
        }
        Chosen::Missing => {
            "The path does not exist yet--it will be created when you make your first backup."
        }
    };
    format!(
        "You chose \"{path}\". Here is what I understand about it:\n\n{extra}\n\nAre you sure this is the correct directory?"
    )
}

pub const CREATE_NOW: &str = "Would you like to create your backup now?";

pub const NO_PATH: &str = "No backup path is set!";

pub const CREATING_PATH: &str = "The backup path does not exist--creating it now.";

/// "update database backup"'s question: updating if a database is there.
pub fn update_question(path: &str, exists: bool) -> String {
    let action = if exists {
        "Update the existing"
    } else {
        "Create a new"
    };
    format!(
        "{action} backup at \"{path}\"?\n\nThe database will be locked while the backup occurs, which may lock up your gui as well."
    )
}

/// The backup popup's title, and its last text.
pub const POPUP_TITLE: &str = "backing up db";
pub const COMPLETE: &str = "backup complete!";

/// "update database backup", with when the last was made (`now` and the
/// last in seconds).
pub fn update_label(last: Option<i64>, now: i64) -> String {
    let mut label = "update database backup".to_owned();
    if let Some(last) = last {
        if now < last + 1800 {
            label.push_str(" (did one recently)");
        } else {
            label.push_str(&format!(
                " (last {})",
                timestamp_to_pretty_time_delta(last, now, " ago")
            ));
        }
    }
    label
}

/// The note shown for a database in several locations.
pub const MULTIPLE_LOCATIONS: &str = "Your database is stored across multiple locations. The in-client backup routine can only handle simple databases (in one location), so the menu commands to backup have been hidden. To back up, please use a third-party program that will work better than anything I can write.\n\nCheck the help for more info on how best to backup manually.";

/// "restore from a database backup"'s question.
pub fn restore_question(path: &str) -> String {
    format!(
        "Are you sure you want to restore a backup from \"{path}\"?\n\nEverything in your current database will be deleted!\n\nThe gui will shut down, and then it will take a while to complete the restore. Once it is done, the client will restart."
    )
}

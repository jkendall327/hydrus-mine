//! Database > file maintenance > "clear orphan files" (`_ClearOrphanFiles`,
//! `ClearOrphans`): its question, choices and the popup's last line.

use hydrus_core::numbers::human_int;

pub const QUESTION: &str = "This job will iterate through every file in your database's file storage, extracting any it does not expect to be there. This is particularly useful for 're-syncing' your file storage to what it should be after, say, marrying an older/newer database with a newer/older file storage.\n\nYou can choose to move the orphans in your file directories somewhere or delete them. Orphan thumbnails will be put in a subdirectory, in case you wish to perform reverse lookups.\n\nAccess to files and thumbnails will be slightly limited while this runs, and it may take some time.";

/// The choices, moving first (`yes_tuples`), and the no.
pub const CHOICES: [&str; 2] = ["move them somewhere", "delete them"];
pub const NO: &str = "forget it";

pub const PICK_TITLE: &str = "Select location.";

pub const POPUP_TITLE: &str = "clearing orphans";

/// "found 3 orphan files, now deleting".
pub fn found(count: usize, what: &str) -> String {
    format!(
        "found {} orphan {what}, now deleting",
        human_int(count as u64)
    )
}

/// The popup's last line.
pub fn final_text(files: usize, thumbnails: usize) -> String {
    if files == 0 && thumbnails == 0 {
        "no orphans found!".into()
    } else {
        format!(
            "{} orphan files and {} orphan thumbnails cleared!",
            human_int(files as u64),
            human_int(thumbnails as u64)
        )
    }
}

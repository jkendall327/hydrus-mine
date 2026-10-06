//! Database > locations (`MoveMediaFilesPanel`): the media locations list's
//! rows, which buttons can be used, and what the dialog says and asks.

use hydrus_core::numbers::human_bytes;
use hydrus_store::storage_locations::{Location, Review, ideal_shares};

pub const TITLE: &str = "database locations";

pub const WARNING: &str =
    "THIS IS ADVANCED. DO NOT CHANGE ANYTHING HERE UNLESS YOU KNOW WHAT IT DOES!";

/// The list's columns (`COLUMN_LIST_DB_MIGRATION_LOCATIONS`).
pub const COLUMNS: [&str; 7] = [
    "location",
    "beneath db dir?",
    "disk free space",
    "current usage",
    "weight",
    "max size",
    "ideal usage",
];

/// A share as `FloatToPercentage( f, sf = 2 )` words it.
#[allow(clippy::float_cmp)] // (as the reference tests for a whole number)
pub fn percent(f: f64) -> String {
    let p = f * 100.0;
    if p == p.trunc() {
        #[allow(clippy::cast_possible_truncation)]
        let whole = p as i64;
        format!("{whole}%")
    } else {
        format!("{p:.2}%")
    }
}

/// The thumbnails' estimated total size, low and high
/// (`_GetThumbnailSizeEstimates`), for `files` thumbnails of `width` by
/// `height`.
pub fn thumbnail_estimates(files: u64, width: u32, height: u32) -> (u64, u64) {
    let typical_pixels = 320.0 * 240.0;
    let typical_size = 36.0 * 1024.0;
    #[allow(clippy::cast_precision_loss)]
    let ours =
        typical_size * (f64::from(width) * f64::from(height) / typical_pixels) * files as f64;
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    ((ours * 0.8) as u64, (ours * 1.4) as u64)
}

fn usage(files: f64, thumbs: f64, total: u64, thumbs_max: u64) -> String {
    let mut parts = Vec::new();
    if files > 0.0 {
        parts.push(format!("{} files", percent(files)));
    }
    if thumbs > 0.0 {
        parts.push(format!("{} thumbnails", percent(thumbs)));
    }
    if parts.is_empty() {
        return "nothing".into();
    }
    #[allow(clippy::float_cmp)]
    if files == thumbs {
        parts = vec![format!("{} everything", percent(files))];
    }
    #[allow(
        clippy::cast_precision_loss,
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss
    )]
    let bytes = (files * total as f64 + thumbs * thumbs_max as f64) as u64;
    format!("{} - {}", human_bytes(bytes), parts.join(","))
}

/// The media locations' ideal shares of the files, by location (those
/// with weight, as `STATICGetIdealWeights` takes them).
pub fn ideal(review: &Review) -> Vec<f64> {
    let media: Vec<(i64, Option<i64>)> = review
        .locations
        .iter()
        .map(|l| (l.weight, l.max_bytes))
        .collect();
    ideal_shares(review.total_bytes, &media)
}

/// What the disk under a location says: its free space, or that it isn't
/// there.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Disk {
    Free(u64),
    Unknown,
    Missing,
}

/// A location's row (`_ConvertLocationToDisplayTuple`); `db_dir` decides
/// "beneath db dir?".
pub fn row(
    location: &Location,
    ideal_share: f64,
    review: &Review,
    thumbs_max: u64,
    override_set: bool,
    db_dir: &std::path::Path,
    disk: Disk,
) -> [String; 7] {
    let path = location.path.display().to_string();
    let (pretty_location, free) = match disk {
        Disk::Free(bytes) => (path, human_bytes(bytes)),
        Disk::Unknown => (path, "unknown".into()),
        Disk::Missing => (format!("DOES NOT EXIST: {path}"), "DOES NOT EXIST".into()),
    };
    let portable = if location.path.starts_with(db_dir) {
        "yes"
    } else {
        "no"
    };
    let weight = if location.weight > 0 {
        location.weight.to_string()
    } else if location.files_share > 0.0 {
        "0".into()
    } else {
        "n/a".into()
    };
    let max = location.max_bytes.map_or_else(
        || "n/a".into(),
        |m| human_bytes(u64::try_from(m).unwrap_or(0)),
    );
    let ideal_thumbs = if override_set {
        if location.thumbnail_override {
            1.0
        } else {
            0.0
        }
    } else {
        ideal_share
    };
    [
        pretty_location,
        portable.into(),
        free,
        usage(
            location.files_share,
            location.thumbnails_share,
            review.total_bytes,
            thumbs_max,
        ),
        weight,
        max,
        usage(ideal_share, ideal_thumbs, review.total_bytes, thumbs_max),
    ]
}

/// The locations with weight (the reference's ideal media locations).
fn weighted(review: &Review) -> Vec<&Location> {
    review.locations.iter().filter(|l| l.weight > 0).collect()
}

/// Which buttons work for the selected location: increase, decrease, set
/// max size, remove (`_CanIncreaseWeight`, ...).
pub fn buttons(review: &Review, selected: Option<&Location>) -> [bool; 4] {
    let Some(l) = selected else {
        return [false; 4];
    };
    let media = weighted(review);
    let with = l.weight > 0;
    let several = media.len() > 1;
    let increase = (with && several) || (!with && l.files_share > 0.0);
    let decrease = with && (l.weight > 1 || several);
    let limitless = media.iter().filter(|m| m.max_bytes.is_none()).count();
    let set_max = with && !(l.max_bytes.is_none() && limitless == 1);
    let remove = with && several;
    [increase, decrease, set_max, remove]
}

pub const ALREADY_ENTERED: &str = "You already have that location entered!";
pub const IS_THUMBNAIL_LOCATION: &str =
    "That path is already used as the special thumbnail location--please choose another.";
pub const IS_FILE_LOCATION: &str =
    "That path already exists as a regular file location! Please choose another.";
pub const SELECT_WITH_WEIGHT: &str = "Please select a location with weight.";
pub const CANNOT_EMPTY_ALL: &str = "You cannot empty every single current file location--please add a new place for the files to be moved to and then try again.";
pub const PICK_LOCATION: &str = "Select location.";
pub const PICK_THUMBNAILS: &str = "Select thumbnail location.";

/// "remove location"'s question.
pub fn remove_question(exists: bool, has_files: bool) -> &'static str {
    match (exists, has_files) {
        (true, true) => {
            "Are you sure you want to remove this location? This will schedule all of the files it is currently responsible for to be moved elsewhere."
        }
        (true, false) => "Are you sure you want to remove this location?",
        (false, true) => {
            "This path does not exist, but it seems to have files. This could be a critical error that has occurred while the client is open (maybe a drive unmounting?). I recommend you do not remove it here and instead shut the client down immediately and fix the problem, then restart it, which will run the 'recover missing file locations' routine if needed."
        }
        (false, false) => "This path does not exist. Just checking, but I recommend you remove it.",
    }
}

pub const CLEAR_THUMBNAILS: &str = "Clear the custom thumbnail location? This will schedule all the thumbnail files to migrate back into the regular file locations.";

pub const MAX_SIZE_TITLE: &str = "edit max size";
pub const MAX_SIZE_MESSAGE: &str = "If a location goes over its set size, it will schedule to migrate some files to other locations. At least one media location must have no limit.\n\nThis is not precise (it works on average size, and thumbnails can add a bit), so give it some padding. Also, in general, remember it is not healthy to fill any hard drive more than 90% full.\n\nAlso, this feature is under active development. The client will go over this limit if your collection grows significantly--the only way things rebalance atm are if you click \"move files now\"--but in future I will have things automatically migrate in the background to ensure limits are obeyed. This is just for advanced users to play with for now!";
/// The max size control's starting value when there is none.
pub const MAX_SIZE_DEFAULT: i64 = 100 * 1024 * 1024 * 1024;

/// "move files now" for a location that isn't there.
pub fn missing_path_warning(path: &str) -> String {
    format!(
        "The path \"{path}\" does not exist! Please ensure all the locations on this dialog are valid before trying to rebalance your files."
    )
}

pub const RUNTIME_QUESTION: &str = "Moving files can be a slow and slightly laggy process, with the UI intermittently hanging, which sometimes makes manually stopping a large ongoing job difficult. Would you like to set a max runtime on this job?";

/// The runtime choices: label and seconds (none: indefinitely; -1:
/// custom).
pub const RUNTIMES: [(&str, Option<i64>); 5] = [
    ("run for 10 minutes", Some(600)),
    ("run for 30 minutes", Some(1800)),
    ("run for 1 hour", Some(3600)),
    ("run for custom time", Some(-1)),
    ("run indefinitely", None),
];

/// The granularity line.
pub fn granularity_label(granularity: usize) -> String {
    match granularity {
        2 => "You have a granularity of 2, which means 512 total subfolders.".into(),
        3 => "You have a granularity of 3, which means 8,192 total subfolders.".into(),
        other => format!(
            "Your granularity is \"{other}\", which is not supported!! This is not supposed to happen."
        ),
    }
}

/// The rebalance status line.
pub const fn rebalance_label(work: bool) -> &'static str {
    if work {
        "files need to be moved"
    } else {
        "all files are in their ideal locations"
    }
}

/// The "database components" lines.
pub fn component_lines(
    install: &str,
    db_dir: &str,
    db_bytes: u64,
    media_bytes: u64,
    thumbs: (u64, u64),
) -> [String; 3] {
    [
        format!("install: {install}"),
        format!("database (about {}): {db_dir}", human_bytes(db_bytes)),
        format!(
            "media is {}, thumbnails are estimated at {}-{}",
            human_bytes(media_bytes),
            human_bytes(thumbs.0),
            human_bytes(thumbs.1)
        ),
    ]
}

/// The rebalance popup's title (`DoFileStorageRebalance`) and its last
/// text.
pub const REBALANCE_TITLE: &str = "rebalancing files";
pub const REBALANCE_DONE: &str = "done!";

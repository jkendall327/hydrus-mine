//! Database > db maintenance > review vacuum data (`ReviewVacuumData`):
//! each database file's size, free pages, last vacuum, whether it can be
//! vacuumed and how long it would take, and the question before vacuuming.

use hydrus_core::numbers::{float_to_percentage, human_bytes};
use hydrus_core::time::{pretty_time_delta_f64, timestamp_to_pretty_time_delta};
use hydrus_store::vacuum::{VacuumData, approx_duration, check};

pub const TITLE: &str = "review vacuum data";
pub const LOADING: &str = "loading database data";
pub const INFO: &str = "Vacuuming is essentially an aggressive defrag of a database file. The entire database is copied contiguously to a new file, which then has tightly packed pages and no empty 'free' pages. Note that even a database that currently has no free pages can still _sometimes_ be packed more efficiently, saving up to 40% of space, but there is no easy way to determine this ahead of time (in general, if it has been five years, you might save some space), and the database will bloat back up in time as more work happens.\n\nBecause the new database is tightly packed, it will generally be smaller than the original file. This is currently the only way to truncate a hydrus database file.\n\nVacuuming is an expensive operation. It creates one (temporary) copy of the database file in your db dir. Hydrus cannot operate while it is going on, and it tends to run quite slow, about 10-50MB/s. The main benefit is in truncating the database files after you delete a lot of data, so I recommend you only do it after you delete the PTR or similar. If the db file is more than 2GB and has less than 5% free pages, it is probably not worth doing.";
pub const COLUMNS: [&str; 6] = [
    "name",
    "size",
    "internal free space",
    "last vacuum",
    "can vacuum?",
    "vacuum time estimate",
];
pub const YES: &str = "do it";
pub const NO: &str = "forget it";
pub const POPUP_TITLE: &str = "database maintenance - vacuum";
pub const FAILED: &str = "An attempt to vacuum the database failed.\n\nIf the error is not obvious, please contact the hydrus developer.";

/// "N to M": a twentieth of the estimate to the estimate.
pub fn estimate(size: u64) -> String {
    #[allow(clippy::cast_precision_loss)] // (seconds)
    let seconds = approx_duration(size) as f64;
    format!(
        "{} to {}",
        pretty_time_delta_f64(seconds / 20.0),
        pretty_time_delta_f64(seconds)
    )
}

/// The "can vacuum?" cell: "yes!" or why not.
pub fn can_vacuum(data: &VacuumData, free: Option<u64>) -> Result<&'static str, String> {
    check(data, free).map(|()| "yes!")
}

/// A file's row, `now` in seconds.
pub fn row(data: &VacuumData, free: Option<u64>, now: i64) -> [String; 6] {
    let total = data.total_size();
    let free_size = data.free_size();
    let mut pretty_free = human_bytes(free_size);
    if total > 0 {
        #[allow(clippy::cast_precision_loss)]
        let ratio = free_size as f64 / total as f64;
        pretty_free = format!("{pretty_free} ({})", float_to_percentage(ratio));
    }
    let last = match data.last_vacuumed_ms {
        None | Some(0) => "never done".to_owned(),
        Some(ms) => timestamp_to_pretty_time_delta(ms / 1000, now, " ago"),
    };
    [
        data.name.clone(),
        human_bytes(total),
        pretty_free,
        last,
        can_vacuum(data, free).map_or_else(|e| e, str::to_owned),
        estimate(total),
    ]
}

/// The question before vacuuming the files of `total` bytes.
pub fn question(total: u64) -> String {
    format!(
        "Do vacuum now? Estimated time to vacuum is {}.",
        estimate(total)
    )
}

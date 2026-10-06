//! The file maintenance window's "add new work" tab (`ReviewFileMaintenance`'s
//! new work panel): pick files by a search or the easy-select buttons, pick
//! a job, see its description and queue it.

use hydrus_core::numbers::human_int;
use hydrus_store::file_maintenance::JobType;

pub use crate::thumbnail_maintenance::HUMAN_ORDER as JOBS;

pub const TABS: [&str; 2] = ["scheduled work", "add new work"];
pub const EXPLANATION: &str = "Here you can queue up new file maintenance work. You determine the files to run a job on by loading a normal file search.\n\nOnce your search is set, you need to hit 'run this search' to actually load up the files. Then select the job type to apply and click 'add job'. Click 'see description' for more information on the job.\n\nBe cautious--do not queue up an integrity scan for all your files on a whim! If you don't know what a job does, do not add it!";
pub const SEARCH_BOX: &str = "select files by search";
pub const NO_RESULTS: &str = "no results yet";
pub const LOADING: &str = "loading\u{2026}";
pub const RUN_SEARCH: &str = "run this search";
pub const EASY_BOX: &str = "easy select";
pub const ALL_MEDIA: &str = "all media files";
pub const ALL_UPDATES: &str = "all repository update files";
pub const ACTION_BOX: &str = "add job";
pub const NONE_SELECTED: &str = "no files selected yet";
pub const SEE_DESCRIPTION: &str = "see description";
pub const ADD_JOB: &str = "add job";
pub const ADDED: &str = "Jobs added!";
pub const YES: &str = "do it";
pub const NO: &str = "forget it";

/// The search's label once it has run.
pub fn found(n: usize) -> String {
    format!("{} files found", human_int(n as u64))
}

/// The action box's label once files are chosen.
pub fn selected(n: usize) -> String {
    format!("{} files selected", human_int(n as u64))
}

/// The job choices, as the dropdown lists them.
pub fn job_labels() -> Vec<&'static str> {
    JOBS.iter().map(|job| job.description()).collect()
}

/// "see description": the job's description and weight.
pub fn description(job: JobType) -> String {
    format!(
        "{}\n\nThis job has weight {}, where a normalised unit of file work has value {}.",
        crate::thumbnail_maintenance::description(job),
        human_int(job.weight()),
        human_int(100)
    )
}

/// "add job" on more than 1,000 files asks first.
pub fn schedule_question(job: JobType, files: usize) -> Option<String> {
    (files > 1000).then(|| {
        format!(
            "Are you sure you want to schedule \"{}\" on {} files?",
            job.description(),
            human_int(files as u64)
        )
    })
}

/// Where a search for new work looks: the typed search in the default
/// local file domain, or an easy-select button's domain.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Pick {
    Search,
    AllMedia,
    AllUpdates,
}

/// The files `pick` selects, with `predicates` for a search (none is
/// everything, as the reference forces `system:everything`), without the
/// implicit search limit.
pub fn find(
    store: &hydrus_store::Store,
    pick: Pick,
    predicates: &[hydrus_search::Predicate],
) -> hydrus_store::Result<Vec<hydrus_core::HashId>> {
    use hydrus_core::search::context::{FileSearchContext, LocationContext};
    use hydrus_core::service::builtin_keys;
    use hydrus_search::{Predicate, SystemPredicate};
    let snapshot = store.snapshot();
    let key = |k: &[u8]| LocationContext::single(hydrus_core::ServiceKey::new(k.to_vec()));
    let location = match pick {
        Pick::Search => store
            .read(hydrus_store::settings::get::<hydrus_store::settings::SearchDefaults>)?
            .resolved_local_location(&snapshot.services),
        Pick::AllMedia => key(builtin_keys::COMBINED_LOCAL_FILE_DOMAINS),
        Pick::AllUpdates => key(builtin_keys::LOCAL_UPDATE),
    };
    let mut search = FileSearchContext {
        location,
        ..FileSearchContext::default()
    };
    search.predicates = if pick == Pick::Search && !predicates.is_empty() {
        predicates.to_vec()
    } else {
        vec![Predicate::System(SystemPredicate::Everything)]
    };
    store.read(|conn| {
        hydrus_search::search_files_without_implicit_limit(
            conn,
            &snapshot,
            &search,
            hydrus_search::FileSort::default(),
            &hydrus_search::Clock::system(),
        )
        .map_err(|e| hydrus_store::StoreError::Invalid(e.to_string()))
    })
}

/// "add job": queue `job` on `files`, due now.
pub fn schedule(
    store: &hydrus_store::Store,
    files: Vec<hydrus_core::HashId>,
    job: JobType,
) -> hydrus_store::Result<()> {
    store.write(move |ctx| hydrus_store::file_maintenance::add_jobs(ctx.conn(), &files, job, 0))
}

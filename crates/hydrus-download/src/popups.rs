//! Popup messages the daemon's work shows the user (the reference's
//! `HydrusData.ShowText` and `ClientImporting.PublishPresentationHashes`),
//! added to the store's popup queue for the client to show.

use hydrus_core::Sha256;
use hydrus_core::import_options::PresentationOptions;
use hydrus_store::Store;
use hydrus_store::popups::{self, Job};
use hydrus_store::queues::{FileSeed, SeedStatus};

fn now() -> f64 {
    hydrus_core::time::TimestampMs::now().millis() as f64 / 1000.0
}

fn add(store: &Store, job: Job) {
    let at = now().floor() as i64;
    if let Err(e) = store.write(move |ctx| popups::add(ctx.conn(), &job, at)) {
        tracing::error!(error = %e, "could not show a popup");
    }
}

/// `HydrusData.ShowText`: a message, done from the start.
pub fn show_text(store: &Store, text: impl Into<String>) {
    add(store, Job::text(text, now()));
}

/// `HydrusData.ShowException`: an error, titled as the reference titles
/// the plain exceptions it raises, with its first line as the text and the
/// whole of it behind "show traceback".
pub fn show_exception(store: &Store, error: impl Into<String>) {
    let error = error.into();
    let first = error.lines().next().unwrap_or("Exception").to_owned();
    let mut job = Job::text(first, now());
    job.status_title = Some("Exception".into());
    job.traceback = Some(error);
    add(store, job);
}

/// What was being done when an error happened, then the error: two
/// popups, as the reference shows them (`ShowText`, `ShowException`).
pub fn show_error(store: &Store, text: impl Into<String>, error: impl Into<String>) {
    show_text(store, text);
    show_exception(store, error);
}

/// `PublishPresentationHashes`, to its popup button: files an importer
/// presents, as a popup with a button to show them, which joins one of
/// the same label.
pub fn publish_presented(store: &Store, label: &str, hashes: Vec<Sha256>) {
    if hashes.is_empty() {
        return;
    }
    let mut job = Job::new(false, false, now());
    job.attached_files_mergable = true;
    job.set_files(hashes, Some(label.to_owned()));
    add(store, job);
}

/// The file a seed just imported, if the importer presents it
/// (`FileSeed.ShouldPresent`, for a file just imported).
pub fn presented_file(
    store: &Store,
    seed: &FileSeed,
    options: &PresentationOptions,
) -> Option<Sha256> {
    if !seed.status.is_successful() {
        return None;
    }
    let hash: Sha256 = seed.meta.hash("sha256")?.parse().ok()?;
    let new = seed.status == SeedStatus::SuccessfulAndNew;
    let inbox = || {
        store
            .read(|conn| {
                let Some(id) = hydrus_store::master::hash_id(conn, &hash)? else {
                    return Ok(false);
                };
                Ok(hydrus_store::media::inboxed(conn, &[id])?.contains(&id))
            })
            .unwrap_or(false)
    };
    options.presents(new, inbox).then_some(hash)
}

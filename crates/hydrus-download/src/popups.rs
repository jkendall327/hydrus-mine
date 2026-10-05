//! Popup messages the daemon's work shows the user (the reference's
//! `HydrusData.ShowText` and `ClientImporting.PublishPresentationHashes`),
//! added to the store's popup queue for the client to show.

use std::sync::{
    Arc, Weak,
    atomic::{AtomicBool, Ordering},
};
use std::time::{Duration, Instant};

use parking_lot::Mutex;

use hydrus_core::Sha256;
use hydrus_core::import_options::PresentationOptions;
use hydrus_store::Store;
use hydrus_store::popups::{self, Job};
use hydrus_store::queues::{FileSeed, SeedStatus};

fn now() -> f64 {
    hydrus_core::time::TimestampMs::now().millis() as f64 / 1000.0
}

fn now_whole() -> i64 {
    now().floor() as i64
}

fn add(store: &Store, job: Job) {
    let at = now_whole();
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

/// How often a working popup's changes reach the store, and it looks for
/// the client's cancel (the client looks four times a second).
const SYNC_EVERY: Duration = Duration::from_millis(250);

/// A popup for work going on: a `JobStatus` the reference sends the popup
/// manager as it works. What the work says (its title, texts, gauges,
/// files and download) reaches the store's queue at most four times a
/// second once it is shown, and a cancel from the client reaches the work.
/// Dropped unfinished, it finishes and goes, as the reference's work
/// finishes its popup however it stops.
#[derive(Debug)]
pub struct Working {
    store: Arc<Store>,
    state: Mutex<WorkingState>,
    dispatching: AtomicBool,
}
struct DispatchGuard<'a>(&'a AtomicBool);
impl Drop for DispatchGuard<'_> {
    fn drop(&mut self) {
        self.0.store(false, Ordering::Release);
    }
}

#[derive(Clone)]
struct Callback(Arc<dyn Fn() + Send + Sync>);
impl std::fmt::Debug for Callback {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Callback").finish_non_exhaustive()
    }
}

#[derive(Debug)]
struct WorkingState {
    job: Job,
    shown: bool,
    /// The work has finished with it.
    ended: bool,
    /// Gone from the queue: nothing more is written.
    gone: bool,
    /// Changed since the store last had it.
    dirty: bool,
    synced: Option<Instant>,
    network: Option<Arc<hydrus_net::Job>>,
    /// A text (1 or 2) that says, after a prefix, what the download's
    /// importer says it is doing, as it says it (a status hook), and how
    /// many times it had said so when this began.
    following: Option<(u8, String, u64)>,
    callable: Option<Callback>,
    answer: Option<bool>,
    had_actions: bool,
    polling: bool,
}

impl Working {
    /// A popup titled `title`, not shown yet; the client can cancel it if
    /// `cancellable`.
    pub fn new(store: &Arc<Store>, title: impl Into<String>, cancellable: bool) -> Arc<Self> {
        let mut job = Job::new(false, cancellable, now());
        job.status_title = Some(title.into());
        job.action_owner = Some(rand::random());
        Arc::new(Self {
            store: Arc::clone(store),
            dispatching: AtomicBool::new(false),
            state: Mutex::new(WorkingState {
                job,
                shown: false,
                ended: false,
                gone: false,
                dirty: false,
                synced: None,
                network: None,
                following: None,
                callable: None,
                answer: None,
                had_actions: false,
                polling: false,
            }),
        })
    }

    /// Show it (once).
    pub fn show(&self) {
        let mut state = self.state.lock();
        if state.shown || state.ended {
            return;
        }
        state.shown = true;
        state.job.network_job = downloading(state.network.as_deref());
        let job = state.job.clone();
        if let Err(error) = self.store.write(move |ctx| {
            hydrus_store::popup_actions::begin(
                ctx.conn(),
                &job.key,
                &job.action_owner.expect("live popup owner"),
            )?;
            popups::add(ctx.conn(), &job, now_whole())
        }) {
            tracing::error!(error = %error, "could not show live popup");
            state.gone = true;
        }
        state.dirty = false;
        state.synced = Some(Instant::now());
    }

    pub fn is_shown(&self) -> bool {
        self.state.lock().shown
    }

    fn change(&self, f: impl FnOnce(&mut Job)) {
        let mut state = self.state.lock();
        if state.ended {
            return;
        }
        f(&mut state.job);
        state.dirty = true;
        self.sync(&mut state, false);
    }

    /// A label and full clipboard payload; never truncate the producer's text.
    pub fn set_clipboard(&self, clipboard: Option<(String, String)>) {
        let mut state = self.state.lock();
        state.job.popup_clipboard = clipboard;
        state.dirty = true;
        self.sync(&mut state, true);
    }

    /// Replace the current question, invalidating answers for an earlier question.
    pub fn set_question(self: &Arc<Self>, question: Option<String>) {
        let mut state = self.state.lock();
        state.answer = None;
        state.had_actions |= question.is_some();
        state.job.popup_yes_no_question = question.map(|text| (rand::random(), text));
        state.dirty = true;
        self.sync(&mut state, true);
        drop(state);
        self.start_action_polling();
    }

    /// A repeatable producer-owned callable; execution occurs outside the state lock.
    pub fn set_user_callable(
        self: &Arc<Self>,
        label: impl Into<String>,
        call: impl Fn() + Send + Sync + 'static,
    ) {
        let mut state = self.state.lock();
        state.had_actions = true;
        state.callable = Some(Callback(Arc::new(call)));
        state.job.user_callable_label = Some(label.into());
        state.dirty = true;
        self.sync(&mut state, true);
        drop(state);
        self.start_action_polling();
    }
    pub fn clear_user_callable(&self) {
        let mut state = self.state.lock();
        state.callable = None;
        state.job.user_callable_label = None;
        state.dirty = true;
        self.sync(&mut state, true);
    }

    /// Consume an answer after its popup row has been finished and dismissed.
    pub fn question_answer(&self) -> Option<bool> {
        self.poll_actions();
        self.state.lock().answer
    }

    /// Drain GUI/API intents for this live producer, including finished callables.
    pub fn poll_actions(&self) {
        if self
            .dispatching
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .is_err()
        {
            return;
        }
        // Serial effects across workers; a callback may flush/reenter without blocking.
        let _dispatch = DispatchGuard(&self.dispatching);
        let (key, owner) = {
            let state = self.state.lock();
            if !state.shown || !state.had_actions {
                return;
            }
            (state.job.key, state.job.action_owner.expect("live owner"))
        };
        let result = self.store.write(move |ctx| {
            Ok((
                hydrus_store::popup_actions::take_owned(ctx.conn(), &key, &owner)?,
                popups::get(ctx.conn(), &key, now_whole())?,
            ))
        });
        let Ok((requests, current)) = result else {
            return;
        };
        for pending in requests {
            match pending.request {
                hydrus_store::popup_actions::Request::Call => {
                    // Each click reads the current callback, including changes made
                    // reentrantly by an earlier callback from the same batch.
                    let live = self.store.read(|conn| {
                        hydrus_store::popup_actions::can_call(
                            conn,
                            &key,
                            &owner,
                            pending.gui_owner.as_ref(),
                            now_whole(),
                        )
                    });
                    if matches!(live, Ok(true)) {
                        let call = self.state.lock().callable.clone();
                        if let Some(call) = call {
                            (call.0)();
                        }
                    }
                }
                hydrus_store::popup_actions::Request::Answer { question, answer } => {
                    let mut state = self.state.lock();
                    if state
                        .job
                        .popup_yes_no_question
                        .as_ref()
                        .is_some_and(|(token, _)| *token == question)
                    {
                        state.answer = Some(answer);
                        state.job.popup_yes_no_question = None;
                        state.job.finish();
                        state.job.dismissed = true;
                        state.ended = true;
                    }
                }
            }
        }
        if current.is_none() {
            let mut state = self.state.lock();
            state.gone = true;
            state.callable = None;
            state.job.user_callable_label = None;
        }
        if current.is_none() {
            let _ = self
                .store
                .write(move |ctx| hydrus_store::popup_actions::retire(ctx.conn(), &key, &owner));
        }
    }

    /// `SetStatusText` (level 1); `None` deletes it.
    pub fn set_text(&self, text: Option<String>) {
        self.change(|job| job.status_text_1 = text);
    }

    /// `SetStatusText(text, 2)`.
    pub fn set_text_2(&self, text: Option<String>) {
        self.change(|job| job.status_text_2 = text);
    }

    /// `SetGauge` (level 1).
    pub fn set_gauge(&self, gauge: Option<(i64, i64)>) {
        self.change(|job| job.popup_gauge_1 = gauge);
    }

    /// `SetGauge(..., level = 2)`.
    pub fn set_gauge_2(&self, gauge: Option<(i64, i64)>) {
        self.change(|job| job.popup_gauge_2 = gauge);
    }

    /// `SetFiles`, or `DeleteFiles` with none.
    pub fn set_files(&self, hashes: Vec<Sha256>, label: &str) {
        self.change(|job| job.set_files(hashes, Some(label.to_owned())));
    }

    /// `SetNetworkJob`, or `DeleteNetworkJob` with none: the download it
    /// shows.
    pub fn set_network_job(&self, network: Option<Arc<hydrus_net::Job>>) {
        let mut state = self.state.lock();
        state.network = network;
        state.dirty = true;
        self.sync(&mut state, false);
    }

    /// Have text `level` say `prefix`, then what the download's importer
    /// says it is doing, each time it says (the reference's status hooks:
    /// "files 2/5: downloading file").
    pub fn follow_stage(&self, level: u8, prefix: impl Into<String>) {
        let mut state = self.state.lock();
        let said = state.network.as_ref().map_or(0, |n| n.state().stages);
        state.following = Some((level, prefix.into(), said));
    }

    /// Stop following the download's importer.
    pub fn stop_following(&self) {
        self.state.lock().following = None;
    }

    /// Whether the client cancelled it.
    pub fn is_cancelled(&self) -> bool {
        let mut state = self.state.lock();
        self.sync(&mut state, false);
        state.job.cancelled
    }

    /// Bring the store up to date now, as the work stops for a while.
    pub fn flush(&self) {
        let mut state = self.state.lock();
        self.sync(&mut state, true);
        drop(state);
        self.poll_actions();
    }

    fn start_action_polling(self: &Arc<Self>) {
        if tokio::runtime::Handle::try_current().is_ok() {
            self.keep_up();
        }
    }

    /// Keep progress and action dispatch current, with at most one weak poller.
    /// Finished jobs continue while a callable or question is present.
    pub fn keep_up(self: &Arc<Self>) {
        {
            let mut state = self.state.lock();
            if state.polling || state.gone {
                return;
            }
            state.polling = true;
        }
        let weak: Weak<Self> = Arc::downgrade(self);
        tokio::spawn(async move {
            loop {
                tokio::time::sleep(SYNC_EVERY).await;
                let Some(working) = weak.upgrade() else {
                    break;
                };
                {
                    let mut state = working.state.lock();
                    if state.gone
                        || (state.ended
                            && state.callable.is_none()
                            && state.job.popup_yes_no_question.is_none())
                    {
                        state.polling = false;
                        break;
                    }
                    working.sync(&mut state, false);
                }
                working.poll_actions();
            }
        });
    }

    /// `Finish`: done, it stays to be read (and dismissed).
    pub fn finish(&self) {
        self.end(false);
    }

    /// `FinishAndDismiss`: done, it goes.
    pub fn finish_and_dismiss(&self) {
        self.end(true);
    }

    /// The files it shows, if any.
    pub fn has_files(&self) -> bool {
        self.state.lock().job.files.is_some()
    }

    fn end(&self, dismiss: bool) {
        let mut state = self.state.lock();
        if state.ended {
            return;
        }
        state.job.finish();
        if dismiss {
            state.job.dismissed = true;
        }
        state.network = None;
        state.dirty = true;
        self.sync(&mut state, true);
        state.ended = true;
    }

    /// Write what the work has changed, or look for the client's cancel,
    /// once it is shown, if a quarter second has passed (or `now`).
    fn sync(&self, state: &mut WorkingState, now: bool) {
        if !state.shown || state.gone {
            return;
        }
        if !now && state.synced.is_some_and(|at| at.elapsed() < SYNC_EVERY) {
            return;
        }
        state.synced = Some(Instant::now());
        if let (Some((level, prefix, said)), Some(network)) =
            (state.following.as_mut(), state.network.as_ref())
        {
            let now = network.state();
            if now.stages != *said {
                *said = now.stages;
                let text = format!("{prefix}{}", now.stage.lines().next().unwrap_or_default());
                let level = *level;
                let job = &mut state.job;
                if level == 1 {
                    job.status_text_1 = Some(text);
                } else {
                    job.status_text_2 = Some(text);
                }
                state.dirty = true;
            }
        }
        let network = downloading(state.network.as_deref());
        let at = now_whole();
        let key = state.job.key;
        let result = if state.dirty || network != state.job.network_job {
            state.job.network_job = network;
            state.dirty = false;
            let ours = state.job.clone();
            self.store.write(move |ctx| {
                if popups::get(ctx.conn(), &key, at)?
                    .is_none_or(|job| job.action_owner != ours.action_owner)
                {
                    return Ok(None);
                }
                popups::update(ctx.conn(), &key, at, |theirs| {
                    // (what the work says is ours; cancelling and pausing
                    // are the client's)
                    theirs.status_title.clone_from(&ours.status_title);
                    theirs.status_text_1.clone_from(&ours.status_text_1);
                    theirs.status_text_2.clone_from(&ours.status_text_2);
                    theirs.popup_gauge_1 = ours.popup_gauge_1;
                    theirs.popup_gauge_2 = ours.popup_gauge_2;
                    theirs.files.clone_from(&ours.files);
                    theirs.popup_clipboard.clone_from(&ours.popup_clipboard);
                    theirs
                        .popup_yes_no_question
                        .clone_from(&ours.popup_yes_no_question);
                    theirs
                        .user_callable_label
                        .clone_from(&ours.user_callable_label);
                    theirs.action_owner = ours.action_owner;
                    theirs.network_job.clone_from(&ours.network_job);
                    if ours.done {
                        theirs.finish();
                    }
                    theirs.dismissed |= ours.dismissed;
                    theirs.cancelled
                })
            })
        } else {
            self.store
                .read(|conn| Ok(popups::get(conn, &key, at)?.map(|job| job.cancelled)))
        };
        match result {
            Ok(Some(cancelled)) => state.job.cancelled |= cancelled,
            Ok(None) => state.gone = true,
            Err(e) => {
                tracing::error!(error = %e, "could not keep a popup up to date");
                state.gone = true;
            }
        }
    }
}

/// The download a popup shows: one going (as the reference's popups show a
/// network job only while it runs).
fn downloading(network: Option<&hydrus_net::Job>) -> Option<hydrus_store::live::JobLive> {
    network
        .map(crate::queue::live)
        .filter(|live| !live.done && !live.url.is_empty())
}

impl Drop for Working {
    fn drop(&mut self) {
        self.end(true);
        let state = self.state.lock();
        let key = state.job.key;
        let owner = state.job.action_owner.expect("live owner");
        let _ = self
            .store
            .write(move |ctx| hydrus_store::popup_actions::retire(ctx.conn(), &key, &owner));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn shown(store: &Store) -> Vec<Job> {
        store.read(|conn| popups::all(conn, now_whole())).unwrap()
    }

    #[test]
    fn current_callable_is_reentrant_and_removal_invalidates_later_queued_calls() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::open(dir.path()).unwrap();
        let working = Working::new(&store, "callable", true);
        let weak = Arc::downgrade(&working);
        let effect = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        working.set_user_callable("remove myself", {
            let effect = effect.clone();
            move || {
                effect.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                let working = weak.upgrade().unwrap();
                working.clear_user_callable();
                working.flush();
            }
        });
        working.show();
        let job = shown(&store).remove(0);
        for _ in 0..2 {
            let key = job.key;
            let owner = job.action_owner.unwrap();
            assert!(
                store
                    .write(move |ctx| hydrus_store::popup_actions::request(
                        ctx.conn(),
                        &key,
                        &owner,
                        now_whole(),
                        hydrus_store::popup_actions::Request::Call
                    ))
                    .unwrap()
            );
        }
        working.poll_actions();
        assert_eq!(effect.load(std::sync::atomic::Ordering::SeqCst), 1);
        assert!(shown(&store)[0].user_callable_label.is_none());
    }

    #[tokio::test]
    async fn finished_callable_dispatches_between_independent_store_handles_and_releases_its_owner()
    {
        let dir = tempfile::tempdir().unwrap();
        let producer_store = Store::open(dir.path()).unwrap();
        let consumer_store = Store::open(dir.path()).unwrap();
        let working = Working::new(&producer_store, "live transport", true);
        let (sent, mut received) = tokio::sync::mpsc::unbounded_channel();
        working.set_user_callable("effect", move || {
            sent.send(()).unwrap();
        });
        working.show();
        working.keep_up();
        working.finish();
        let job = shown(&consumer_store).remove(0);
        assert!(job.done);
        let key = job.key;
        let owner = job.action_owner.unwrap();
        consumer_store
            .write(move |ctx| {
                hydrus_store::popup_actions::request(
                    ctx.conn(),
                    &key,
                    &owner,
                    now_whole(),
                    hydrus_store::popup_actions::Request::Call,
                )
            })
            .unwrap();
        tokio::time::timeout(Duration::from_secs(2), received.recv())
            .await
            .unwrap()
            .unwrap();
        let weak = Arc::downgrade(&working);
        drop(working);
        assert!(
            weak.upgrade().is_none(),
            "the polling task holds no permanent producer lease"
        );
        assert!(shown(&consumer_store)[0].user_callable_label.is_none());
        assert!(
            !consumer_store
                .write(move |ctx| hydrus_store::popup_actions::request(
                    ctx.conn(),
                    &key,
                    &owner,
                    now_whole(),
                    hydrus_store::popup_actions::Request::Call
                ))
                .unwrap()
        );
    }

    #[tokio::test]
    async fn a_callable_added_after_finish_restarts_the_single_weak_poller() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::open(dir.path()).unwrap();
        let working = Working::new(&store, "post-finish action", true);
        working.show();
        working.keep_up();
        working.finish();
        tokio::time::timeout(Duration::from_secs(2), async {
            while working.state.lock().polling {
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
        let (sent, mut received) = tokio::sync::mpsc::unbounded_channel();
        working.set_user_callable("late callable", move || {
            sent.send(()).unwrap();
        });
        working.keep_up();
        working.keep_up();
        let job = shown(&store).remove(0);
        let key = job.key;
        let owner = job.action_owner.unwrap();
        assert!(
            store
                .write(move |ctx| hydrus_store::popup_actions::request(
                    ctx.conn(),
                    &key,
                    &owner,
                    now_whole(),
                    hydrus_store::popup_actions::Request::Call
                ))
                .unwrap()
        );
        tokio::time::timeout(Duration::from_secs(2), received.recv())
            .await
            .unwrap()
            .unwrap();
        assert!(received.try_recv().is_err());
        working.clear_user_callable();
        tokio::time::timeout(Duration::from_secs(2), async {
            while working.state.lock().polling {
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
    }

    #[test]
    fn gui_retirement_during_the_first_callable_invalidates_later_claimed_calls() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::open(dir.path()).unwrap();
        let working = Working::new(&store, "retirement during dispatch", true);
        let gui = [8; 32];
        store
            .write(move |ctx| hydrus_store::popup_actions::begin_gui(ctx.conn(), &gui))
            .unwrap();
        let effect = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        working.set_user_callable("close GUI", {
            let store = store.clone();
            let effect = effect.clone();
            move || {
                effect.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                store
                    .write(move |ctx| hydrus_store::popup_actions::retire_gui(ctx.conn(), &gui))
                    .unwrap();
            }
        });
        working.show();
        let job = shown(&store).remove(0);
        for _ in 0..2 {
            let key = job.key;
            let owner = job.action_owner.unwrap();
            assert!(
                store
                    .write(move |ctx| hydrus_store::popup_actions::request_from_gui(
                        ctx.conn(),
                        &key,
                        &owner,
                        &gui,
                        now_whole(),
                        hydrus_store::popup_actions::Request::Call
                    ))
                    .unwrap()
            );
        }
        working.poll_actions();
        assert_eq!(effect.load(std::sync::atomic::Ordering::SeqCst), 1);
    }

    #[test]
    fn concurrent_pollers_cannot_overtake_a_reentrant_callable_removal() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::open(dir.path()).unwrap();
        let working = Working::new(&store, "serial effects", true);
        let effect = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let (entered, ready) = std::sync::mpsc::channel();
        let (release, released) = std::sync::mpsc::channel();
        let released = Arc::new(Mutex::new(released));
        working.set_user_callable("remove before next click", {
            let effect = effect.clone();
            let weak = Arc::downgrade(&working);
            move || {
                effect.fetch_add(1, Ordering::SeqCst);
                entered.send(()).unwrap();
                released
                    .lock()
                    .recv_timeout(Duration::from_secs(2))
                    .unwrap();
                let working = weak.upgrade().unwrap();
                working.clear_user_callable();
                working.flush(); // Nested polling must be nonblocking.
            }
        });
        working.show();
        let job = shown(&store).remove(0);
        let key = job.key;
        let owner = job.action_owner.unwrap();
        store
            .write(move |ctx| {
                hydrus_store::popup_actions::request(
                    ctx.conn(),
                    &key,
                    &owner,
                    now_whole(),
                    hydrus_store::popup_actions::Request::Call,
                )
            })
            .unwrap();
        let worker = std::thread::spawn({
            let working = working.clone();
            move || working.poll_actions()
        });
        ready.recv_timeout(Duration::from_secs(2)).unwrap();
        store
            .write(move |ctx| {
                hydrus_store::popup_actions::request(
                    ctx.conn(),
                    &key,
                    &owner,
                    now_whole(),
                    hydrus_store::popup_actions::Request::Call,
                )
            })
            .unwrap();
        working.poll_actions();
        assert_eq!(effect.load(Ordering::SeqCst), 1);
        release.send(()).unwrap();
        worker.join().unwrap();
        working.poll_actions();
        assert_eq!(effect.load(Ordering::SeqCst), 1);
        assert!(shown(&store)[0].user_callable_label.is_none());
    }

    #[test]
    fn a_working_popup_keeps_the_store_up_to_date_and_hears_a_cancel() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::open(dir.path()).unwrap();

        // nothing until it is shown
        let working = Working::new(&store, "import folder - inbox", true);
        working.set_text(Some("checking".into()));
        assert!(shown(&store).is_empty());
        working.show();
        let job = &shown(&store)[0];
        assert_eq!(job.status_title.as_deref(), Some("import folder - inbox"));
        assert_eq!(job.status_text_1.as_deref(), Some("checking"));
        assert!(job.cancellable && !job.done);

        // changes reach the store at most four times a second, or at once
        // when flushed
        working.set_text(Some("importing: 0/2".into()));
        working.set_gauge(Some((0, 2)));
        assert_eq!(shown(&store)[0].status_text_1.as_deref(), Some("checking"));
        working.flush();
        let job = &shown(&store)[0];
        assert_eq!(job.status_text_1.as_deref(), Some("importing: 0/2"));
        assert_eq!(job.popup_gauge_1, Some((0, 2)));
        std::thread::sleep(SYNC_EVERY);
        working.set_text(Some("importing: 1/2".into()));
        assert_eq!(
            shown(&store)[0].status_text_1.as_deref(),
            Some("importing: 1/2"),
            "(a quarter second on)"
        );

        // the client cancels it: the work hears, and saying more doesn't
        // undo it
        let key = job.key;
        store
            .write(move |ctx| popups::update(ctx.conn(), &key, now_whole(), Job::cancel))
            .unwrap();
        working.flush();
        assert!(working.is_cancelled());
        working.set_text(Some("stopping".into()));
        working.flush();
        let job = &shown(&store)[0];
        assert!(job.cancelled && job.done);
        assert_eq!(job.status_text_1.as_deref(), Some("stopping"));

        // finished and dismissed, it goes
        working.finish_and_dismiss();
        assert!(shown(&store).is_empty());

        // finished, one stays to be read; dropped unfinished, one goes
        let kept = Working::new(&store, "subscriptions - art", true);
        kept.show();
        kept.set_files(vec![Sha256([1; 32])], "art");
        kept.finish();
        let job = &shown(&store)[0];
        assert!(job.done && !job.cancellable);
        assert_eq!(job.files, Some((vec![Sha256([1; 32])], Some("art".into()))));
        let dropped = Working::new(&store, "export folder - out", true);
        dropped.show();
        assert_eq!(shown(&store).len(), 2);
        drop(dropped);
        assert_eq!(shown(&store).len(), 1);
    }

    #[test]
    fn a_working_popup_says_what_its_downloads_importer_says_after_a_prefix() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::open(dir.path()).unwrap();
        let working = Working::new(&store, "subscriptions - art", true);
        let network = hydrus_net::Job::new();
        working.set_network_job(Some(Arc::clone(&network)));
        working.show();
        network.set_status_text("checking url status");
        working.follow_stage(2, "files 1/3: ");
        working.flush();
        assert_eq!(shown(&store)[0].status_text_2, None, "(said before)");
        network.set_status_text("downloading file");
        working.flush();
        assert_eq!(
            shown(&store)[0].status_text_2.as_deref(),
            Some("files 1/3: downloading file")
        );
        // a hook said again is said again, over what the work said since
        working.set_text_2(Some("files 1/3: file failed".into()));
        network.set_stage("404");
        working.flush();
        assert_eq!(
            shown(&store)[0].status_text_2.as_deref(),
            Some("files 1/3: 404")
        );
        // (with no request going, no download shows)
        assert_eq!(shown(&store)[0].network_job, None);
    }
}

//! Help > debug's actions (hydrus-gui-model's `debug_actions`): messages,
//! popups, the delayed modal popup (in its dialog, counting
//! down), saving the last session, the database checkpoint, the
//! environment dump, clearing the rendering caches and leaving the event
//! loop at once.
use std::cell::RefCell;
use std::io::Write as _;
use std::rc::Rc;
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use hydrus_core::Sha256;
use hydrus_gui_model::debug_actions::{self as model, Action};
use hydrus_store::Store;
use hydrus_store::popups::{self, Job};

use crate::ChoiceButtonsWindow;
use slint::ComponentHandle as _;

/// How the modal popup's thread waits out the five seconds and each second.
#[doc(hidden)]
pub type Wait = Arc<dyn Fn(Duration) + Send + Sync>;

thread_local! {
    /// The message shown, kept open until answered.
    static MESSAGE: RefCell<Option<ChoiceButtonsWindow>> = const { RefCell::new(None) };
    /// A test's way to wait, for the modal popups made from this thread.
    static MODAL_WAIT: RefCell<Option<Wait>> = const { RefCell::new(None) };
}

/// Make the modal popups made from this thread wait this way instead of
/// sleeping, so a test can release each second itself.
#[doc(hidden)]
pub fn set_modal_wait(wait: Option<Wait>) {
    MODAL_WAIT.with(|w| *w.borrow_mut() = wait);
}

/// The message window now shown, if any (for tests of the actions that say
/// something and wait for "ok").
pub fn message_window() -> Option<ChoiceButtonsWindow> {
    MESSAGE.with(|m| m.borrow().as_ref().map(ChoiceButtonsWindow::clone_strong))
}

/// What `HydrusData.DebugPrint` was given, oldest first: the debug actions
/// print to the console and keep the lines, for the tests.
static PRINTED: Mutex<Vec<String>> = Mutex::new(Vec::new());

/// The lines the debug actions have printed so far.
pub fn debug_printed() -> Vec<String> {
    PRINTED
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .clone()
}

/// `HydrusData.DebugPrint`: a line on the console, flushed at once.
fn debug_print(line: &str) {
    eprintln!("{line}");
    let _ = std::io::stderr().flush();
    PRINTED
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .push(line.to_owned());
}

/// Send the report modes' messages to the console and a popup in `store`
/// (`HydrusData.ShowText`), unless something already receives them.
pub(crate) fn install_report_sink(store: &std::sync::Arc<Store>) {
    let popup = hydrus_store::debug_switches::report_to_popups(store);
    hydrus_core::debug_flags::set_sink_if_none(Box::new(move |text| {
        debug_print(text);
        popup(text);
    }));
    // (the switches are not kept between runs, and the daemon follows the
    // ones set here)
    let _ = hydrus_store::debug_switches::publish(store);
}

/// Whether panics are logged: always (the reference logs crashes only
/// while "use faulthandler to log crashes" is on; see `hydrus_core::crash_log`).
pub fn crash_logging() -> bool {
    true
}

/// "use faulthandler to log crashes": it stays in the menu, shown on, and
/// says where the log is. (The reference's faulthandler also catches native
/// signals; a handler for those would need `unsafe`, which this workspace
/// denies.)
fn flip_crash_logging(store: &Store) {
    let path = hydrus_core::crash_log::path()
        .unwrap_or_else(|| store.dir().join(hydrus_core::crash_log::LOG_NAME));
    post(
        store,
        vec![Job::text(
            format!(
                "Crashes are always logged here. Each panic, on any thread, is written with a backtrace to {}",
                path.display()
            ),
            now(),
        )],
    );
}

thread_local! {
    /// The environment "show env" shows in place of the process's own.
    static ENVIRONMENT: RefCell<Option<Vec<(String, String)>>> = const { RefCell::new(None) };
}

/// Show `variables` in "show env" rather than the process's environment (for
/// tests, which cannot change it), on this thread; `None` puts it back.
pub fn set_environment(variables: Option<Vec<(String, String)>>) {
    ENVIRONMENT.with(|e| *e.borrow_mut() = variables);
}

/// What the actions work with.
pub(crate) struct Context {
    pub pages: Rc<RefCell<crate::Pages>>,
    pub ask: crate::menu_bar::Ask,
    /// Clear the image, tile and thumbnail caches.
    pub clear_caches: Rc<dyn Fn()>,
    /// Tell the caches to maintain themselves (the fast pulse).
    pub maintain_caches: Rc<dyn Fn()>,
}

fn now() -> f64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0.0, |d| d.as_secs_f64())
}

#[allow(clippy::cast_possible_truncation)] // (seconds)
fn post(store: &Store, jobs: Vec<Job>) {
    let at = now() as i64;
    let done = store.write(move |ctx| {
        for job in &jobs {
            popups::add(ctx.conn(), job, at)?;
        }
        Ok(())
    });
    if let Err(e) = done {
        eprintln!("could not show the debug popups: {e}");
    }
}

/// An information or warning message with an "ok" button.
pub(crate) fn message(title: &str, text: &str) {
    message_then(title, text, Box::new(|| {}));
}

/// A message with an "ok" button; `then` runs once it is dismissed.
pub(crate) fn message_then(title: &str, text: &str, then: Box<dyn FnOnce()>) {
    let ask = crate::choice_buttons::Ask {
        title,
        message: text,
        choices: Vec::new(),
        no_label: "ok",
    };
    match crate::choice_buttons::open(&ask, move |_| then()) {
        Ok(window) => MESSAGE.with(|m| *m.borrow_mut() = window),
        Err(e) => eprintln!("could not show the message: {e}"),
    }
}

/// `_DebugMakeDelayedModalPopup`'s thread: after five seconds, ten seconds
/// counting down (stopping if cancelled), then gone.
#[allow(clippy::cast_possible_truncation)] // (seconds)
fn modal(store: &Store, cancellable: bool, wait: Option<Wait>) {
    let wait = move |time: Duration| match &wait {
        Some(wait) => wait(time),
        None => std::thread::sleep(time),
    };
    wait(Duration::from_secs(5));
    let mut job = Job::new(false, cancellable, now());
    job.status_title = Some(model::MODAL_TITLE.into());
    // (`pub( 'modal_message' )`: held for its dialog)
    job.held_by_modal = true;
    let key = job.key;
    post(store, vec![job]);
    for i in 0..10 {
        let at = now() as i64;
        let cancelled = store
            .write(move |ctx| {
                // (gone, as dismissed, counts as cancelled)
                Ok(popups::update(ctx.conn(), &key, at, |job| {
                    // (as the reference: cancelled is checked before the
                    // next second's words are written)
                    if !job.cancelled {
                        job.status_text_1 = Some(model::modal_text(i));
                        job.popup_gauge_1 = Some((i, 10));
                    }
                    job.cancelled
                })?
                .unwrap_or(true))
            })
            .unwrap_or(true);
        if cancelled {
            break;
        }
        wait(Duration::from_secs(1));
    }
    let at = now() as i64;
    let _ = store.write(move |ctx| {
        popups::update(ctx.conn(), &key, at, |job| job.finish_and_dismiss(None, at)).map(|_| ())
    });
}

thread_local! {
    /// The popups whose buttons call back, kept for as long as the client
    /// runs (a dropped one finishes and goes).
    static CALLING: RefCell<Vec<std::sync::Arc<hydrus_download::popups::Working>>> =
        const { RefCell::new(Vec::new()) };
    /// Buttons clicked that need the pages, from the thread that polls them.
    static GAP_CALLS: std::sync::Arc<Mutex<usize>> = std::sync::Arc::new(Mutex::new(0));
    /// The pages the gap downloader's popup works on.
    static GAP_PAGES: RefCell<std::rc::Weak<RefCell<crate::Pages>>> =
        const { RefCell::new(std::rc::Weak::new()) };
    static CALL_TIMER: slint::Timer = slint::Timer::default();
}

/// `_DebugMakeSomePopups`' popups at once, those with buttons in a live
/// popup that is polled for its clicks, and the key of the one that pulses.
fn some_popups(store: &std::sync::Arc<Store>, context: &Context) -> Option<[u8; 32]> {
    let mut pulsing = None;
    for job in model::some_popups(now(), || Sha256(rand::random::<[u8; 32]>())) {
        if job.status_title.as_deref() == Some(model::PULSING_JOB) {
            pulsing = Some(job.key);
        }
        let Some(call) = model::call_of(&job) else {
            post(store, vec![job]);
            continue;
        };
        let popup = hydrus_download::popups::Working::new(
            store,
            job.status_title.clone().unwrap_or_default(),
            false,
        );
        popup.set_text(job.status_text_1.clone());
        let label = job.user_callable_label.clone().unwrap_or_default();
        match call {
            model::Call::ShowText => {
                let store = store.clone();
                popup.set_user_callable(label, move || {
                    hydrus_download::popups::show_text(&store, model::USER_CALL_TEXT);
                });
            }
            model::Call::GapDownloader => {
                let calls = GAP_CALLS.with(std::sync::Arc::clone);
                popup.set_user_callable(label, move || {
                    *calls
                        .lock()
                        .unwrap_or_else(std::sync::PoisonError::into_inner) += 1;
                });
            }
        }
        popup.show();
        CALLING.with(|c| c.borrow_mut().push(popup));
    }
    GAP_PAGES.with(|p| *p.borrow_mut() = Rc::downgrade(&context.pages));
    let store = store.clone();
    CALL_TIMER.with(|timer| {
        timer.start(
            slint::TimerMode::Repeated,
            Duration::from_millis(100),
            move || {
                for popup in CALLING.with(|c| c.borrow().clone()) {
                    popup.poll_actions();
                }
                let calls = GAP_CALLS.with(std::sync::Arc::clone);
                let clicks = std::mem::take(
                    &mut *calls
                        .lock()
                        .unwrap_or_else(std::sync::PoisonError::into_inner),
                );
                for _ in 0..clicks {
                    if let Some(pages) = GAP_PAGES.with(|p| p.borrow().upgrade()) {
                        gap_downloader(&store, &pages);
                    }
                }
            },
        );
    });
    pulsing
}

/// `CreateNewSubscriptionGapDownloader`: the "subscription gap downloaders"
/// page, and the search pended on it.
fn gap_downloader(store: &Store, pages: &Rc<RefCell<crate::Pages>>) {
    let page = pages.borrow_mut().gallery_page_named(model::GAP_PAGE_NAME);
    let result = page.and_then(|page| {
        page.borrow_mut().pend_gap_downloader(
            model::GAP_GUG,
            model::GAP_QUERY,
            hydrus_core::import_options::ImportOptionsSlice::default(),
            model::GAP_FILE_LIMIT,
        )
    });
    if let Err(text) = result {
        hydrus_download::popups::show_text(store, text);
    }
}

pub(crate) fn run(context: &Context, action: Action) {
    let store = context.pages.borrow().store().clone();
    match action {
        Action::ProfileInfo => message("Information", model::PROFILE_MESSAGE),
        Action::MessageBox => message("Warning", model::MESSAGE_BOX_TEXT),
        Action::SomePopups => {
            let pulsing = some_popups(&store, context);
            if let Some(key) = pulsing {
                // (`CallLater( 2.0, ... )`: the test job gets a pulsing sub-job)
                let store = store.clone();
                slint::Timer::single_shot(Duration::from_secs(2), move || {
                    #[allow(clippy::cast_possible_truncation)] // (seconds)
                    let at = now() as i64;
                    let _ = store.write(move |ctx| {
                        popups::update(ctx.conn(), &key, at, model::pulse).map(|_| ())
                    });
                });
            }
            for i in 1..4 {
                let store = store.clone();
                slint::Timer::single_shot(Duration::from_millis(500 * i as u64), move || {
                    post(&store, vec![Job::text(model::delayed_popup_text(i), now())]);
                });
            }
        }
        Action::ModalPopup { cancellable } => {
            let wait = MODAL_WAIT.with(|w| w.borrow().clone());
            std::thread::spawn(move || modal(&store, cancellable, wait));
        }
        // (hydrus-rs keeps no column widths to reset)
        Action::ResetColumns => (context.ask)(model::RESET_COLUMNS_QUESTION.into(), Rc::new(|| {})),
        Action::SaveLastSession => {
            #[allow(clippy::cast_possible_truncation)] // (seconds)
            let at = now() as i64;
            if let Err(e) = context.pages.borrow_mut().save(at) {
                eprintln!("could not save the last session: {e}");
            }
        }
        Action::FlushLog => {
            debug_print(model::FLUSH_LOG);
        }
        Action::ForceCommit => {
            // (pausing moves the whole write-ahead log into the database file)
            if let Err(e) = store.pause() {
                eprintln!("could not commit the database: {e}");
            }
        }
        Action::ShowEnv => {
            let separator = if cfg!(windows) { ';' } else { ':' };
            let vars = ENVIRONMENT
                .with(|e| e.borrow().clone())
                .unwrap_or_else(|| std::env::vars().collect());
            let text = model::env_text(vars, separator);
            debug_print(&text);
            post(&store, vec![Job::text(text, now())]);
        }
        // (as the reference's `QApplication.exit`: the event loop stops)
        Action::Exit => {
            let _ = slint::quit_event_loop();
        }
        Action::ClearRenderingCaches => (context.clear_caches)(),
        Action::MaintainMemoryFast => (context.maintain_caches)(),
        Action::MaintainMemorySlow => context.pages.borrow_mut().delete_old_closed_pages(),
        Action::ScanStorage => {
            let Some(path) = crate::pick_in(crate::Pick::Folder, "Select directory", store.dir())
                .into_iter()
                .next()
            else {
                return;
            };
            let granularity = store.snapshot().storage.granularity();
            let started = std::time::Instant::now();
            let found = model::presumptive_subfolders(&path, granularity).map(|f| f.len());
            let text = model::scan_text(&found, started.elapsed().as_secs_f64());
            post(&store, vec![Job::text(text, now())]);
        }
        Action::FlipCrashLogging => flip_crash_logging(&store),
        Action::CrashTheProgram => crash_the_program(),
    }
}

/// `CrashTheProgram`: asks `GetYesYesNo`'s question, then "nice crash" ends
/// the process (the reference's `os.abort`; here a panic on the calling
/// thread, which the crash log records and which ends the program) and "not
/// a nice crash" panics a worker (the reference repaints in a loop on a
/// thread until Qt crashes). "forget it" and closing the question do
/// nothing.
fn crash_the_program() {
    let ask = crate::choice_buttons::Ask {
        title: model::CRASH_TITLE,
        message: model::CRASH_MESSAGE,
        choices: vec![model::CRASH_NICE.into(), model::CRASH_NOT_NICE.into()],
        no_label: model::CRASH_FORGET_IT,
    };
    let answer = |choice: Option<usize>| match choice {
        Some(0) => panic!("{}", model::CRASH_PANIC),
        Some(_) => {
            let _ = std::thread::Builder::new()
                .name("crashtime-not-nice".to_owned())
                .spawn(|| panic!("{}", model::CRASH_NOT_NICE_PANIC));
        }
        None => {}
    };
    match crate::choice_buttons::open(&ask, answer) {
        Ok(window) => MESSAGE.with(|m| *m.borrow_mut() = window),
        Err(e) => eprintln!("could not ask about the crash: {e}"),
    }
}

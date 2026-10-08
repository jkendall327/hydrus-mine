//! Database > backup's entries (hydrus-gui-model's `database_backup`):
//! choosing the backup location, updating the backup off the UI thread
//! with a popup, restoring one on the next start, and the note for a
//! database in several locations.
use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use hydrus_gui_model::database_backup::{self as model, Action, Chosen};
use hydrus_store::Store;
use hydrus_store::backup::BackupSettings;
use hydrus_store::popups::{self, Job};

/// What the entries work with.
pub(crate) struct Context {
    pub store: Arc<Store>,
    pub ask: crate::menu_bar::Ask,
    /// Save the last session (before a backup, as the reference does).
    pub save_session: Rc<dyn Fn()>,
    /// Exit and start again.
    pub restart: Rc<dyn Fn()>,
}

fn now() -> i64 {
    hydrus_core::TimestampMs::now().millis() / 1000
}

#[allow(clippy::cast_precision_loss)] // (seconds)
fn text_popup(store: &Store, text: &str) {
    let job = Job::text(text, now() as f64);
    let at = now();
    if let Err(e) = store.write(move |ctx| popups::add(ctx.conn(), &job, at)) {
        eprintln!("could not show the popup: {e}");
    }
}

fn settings(store: &Store) -> BackupSettings {
    store.read(hydrus_store::settings::get).unwrap_or_default()
}

pub(crate) fn run(context: &Rc<Context>, action: Action) {
    match action {
        Action::Multiple => text_popup(&context.store, model::MULTIPLE_LOCATIONS),
        Action::SetUp => set_up(context),
        Action::Update => update(context),
        Action::Restore => restore(context),
    }
}

/// `_SetupBackupPath`: say what a backup is, pick the location, check it,
/// keep it, and offer to make the backup now.
fn set_up(context: &Rc<Context>) {
    let existing = settings(&context.store).path;
    let context = context.clone();
    crate::debug_actions::message_then(
        "Information",
        &model::intro(existing.as_deref()),
        Box::new(move || {
            let Some(path) = crate::pick(crate::Pick::Folder, model::PICK_TITLE)
                .into_iter()
                .next()
            else {
                return;
            };
            if path == context.store.dir() {
                crate::debug_actions::message("Warning", model::SAME_AS_DATABASE);
                return;
            }
            let text = path.to_string_lossy().into_owned();
            if existing.as_deref() == Some(text.as_str()) {
                crate::debug_actions::message("Information", model::UNCHANGED);
                return;
            }
            let chosen = Chosen::of(&path, hydrus_store::store::DB_FILE_NAME);
            let after = context.clone();
            (context.ask)(
                model::chosen_question(&text, chosen),
                Rc::new(move || {
                    let path = text.clone();
                    let saved = after.store.write(move |ctx| {
                        hydrus_store::settings::set(
                            ctx.conn(),
                            &BackupSettings {
                                path: Some(path),
                                last_backup: None,
                            },
                        )
                    });
                    if let Err(e) = saved {
                        eprintln!("could not save the backup location: {e}");
                        return;
                    }
                    let now_context = after.clone();
                    (after.ask)(
                        model::CREATE_NOW.into(),
                        Rc::new(move || update(&now_context)),
                    );
                }),
            );
        }),
    );
}

/// `_BackupDatabase`: ask, save the session, then back up off the UI
/// thread with a popup saying how it goes.
fn update(context: &Rc<Context>) {
    let Some(path) = settings(&context.store).path else {
        crate::debug_actions::message("Warning", model::NO_PATH);
        return;
    };
    let dest = std::path::PathBuf::from(&path);
    let ask = {
        let context = context.clone();
        let path = path.clone();
        move || {
            let exists = dest.join(hydrus_store::store::DB_FILE_NAME).exists()
                || dest.join("client.db").exists();
            let go = {
                let context = context.clone();
                let dest = dest.clone();
                Rc::new(move || {
                    (context.save_session)();
                    start(&context.store, dest.clone());
                })
            };
            (context.ask)(model::update_question(&path, exists), go);
        }
    };
    if std::path::Path::new(&path).exists() {
        ask();
    } else {
        if let Err(e) = std::fs::create_dir_all(&path) {
            eprintln!("could not make the backup directory: {e}");
        }
        crate::debug_actions::message_then("Information", model::CREATING_PATH, Box::new(ask));
    }
}

/// Back up to `dest` on a worker: a cancellable, modal "backing up db" job with
/// its progress, finished "backup complete!", and the time kept.
#[allow(clippy::cast_precision_loss)] // (seconds)
fn start(store: &Arc<Store>, dest: std::path::PathBuf) {
    let store = store.clone();
    std::thread::spawn(move || {
        let mut job = Job::new(false, true, now() as f64);
        job.status_title = Some(model::POPUP_TITLE.into());
        job.status_text_1 = Some("closing db".into());
        // (`pub( 'modal_message' )`: in a dialog of its own while it runs)
        job.held_by_modal = true;
        let key = job.key;
        let at = now();
        if store
            .write(move |ctx| popups::add(ctx.conn(), &job, at))
            .is_err()
        {
            return;
        }
        let cancelled = Arc::new(AtomicBool::new(false));
        let say = |text: String| {
            let at = now();
            let flag = cancelled.clone();
            let _ = store.write(move |ctx| {
                popups::update(ctx.conn(), &key, at, |job| {
                    job.status_text_1 = Some(text);
                    if job.cancelled {
                        flag.store(true, Ordering::Relaxed);
                    }
                })
                .map(|_| ())
            });
        };
        let mut say = say;
        let done = hydrus_store::backup::backup(&store, &dest, &mut say, &|| {
            cancelled.load(Ordering::Relaxed)
        });
        let finished = now();
        let last = match &done {
            Ok(()) => Some(finished),
            Err(_) => None,
        };
        let text = match done {
            Ok(()) => model::COMPLETE.to_owned(),
            Err(e) => format!("could not back up: {e}"),
        };
        let _ = store.write(move |ctx| {
            popups::update(ctx.conn(), &key, finished, |job| {
                job.status_text_1 = Some(text);
                job.finish();
            })?;
            if let Some(last) = last {
                let mut settings: BackupSettings = hydrus_store::settings::get(ctx.conn())?;
                settings.last_backup = Some(last);
                hydrus_store::settings::set(ctx.conn(), &settings)?;
            }
            Ok(())
        });
    });
}

/// `RestoreDatabase`: pick the backup, ask, then restart to restore it.
fn restore(context: &Rc<Context>) {
    let Some(from) = crate::pick(crate::Pick::Folder, model::PICK_TITLE)
        .into_iter()
        .next()
    else {
        return;
    };
    let after = context.clone();
    (context.ask)(
        model::restore_question(&from.to_string_lossy()),
        Rc::new(move || {
            let media = hydrus_store::backup::media_location(&after.store)
                .ok()
                .flatten()
                .unwrap_or_else(|| after.store.dir().join("client_files"));
            match hydrus_store::backup::request_restore(after.store.dir(), &from, &media) {
                Ok(()) => (after.restart)(),
                Err(e) => eprintln!("could not ask for the restore: {e}"),
            }
        }),
    );
}

//! Independently retained migration popup jobs, separate from the settings draft.
use crate::{TagMigrationProgressWindow, TagMigrationWindow};
use hydrus_gui_model::tag_migration as model;
use hydrus_store::{
    Store,
    tag_migration::{Event, Options, Request},
};
use slint::ComponentHandle as _;
use std::{
    cell::RefCell,
    rc::Rc,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};
struct Job {
    window: TagMigrationProgressWindow,
    timer: slint::Timer,
}
thread_local! {
    static ACTIVE:RefCell<Vec<Rc<Job>>>=const{RefCell::new(Vec::new())};
}
/// Latest visible migration popup, including a recently completed job.
pub fn last_opened() -> Option<TagMigrationProgressWindow> {
    ACTIVE.with(|active| {
        active
            .borrow()
            .iter()
            .rev()
            .find(|job| job.window.window().is_visible())
            .map(|job| job.window.clone_strong())
    })
}
fn retire(job: &Rc<Job>) {
    job.timer.stop();
    let _ = job.window.hide();
    ACTIVE.with(|active| active.borrow_mut().retain(|other| !Rc::ptr_eq(other, job)));
}
/// Publish a job independently of its draft window, as the reference does.
pub(super) fn start(
    store: Arc<Store>,
    request: Request,
    options: Options,
    title: &str,
    parent: slint::Weak<TagMigrationWindow>,
    changed: Rc<dyn Fn()>,
) -> Result<TagMigrationProgressWindow, String> {
    let window = TagMigrationProgressWindow::new().map_err(|e| e.to_string())?;
    window.set_job_title(title.into());
    window.set_running(true);
    window.set_can_control(true);
    let job = Rc::new(Job {
        window: window.clone_strong(),
        timer: slint::Timer::default(),
    });
    let cancel = Arc::new(AtomicBool::new(false));
    let paused = Arc::new(AtomicBool::new(false));
    window.on_pause_job({
        let paused = paused.clone();
        let weak = window.as_weak();
        move || {
            if let Some(w) = weak.upgrade()
                && w.get_can_control()
            {
                let value = !paused.load(Ordering::Acquire);
                paused.store(value, Ordering::Release);
                w.set_paused(value);
            }
        }
    });
    window.on_cancel_job({
        let cancel = cancel.clone();
        let paused = paused.clone();
        let weak = window.as_weak();
        move || {
            if let Some(w) = weak.upgrade()
                && w.get_can_control()
            {
                cancel.store(true, Ordering::Release);
                paused.store(false, Ordering::Release);
                w.set_paused(false);
                w.set_can_control(false);
            }
        }
    });
    window.on_dismiss({
        let weak = Rc::downgrade(&job);
        move || {
            if let Some(job) = weak.upgrade()
                && !job.window.get_can_control()
            {
                let _ = job.window.hide();
                if !job.window.get_running() {
                    retire(&job);
                }
            }
        }
    });
    window.window().on_close_requested({
        let weak = window.as_weak();
        move || {
            if let Some(w) = weak.upgrade() {
                if w.get_can_control() {
                    return slint::CloseRequestResponse::KeepWindowShown;
                }
                w.invoke_dismiss();
            }
            slint::CloseRequestResponse::HideWindow
        }
    });
    window.show().map_err(|e| e.to_string())?;
    let (send, receive) = std::sync::mpsc::channel::<Result<Event, String>>();
    std::thread::Builder::new()
        .name("tag migration".into())
        .spawn(move || {
            let result = hydrus_store::tag_migration::run_job_events(
                &store,
                &request,
                &options,
                &cancel,
                &paused,
                512,
                |event| {
                    let _ = send.send(Ok(event));
                },
            );
            if let Err(error) = result {
                let _ = send.send(Err(error.to_string()));
            }
        })
        .map_err(|e| {
            let _ = window.hide();
            e.to_string()
        })?;
    ACTIVE.with(|active| active.borrow_mut().push(job.clone()));
    let weak = Rc::downgrade(&job);
    let mut accepted = 0;
    job.timer.start(
        slint::TimerMode::Repeated,
        Duration::from_millis(80),
        move || {
            let Some(job) = weak.upgrade() else { return };
            let w = &job.window;
            loop {
                match receive.try_recv() {
                    Ok(Ok(event)) => {
                        let text = model::event_text(event, accepted);
                        if let Event::Batch { progress, .. } = event {
                            accepted = progress.accepted;
                        }
                        w.set_progress(text.clone().into());
                        if let Event::Done(_) = event {
                            w.set_can_control(false);
                            w.set_paused(false);
                        }
                        if let Some(parent) = parent.upgrade() {
                            parent.set_progress(text.into());
                        }
                    }
                    Ok(Err(error)) => {
                        w.set_error(error.clone().into());
                        w.set_can_control(false);
                        w.set_paused(false);
                        if let Some(parent) = parent.upgrade() {
                            parent.set_error(error.into());
                        }
                    }
                    Err(std::sync::mpsc::TryRecvError::Empty) => break,
                    Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                        w.set_running(false);
                        w.set_can_control(false);
                        w.set_paused(false);
                        if let Some(parent) = parent.upgrade() {
                            parent.set_running(false);
                            parent.set_paused(false);
                        }
                        changed();
                        job.timer.stop();
                        if !w.window().is_visible() {
                            retire(&job);
                        } else if w.get_error().is_empty() {
                            let deadline = std::time::SystemTime::now()
                                .duration_since(std::time::UNIX_EPOCH)
                                .unwrap_or_default()
                                .as_secs()
                                .saturating_add(3);
                            let weak = Rc::downgrade(&job);
                            job.timer.start(
                                slint::TimerMode::Repeated,
                                Duration::from_millis(80),
                                move || {
                                    let now = std::time::SystemTime::now()
                                        .duration_since(std::time::UNIX_EPOCH)
                                        .unwrap_or_default()
                                        .as_secs();
                                    if now > deadline
                                        && let Some(job) = weak.upgrade()
                                    {
                                        retire(&job);
                                    }
                                },
                            );
                        }
                        break;
                    }
                }
            }
        },
    );
    Ok(window)
}

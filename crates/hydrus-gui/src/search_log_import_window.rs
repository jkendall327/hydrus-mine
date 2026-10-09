//! Owned reference duplicate/continuation questions. No queue writes precede
//! the final answer; closing an owner invalidates the child callbacks.
use crate::{SearchLogImportWindow, search_log::ImportStep};
use hydrus_store::{Store, queues};
use slint::{ComponentHandle as _, ModelRc, SharedString, VecModel};
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
    sync::Arc,
};

/// Import question children retained by the log or main downloader window.
#[derive(Clone, Default)]
pub struct Slots(pub Rc<RefCell<Option<SearchLogImportWindow>>>);
impl std::fmt::Debug for Slots {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("SearchLogImportSlots")
            .field(&self.has_open())
            .finish()
    }
}
impl Drop for Slots {
    fn drop(&mut self) {
        if Rc::strong_count(&self.0) == 1 {
            self.cancel();
        }
    }
}
impl Slots {
    pub fn has_open(&self) -> bool {
        self.0.borrow().is_some()
    }
    pub fn window(&self) -> Option<SearchLogImportWindow> {
        self.0
            .borrow()
            .as_ref()
            .map(slint::ComponentHandle::clone_strong)
    }
    pub fn cancel(&self) {
        if let Some(w) = self.window() {
            w.invoke_cancelled();
        }
    }
}
thread_local! { static LAST: RefCell<slint::Weak<SearchLogImportWindow>> = RefCell::new(slint::Weak::default()); }
/// The current native question, for owner/consumer regression assertions.
pub fn last() -> Option<SearchLogImportWindow> {
    LAST.with(|s| s.borrow().upgrade())
}

fn commit(store: &Store, queue: i64, urls: Vec<String>, more: bool) -> Result<(), String> {
    if urls.is_empty() {
        return Ok(());
    }
    store
        .write(move |ctx| {
            let classes =
                hydrus_core::url::UrlClasses::new(hydrus_store::settings::get(ctx.conn())?);
            let seeds = urls
                .into_iter()
                .map(|url| queues::NewGallerySeed {
                    url: classes.normalise(&url, true).unwrap_or(url),
                    can_generate_more_pages: more,
                    referral_url: None,
                    meta: queues::GallerySeedMeta {
                        run_token: hex::encode(rand::random::<[u8; 32]>()),
                        ..queues::GallerySeedMeta::default()
                    },
                })
                .collect::<Vec<_>>();
            let now = hydrus_core::time::TimestampMs::now().millis() / 1000;
            // The reference also dedupes normalised URLs within this batch.
            queues::add_gallery_seeds(ctx.conn(), queue, &seeds, None, now)?;
            queues::nudge(ctx.conn(), queue)
        })
        .map_err(|e| e.to_string())
}

fn show(window: &SearchLogImportWindow, step: &ImportStep) {
    if let Some((message, choices)) = step.question() {
        window.set_window_title("Are you sure?".into());
        window.set_message(message.into());
        window.set_choices(ModelRc::new(VecModel::from(
            choices
                .into_iter()
                .map(SharedString::from)
                .collect::<Vec<_>>(),
        )));
    }
}

/// Start a frozen input batch. URL rules are read again when it is committed.
pub(crate) fn open(
    slots: &Slots,
    store: &Arc<Store>,
    queue: i64,
    text: &str,
    more: bool,
    closed: Rc<dyn Fn()>,
) -> Result<(), String> {
    if slots.has_open() {
        return Ok(());
    }
    let seeds = store
        .read(|c| queues::gallery_seeds(c, queue))
        .map_err(|e| e.to_string())?;
    let step = ImportStep::start(text, &seeds, &store.snapshot().url_classes, more);
    if let ImportStep::Ready { urls, more } = step {
        commit(store, queue, urls, more)?;
        return Ok(());
    }
    present(slots, Some(step), store, queue, more, None, closed)
}

pub(crate) fn error(
    slots: &Slots,
    store: &Arc<Store>,
    title: &str,
    message: String,
    closed: Rc<dyn Fn()>,
) -> Result<(), String> {
    present(
        slots,
        None,
        store,
        0,
        false,
        Some((title.to_owned(), message)),
        closed,
    )
}

fn present(
    slots: &Slots,
    step: Option<ImportStep>,
    store: &Arc<Store>,
    queue: i64,
    more: bool,
    error: Option<(String, String)>,
    closed: Rc<dyn Fn()>,
) -> Result<(), String> {
    slots.cancel();
    let window = crate::app_title::new::<crate::SearchLogImportWindow>().map_err(|e| e.to_string())?;
    let alive = Rc::new(Cell::new(true));
    let pending = Rc::new(RefCell::new(step));
    let close: Rc<dyn Fn()> = Rc::new({
        let weak = window.as_weak();
        let slot = Rc::downgrade(&slots.0);
        let alive = alive.clone();
        move || {
            if !alive.replace(false) {
                return;
            }
            if let Some(w) = weak.upgrade() {
                let _ = w.hide();
            }
            if let Some(slot) = slot.upgrade() {
                slot.borrow_mut().take();
            }
            closed();
        }
    });
    window.on_chosen({
        let weak = window.as_weak();
        let pending = pending.clone();
        let close = close.clone();
        let alive = alive.clone();
        let store = store.clone();
        move |index| {
            if !alive.get() {
                return;
            }
            let Some(w) = weak.upgrade() else {
                return;
            };
            let Some(step) = pending.borrow_mut().take() else {
                close();
                return;
            };
            let step = step.answer(index, more);
            match step {
                ImportStep::Ready { urls, more } => match commit(&store, queue, urls, more) {
                    Ok(()) => close(),
                    Err(error) => {
                        w.set_window_title("Could not import!".into());
                        w.set_message(error.into());
                        w.set_choices(ModelRc::new(VecModel::from(vec!["ok".into()])));
                    }
                },
                ImportStep::Cancelled => close(),
                step => {
                    show(&w, &step);
                    *pending.borrow_mut() = Some(step);
                }
            }
        }
    });
    window.on_cancelled({
        let close = close.clone();
        move || close()
    });
    window.window().on_close_requested(move || {
        close();
        slint::CloseRequestResponse::HideWindow
    });
    if let Some((title, message)) = error {
        window.set_window_title(title.into());
        window.set_message(message.into());
        window.set_choices(ModelRc::new(VecModel::from(vec!["ok".into()])));
    } else if let Some(step) = pending.borrow().as_ref() {
        show(&window, step);
    }
    *slots.0.borrow_mut() = Some(window.clone_strong());
    LAST.with(|s| *s.borrow_mut() = window.as_weak());
    window.show().map_err(|e| e.to_string())
}

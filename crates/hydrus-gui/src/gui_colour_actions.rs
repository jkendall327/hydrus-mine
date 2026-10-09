//! Help darkmode toggles the legacy colourset, independently of the style palette.
use crate::{MainWindow, SessionDialog, Theme};
use hydrus_store::Store;
use slint::ComponentHandle as _;
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
    sync::Arc,
};
thread_local! { static LAST: RefCell<Option<slint::Weak<SessionDialog>>> = const { RefCell::new(None) }; }
/// The visible Help colourset warning, without retaining its owner.
pub fn last_notice() -> Option<SessionDialog> {
    LAST.with(|last| last.borrow().as_ref().and_then(slint::Weak::upgrade))
        .filter(|notice| notice.window().is_visible())
}
const WARNING: &str = "Hey, this command comes from an old colour system. If you want to change to darkmode, try _options->style_ instead. Or, if you know what you are doing, make sure you flip the \"override\" checkbox in _options->colours_ and then try this again.";
pub(crate) struct Binding {
    window: slint::Weak<MainWindow>,
    store: Arc<Store>,
    parent_active: Rc<Cell<bool>>,
    active: Cell<bool>,
    pub notice: Rc<RefCell<Option<SessionDialog>>>,
}
impl std::fmt::Debug for Binding {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Binding")
            .field("active", &self.active.get())
            .field("parent_active", &self.parent_active.get())
            .field("notice_open", &self.notice.borrow().is_some())
            .finish_non_exhaustive()
    }
}
impl Binding {
    pub fn new(window: &MainWindow, store: Arc<Store>, parent_active: Rc<Cell<bool>>) -> Rc<Self> {
        Rc::new(Self {
            window: window.as_weak(),
            store,
            parent_active,
            active: Cell::new(true),
            notice: Rc::default(),
        })
    }
    fn valid(&self) -> bool {
        self.active.get()
            && self.parent_active.get()
            && self
                .window
                .upgrade()
                .is_some_and(|window| window.window().is_visible())
    }
    fn commit(&self) {
        if !self.valid() {
            return;
        }
        if let Err(error) = self.store.write(move |ctx| {
            let mut settings = hydrus_store::gui_colours::load(ctx.conn())?;
            settings.flip();
            hydrus_store::settings::set(ctx.conn(), &settings)
        }) {
            eprintln!("could not change the colourset: {error}");
        }
        if let Some(window) = self.window.upgrade() {
            window.global::<Theme<'_>>().invoke_refresh_colours();
        }
    }
    pub fn retire(&self) {
        self.active.set(false);
        let notice = self.notice.borrow_mut().take();
        if let Some(notice) = notice {
            let _ = notice.hide();
        }
    }
    pub fn callback(self: &Rc<Self>) -> Rc<dyn Fn()> {
        let weak = Rc::downgrade(self);
        Rc::new(move || {
            if let Some(binding) = weak.upgrade() {
                binding.flip();
            }
        })
    }
    pub fn retire_callback(self: &Rc<Self>) -> Rc<dyn Fn()> {
        let weak = Rc::downgrade(self);
        Rc::new(move || {
            if let Some(binding) = weak.upgrade() {
                binding.retire();
            }
        })
    }
    fn flip(self: &Rc<Self>) {
        if !self.valid() || self.notice.borrow().is_some() {
            return;
        }
        let Ok(settings) = self.store.read(hydrus_store::gui_colours::load) else {
            return;
        };
        if settings.override_stylesheet {
            self.commit();
            return;
        }
        let Ok(notice) = crate::app_title::new::<crate::SessionDialog>() else {
            return;
        };
        notice.set_window_title("Information".into());
        notice.set_message(WARNING.into());
        notice.set_notice_only(true);
        notice.set_notice_ok_label("OK".into());
        let alive = Rc::new(Cell::new(true));
        let done: Rc<dyn Fn()> = Rc::new({
            let binding = Rc::downgrade(self);
            let weak = notice.as_weak();
            move || {
                let Some(binding) = binding.upgrade() else {
                    return;
                };
                if !binding.valid()
                    || !weak
                        .upgrade()
                        .is_some_and(|notice| notice.window().is_visible())
                    || !alive.replace(false)
                {
                    return;
                }
                if let Some(notice) = weak.upgrade() {
                    let _ = notice.hide();
                }
                binding.notice.borrow_mut().take();
                binding.commit();
            }
        });
        notice.on_cancelled({
            let done = done.clone();
            move || done()
        });
        notice.window().on_close_requested(move || {
            done();
            slint::CloseRequestResponse::HideWindow
        });
        LAST.with(|last| *last.borrow_mut() = Some(notice.as_weak()));
        *self.notice.borrow_mut() = Some(notice.clone_strong());
        if notice.show().is_err() {
            self.retire();
        }
    }
}
impl Drop for Binding {
    fn drop(&mut self) {
        self.retire();
    }
}

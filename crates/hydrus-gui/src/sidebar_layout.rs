//! One window owns per-page splitter geometry; global defaults use typed settings.
use crate::{MainWindow, Pages, SearchPage};
use hydrus_core::pages::PageKey;
use hydrus_gui_model::page_layout::{Action, Layout};
use hydrus_store::{Store, page_layout, settings};
use slint::ComponentHandle as _;
use std::{
    cell::{Cell, RefCell},
    collections::HashMap,
    rc::Rc,
    sync::Arc,
};

struct State {
    window: slint::Weak<MainWindow>,
    pages: Rc<RefCell<Pages>>,
    current: Rc<RefCell<Rc<RefCell<SearchPage>>>>,
    store: Arc<Store>,
    layouts: RefCell<HashMap<PageKey, Layout>>,
    owners: RefCell<HashMap<PageKey, std::rc::Weak<RefCell<SearchPage>>>>,
    shown_owner: RefCell<std::rc::Weak<RefCell<SearchPage>>>,
    shown: Cell<Option<PageKey>>,
    epoch: Cell<i32>,
    alive: Cell<bool>,
}
impl State {
    fn valid(&self) -> bool {
        self.alive.get()
            && self
                .window
                .upgrade()
                .is_some_and(|w| w.window().is_visible())
    }
    fn saved(&self) -> page_layout::PageLayout {
        self.store.read(page_layout::load).unwrap_or_else(|e| {
            eprintln!("could not load page layout: {e}");
            page_layout::PageLayout::default()
        })
    }
    fn key(&self) -> Option<PageKey> {
        let pages = self.pages.borrow();
        (!matches!(
            pages.shown().content,
            hydrus_core::pages::PageContent::Pages(_)
        ))
        .then_some(pages.shown().key)
    }
    fn actual(&self) -> (i32, i32) {
        self.window.upgrade().map_or((0, 0), |w| {
            (
                w.get_sidebar_actual_width().round() as i32,
                w.get_preview_actual_height().round() as i32,
            )
        })
    }
    fn show(&self, key: PageKey) {
        let Some(w) = self.window.upgrade() else {
            return;
        };
        let Some(layout) = self.layouts.borrow().get(&key).copied() else {
            return;
        };
        let (sidebar, preview) = layout.dimensions(
            w.get_layout_available_width().round() as i32,
            w.get_layout_available_height().round() as i32,
        );
        w.set_layout_page_key(key.to_hex().into());
        w.set_layout_epoch(self.epoch.get());
        w.set_sidebar_requested_width(sidebar as f32);
        w.set_preview_requested_height(preview as f32);
        w.set_sidebar_hidden(layout.sidebar_hidden);
        w.set_preview_splitter_hidden(
            layout.preview_hidden || layout.sidebar_hidden || sidebar == 0,
        );
    }
    fn refresh(&self) {
        if !self.alive.get() {
            return;
        }
        let key = self.key();
        let owner = self.current.borrow().clone();
        if key == self.shown.get()
            && self
                .shown_owner
                .borrow()
                .upgrade()
                .is_some_and(|old| Rc::ptr_eq(&old, &owner))
        {
            return;
        }
        *self.shown_owner.borrow_mut() = Rc::downgrade(&owner);
        self.shown.set(key);
        self.epoch.set(self.epoch.get().wrapping_add(1));
        if let Some(key) = key {
            let same = self
                .owners
                .borrow()
                .get(&key)
                .and_then(std::rc::Weak::upgrade)
                .is_some_and(|old| Rc::ptr_eq(&old, &owner));
            if !same {
                self.layouts
                    .borrow_mut()
                    .insert(key, Layout::saved(&self.saved()));
                self.owners.borrow_mut().insert(key, Rc::downgrade(&owner));
            }
            self.show(key);
        } else if let Some(w) = self.window.upgrade() {
            w.set_layout_page_key("".into());
            w.set_sidebar_hidden(true);
            w.set_preview_splitter_hidden(true);
        }
    }
    fn clear_focus(&self) {
        self.current.borrow().borrow_mut().clear_preview_focus();
        if let Some(w) = self.window.upgrade() {
            w.invoke_preview_presentation_changed();
        }
    }
    fn save_now(&self) {
        let Some(key) = self.key() else { return };
        let saved = self.saved();
        let Some(layout) = self.layouts.borrow().get(&key).copied() else {
            return;
        };
        let (hpos, vpos) = layout.positions(self.actual(), &saved);
        if let Err(e) = self.store.write(move |ctx| {
            let mut current = page_layout::load(ctx.conn())?;
            current.hpos = hpos;
            current.vpos = vpos;
            settings::set(ctx.conn(), &current)
        }) {
            eprintln!("could not save page layout: {e}");
        }
    }
    fn action(&self, action: Action) {
        if !self.valid() {
            return;
        }
        self.refresh();
        match action {
            Action::SaveNow => self.save_now(),
            Action::SaveOnExit => {
                if let Err(e) = self.store.write(|ctx| {
                    let mut value = page_layout::load(ctx.conn())?;
                    value.save_on_exit = !value.save_on_exit;
                    settings::set(ctx.conn(), &value)
                }) {
                    eprintln!("could not change exit layout saving: {e}");
                }
            }
            Action::Toggle => {
                if let Some(key) = self.key() {
                    let actual_preview = self.actual().1;
                    if let Some(layout) = self.layouts.borrow_mut().get_mut(&key) {
                        // Qt keeps the inner splitter's last visible size while
                        // its entire parent sidebar is hidden.
                        if !layout.sidebar_hidden && actual_preview > 0 {
                            layout.vpos = -i64::from(actual_preview);
                        }
                        layout.toggle(&self.saved());
                    }
                    self.epoch.set(self.epoch.get().wrapping_add(1));
                    self.show(key);
                    self.clear_focus();
                }
            }
            Action::RestoreAll => {
                let saved = self.saved();
                for value in self.layouts.borrow_mut().values_mut() {
                    *value = Layout::saved(&saved);
                }
                self.epoch.set(self.epoch.get().wrapping_add(1));
                if let Some(key) = self.key() {
                    self.show(key);
                }
            }
        }
    }
    fn resize(&self, key: &str, epoch: i32, preview: bool, pixels: f32, collapse: bool) {
        if !self.valid() {
            return;
        }
        self.refresh();
        if !pixels.is_finite() || epoch != self.epoch.get() {
            return;
        }
        let Some(current) = self.key().filter(|k| k.to_hex() == key) else {
            return;
        };
        if let Some(layout) = self.layouts.borrow_mut().get_mut(&current) {
            layout.resize(preview, pixels.round() as i32);
        }
        self.show(current);
        if collapse {
            self.clear_focus();
        }
    }
}
/// Binding retained by the window's owner, with weak window callbacks.
#[derive(Clone)]
pub(crate) struct Binding(Rc<State>);
impl std::fmt::Debug for Binding {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SidebarLayout")
            .field("alive", &self.0.alive.get())
            .field("shown", &self.0.shown.get())
            .finish_non_exhaustive()
    }
}
impl Binding {
    pub(crate) fn bind(
        window: &MainWindow,
        pages: Rc<RefCell<Pages>>,
        current: Rc<RefCell<Rc<RefCell<SearchPage>>>>,
    ) -> Self {
        window.invoke_sidebar_layout_retired();
        let store = pages.borrow().store().clone();
        let state = Rc::new(State {
            window: window.as_weak(),
            pages,
            current,
            store,
            layouts: RefCell::default(),
            owners: RefCell::default(),
            shown_owner: RefCell::default(),
            shown: Cell::new(None),
            epoch: Cell::new(window.get_layout_epoch().wrapping_add(1)),
            alive: Cell::new(true),
        });
        window.on_sidebar_layout_retired({
            let state = Rc::downgrade(&state);
            move || {
                if let Some(s) = state.upgrade() {
                    s.alive.set(false);
                }
            }
        });
        window.on_sidebar_measured({
            let state = Rc::downgrade(&state);
            move || {
                if let Some(s) = state.upgrade()
                    && s.alive.get()
                    && let Some(key) = s.key()
                {
                    s.show(key);
                }
            }
        });
        window.on_sidebar_resized({
            let state = Rc::downgrade(&state);
            move |key, epoch, preview, pixels| {
                if let Some(s) = state.upgrade() {
                    s.resize(&key, epoch, preview, pixels, false);
                }
            }
        });
        window.on_sidebar_collapsed({
            let state = Rc::downgrade(&state);
            move |key, epoch, preview| {
                if let Some(s) = state.upgrade() {
                    s.resize(&key, epoch, preview, 0.0, true);
                }
            }
        });
        state.refresh();
        Self(state)
    }
    pub(crate) fn refresh(&self) {
        self.0.refresh();
    }
    pub(crate) fn action(&self, action: Action) {
        self.0.action(action);
    }
    pub(crate) fn accepted_exit(&self) {
        if self.0.valid() && self.0.saved().save_on_exit {
            self.0.save_now();
        }
        self.0.alive.set(false);
    }
}

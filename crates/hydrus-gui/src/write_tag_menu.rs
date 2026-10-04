//! Shared write-list popup actions; pending removals are invalidated with the owner.
use crate::popup_menu::{Chosen, Popup};
use hydrus_core::search::{
    context::{LocationContext, TagContext},
    predicate::Predicate,
};
use hydrus_gui_model::{
    main_menu::{self, PopupNode},
    write_tag_menu::{Action, Entry},
};
use hydrus_store::Store;
use slint::Model as _;
use std::{cell::RefCell, rc::Rc, sync::Arc};
type SearchLauncher = Rc<dyn Fn(LocationContext, TagContext, Vec<Predicate>, bool)>;
thread_local! { static SEARCH_LAUNCHER: RefCell<Option<SearchLauncher>>=RefCell::new(None); }
/// Install the main window's weak search-page launcher on this GUI thread.
pub fn install_search_launcher(launcher: SearchLauncher) {
    SEARCH_LAUNCHER.with(|slot| *slot.borrow_mut() = Some(launcher));
}
/// Clear a closed main window's launcher.
pub fn clear_search_launcher() {
    SEARCH_LAUNCHER.with(|slot| slot.borrow_mut().take());
}

pub(crate) struct TagMenu {
    pub popup: Rc<Popup<Action>>,
    pending: RefCell<Option<Action>>,
    store: Arc<Store>,
    editable: Rc<dyn Fn() -> bool>,
    decorate: Rc<dyn Fn(Action)>,
    refresh: Rc<dyn Fn()>,
    question: Rc<dyn Fn(&str)>,
    error: Rc<dyn Fn(&str)>,
}
impl TagMenu {
    pub fn new(
        store: Arc<Store>,
        editable: Rc<dyn Fn() -> bool>,
        decorate: Rc<dyn Fn(Action)>,
        refresh: Rc<dyn Fn()>,
        question: Rc<dyn Fn(&str)>,
        error: Rc<dyn Fn(&str)>,
    ) -> Rc<Self> {
        Rc::new(Self {
            popup: Popup::new(),
            pending: RefCell::default(),
            store,
            editable,
            decorate,
            refresh,
            question,
            error,
        })
    }
    pub fn open(&self, entries: &[Entry], x: f32, y: f32) {
        if !(self.editable)() || self.pending.borrow().is_some() {
            return;
        }
        let (entries, actions) = main_menu::popup(entries, &|entry| match entry {
            Entry::Item(label, action) => PopupNode::Item(label, action),
            Entry::Menu(label, children) => PopupNode::Menu(label, children),
            Entry::Separator => PopupNode::Separator,
        });
        self.popup.open(entries, actions, x, y);
    }
    pub fn busy(&self) -> bool {
        self.pending.borrow().is_some() || self.popup.model().row_count() > 0
    }
    pub fn close(&self) {
        self.pending.borrow_mut().take();
        self.popup.close();
        (self.question)("");
    }
    pub fn choose(&self, chosen: Option<Chosen<Action>>) {
        if !(self.editable)() {
            self.close();
            return;
        }
        match chosen {
            Some(Chosen::Copy(text)) => crate::copy_to_clipboard(&text),
            Some(Chosen::Action(action)) => {
                if let Some(question) = action.question() {
                    (self.question)(question);
                    *self.pending.borrow_mut() = Some(action);
                } else {
                    self.execute(action);
                }
            }
            None => {}
        }
    }
    pub fn answer(&self, yes: bool) {
        let action = self.pending.borrow_mut().take();
        (self.question)("");
        if yes
            && (self.editable)()
            && let Some(action) = action
        {
            self.execute(action);
        }
    }
    fn execute(&self, action: Action) {
        match action {
            Action::Copy(text) => crate::copy_to_clipboard(&text),
            Action::Launch {
                location,
                context,
                predicates,
                duplicate,
            } => {
                let launcher = SEARCH_LAUNCHER.with(|slot| slot.borrow().clone());
                if let Some(launcher) = launcher {
                    launcher(location, context, predicates, duplicate);
                }
            }
            Action::Decorate { .. } => (self.decorate)(action),
            Action::Favourite { .. } => {
                if let Err(e) = action.persist(&self.store) {
                    (self.error)(&format!("could not update favourite tags: {e}"));
                }
            }
        }
        (self.refresh)();
    }
}
// All shared owners export the same small popup callback surface.
macro_rules! bind {
    ($window:expr,$menu:expr) => {{
        let menu = $menu.clone();
        $window.set_tag_menu_panes(menu.popup.model());
        $window.on_tag_menu_placed({
            let menu = menu.clone();
            move |p, x, y, w| menu.popup.placed(p, x, y, w)
        });
        $window.on_tag_menu_hovered({
            let menu = menu.clone();
            move |p, l, r, t, x| menu.popup.hover(p, l, r, t, x)
        });
        $window.on_tag_menu_clicked({
            let menu = menu.clone();
            move |p, l, r, t, x| {
                let chosen = menu.popup.click(p, l, r, t, x);
                menu.choose(chosen);
            }
        });
        $window.on_tag_menu_dismissed({
            let menu = menu.clone();
            move || menu.close()
        });
        $window.on_tag_menu_answered(move |yes| menu.answer(yes));
    }};
}
pub(crate) use bind;

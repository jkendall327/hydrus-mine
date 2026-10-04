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
    relationships: Rc<RefCell<Option<crate::TagRelationshipsWindow>>>,
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
            relationships: Rc::default(),
            store,
            editable,
            decorate,
            refresh,
            question,
            error,
        })
    }
    pub fn open(&self, entries: &[Entry], x: f32, y: f32) {
        if !(self.editable)() || self.busy() {
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
        self.pending.borrow().is_some()
            || self.popup.model().row_count() > 0
            || self.relationships.borrow().is_some()
    }
    pub fn close(&self) {
        self.pending.borrow_mut().take();
        let child = self.relationships.borrow_mut().take();
        if let Some(child) = child {
            child.invoke_cancel();
        }
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
            Action::Relationship { kind, tags } => {
                match hydrus_gui_model::tag_relationships::Relationships::new_with_tags(
                    self.store.clone(),
                    kind,
                    &tags,
                ) {
                    Ok(model) => {
                        match crate::tag_relationships_window::open(
                            model,
                            &self.relationships,
                            self.refresh.clone(),
                        ) {
                            Ok(child) => *self.relationships.borrow_mut() = Some(child),
                            Err(e) => {
                                (self.error)(&format!("could not open tag relationships: {e}"))
                            }
                        }
                    }
                    Err(e) => (self.error)(&format!("could not load tag relationships: {e}")),
                }
            }
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
            Action::LaunchMany {
                location,
                context,
                pages,
            } => {
                let launcher = SEARCH_LAUNCHER.with(|slot| slot.borrow().clone());
                if let Some(launcher) = launcher {
                    for predicates in pages {
                        if !(self.editable)() {
                            break;
                        }
                        launcher(location.clone(), context.clone(), predicates, false);
                    }
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

#[cfg(test)]
mod tests {
    use super::*;
    use hydrus_core::Tag;
    use hydrus_store::display::RelationKind;
    use slint::{ComponentHandle as _, Model as _};
    use std::cell::Cell;

    #[test]
    fn seeded_relationship_child_apply_refresh_and_owner_close_discard_stale_actions() {
        let _windows = crate::headless::init();
        let legacy = hydrus_testkit::legacy_fixture("basic");
        let dir = tempfile::tempdir().unwrap();
        hydrus_store::import::import_legacy(
            legacy.path(),
            &dir.path().join(hydrus_store::store::DB_FILE_NAME),
        )
        .unwrap();
        let store = Store::open(dir.path()).unwrap();
        let active = Rc::new(Cell::new(true));
        let refreshed = Rc::new(Cell::new(0));
        let menu = TagMenu::new(
            store.clone(),
            Rc::new({
                let active = active.clone();
                move || active.get()
            }),
            Rc::new(|_| {}),
            Rc::new({
                let refreshed = refreshed.clone();
                move || refreshed.set(refreshed.get() + 1)
            }),
            Rc::new(|_| {}),
            Rc::new(|error| panic!("{error}")),
        );
        let tag = Tag::new("parity:context child").unwrap();
        menu.choose(Some(Chosen::Action(Action::Relationship {
            kind: RelationKind::Siblings,
            tags: vec![tag.as_str().to_owned()],
        })));
        assert!(menu.busy());
        let child = menu.relationships.borrow().as_ref().unwrap().clone_strong();
        assert!(child.get_siblings());
        assert_eq!(
            child.get_left_tags().row_data(0).unwrap().text,
            tag.as_str()
        );
        child.invoke_enter_tags(true, "parity:cancelled ideal".into());
        child.invoke_add();
        active.set(false);
        menu.close();
        child.invoke_apply();
        assert!(!menu.busy());
        assert!(
            store
                .read(|conn| hydrus_store::master::tag_id(
                    conn,
                    &Tag::new("parity:cancelled ideal").unwrap()
                ))
                .unwrap()
                .is_none()
        );
        active.set(true);
        menu.choose(Some(Chosen::Action(Action::Relationship {
            kind: RelationKind::Parents,
            tags: vec![tag.as_str().to_owned()],
        })));
        let child = menu.relationships.borrow().as_ref().unwrap().clone_strong();
        assert!(!child.get_siblings());
        child.invoke_enter_tags(true, "parity:applied parent".into());
        child.invoke_add();
        let before = refreshed.get();
        child.invoke_apply();
        assert!(!menu.busy());
        assert_eq!(refreshed.get(), before + 1);
        let id = store
            .read(move |conn| hydrus_store::master::tag_id(conn, &tag))
            .unwrap()
            .unwrap();
        let service = store.snapshot().services.by_name("my tags").unwrap().id;
        assert!(
            !store
                .snapshot()
                .display
                .get(service)
                .ancestors(id)
                .is_empty()
        );
        menu.close();
    }
}

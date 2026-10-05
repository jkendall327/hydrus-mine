//! Each popup captures its caller and typed context; menu ids are never saved.
use std::cell::{Cell, RefCell};
use std::rc::{Rc, Weak};
use std::sync::Arc;

use hydrus_core::{
    ServiceType,
    pages::{PageCollect, PageSort},
    search::context::TagContext,
};
use hydrus_gui_model::{
    options::{Editor, Row, Value},
    sort_cog,
};
use hydrus_store::Store;
use slint::{ComponentHandle as _, ModelRc, VecModel};

use crate::{ContextCogItem, ContextCogServices, MainWindow, OptionsWindow, SearchPage};

#[derive(Clone)]
enum Action {
    Sort(sort_cog::Action),
    Service(hydrus_core::ServiceKey),
    Unmatched(bool),
}

#[derive(Default)]
struct Choices {
    next: i32,
    actions: Vec<(i32, Action)>,
}

impl Choices {
    fn item(&mut self, label: &str, checked: bool, action: Action) -> Option<ContextCogItem> {
        self.next = self.next.checked_add(1)?;
        self.actions.push((self.next, action));
        Some(ContextCogItem {
            label: label.into(),
            checked,
            id: self.next,
        })
    }

    fn action(&self, id: i32) -> Option<Action> {
        self.actions
            .iter()
            .find(|(key, _)| *key == id)
            .map(|(_, action)| action.clone())
    }

    fn services(
        &mut self,
        store: &Store,
        context: &TagContext,
        collect: bool,
    ) -> ContextCogServices {
        let snapshot = store.snapshot();
        let mut groups = [Vec::new(), Vec::new(), Vec::new()];
        for entry in sort_cog::services(store, context) {
            let Some(sort_cog::Action::Service(key)) = entry.action else {
                continue;
            };
            let Ok(service) = snapshot.services.by_key(&key) else {
                continue;
            };
            let group = match service.service_type() {
                ServiceType::LocalTag => 0,
                ServiceType::TagRepository => 1,
                ServiceType::CombinedTag => 2,
                _ => continue,
            };
            let action = if collect {
                Action::Service(key)
            } else {
                Action::Sort(sort_cog::Action::Service(key))
            };
            if let Some(item) = self.item(&entry.label, entry.checked, action) {
                groups[group].push(item);
            }
        }
        let [local, repositories, combined] = groups;
        ContextCogServices {
            local: ModelRc::new(VecModel::from(local)),
            repositories: ModelRc::new(VecModel::from(repositories)),
            combined: ModelRc::new(VecModel::from(combined)),
        }
    }

    fn extra(&mut self, collect: &PageCollect) -> ModelRc<ContextCogItem> {
        ModelRc::new(VecModel::from(
            [(true, "collect into one group"), (false, "leave separate")]
                .into_iter()
                .filter_map(|(on, label)| {
                    self.item(
                        label,
                        collect.collect_unmatched == on,
                        Action::Unmatched(on),
                    )
                })
                .collect::<Vec<_>>(),
        ))
    }

    fn sort_extra(&mut self, store: &Store, sort: &PageSort) -> ModelRc<ContextCogItem> {
        ModelRc::new(VecModel::from(
            sort_cog::entries(store, sort, 1)
                .into_iter()
                .filter_map(|entry| {
                    self.item(&entry.label, entry.checked, Action::Sort(entry.action?))
                })
                .collect::<Vec<_>>(),
        ))
    }
}

struct PageTarget {
    page: Weak<RefCell<SearchPage>>,
    sort: PageSort,
    collect: PageCollect,
    is_collect: bool,
}

/// Bind the search sidebar's two independent menus to the visible page.
pub(crate) fn bind(
    window: &MainWindow,
    page: impl Fn() -> Rc<RefCell<SearchPage>> + Clone + 'static,
    shown: impl Fn(bool) + Clone + 'static,
) {
    let choices: Rc<RefCell<Choices>> = Rc::default();
    let target: Rc<RefCell<Option<PageTarget>>> = Rc::default();
    window.on_context_cog_opened({
        let weak = window.as_weak();
        let choices = choices.clone();
        let target = target.clone();
        let page = page.clone();
        move |is_collect| {
            target.borrow_mut().take();
            let Some(window) = weak.upgrade().filter(|window| window.window().is_visible()) else {
                return;
            };
            let caller = page();
            let caller_ref = caller.borrow();
            if !is_collect && sort_cog::groups(caller_ref.sort()).is_empty() {
                return;
            }
            let mut choices = choices.borrow_mut();
            choices.actions.clear();
            let context = if is_collect {
                &caller_ref.collect().tag_context
            } else {
                &caller_ref.sort().tag_context
            };
            let services = choices.services(caller_ref.store(), context, is_collect);
            if is_collect {
                window.set_collect_cog_services(services);
                window.set_collect_cog_extra(choices.extra(caller_ref.collect()));
            } else {
                window.set_sort_cog_services(services);
                window
                    .set_sort_cog_extra(choices.sort_extra(caller_ref.store(), caller_ref.sort()));
            }
            *target.borrow_mut() = Some(PageTarget {
                page: Rc::downgrade(&caller),
                sort: caller_ref.sort().clone(),
                collect: caller_ref.collect().clone(),
                is_collect,
            });
        }
    });
    window.on_context_cog_chosen({
        let weak = window.as_weak();
        move |is_collect, id| {
            if !weak
                .upgrade()
                .is_some_and(|window| window.window().is_visible())
            {
                return;
            }
            let Some(action) = choices.borrow().action(id) else {
                return;
            };
            let Some(captured) = target.borrow_mut().take() else {
                return;
            };
            let Some(caller) = captured.page.upgrade() else {
                return;
            };
            if captured.is_collect != is_collect || !Rc::ptr_eq(&caller, &page()) {
                return;
            }
            let mut caller = caller.borrow_mut();
            if (is_collect && *caller.collect() != captured.collect)
                || (!is_collect && *caller.sort() != captured.sort)
            {
                return;
            }
            match action {
                Action::Sort(action) if !is_collect => {
                    if !caller.apply_sort_cog(&action) {
                        return;
                    }
                    crate::sort_chosen(&caller);
                }
                Action::Service(service) if is_collect => {
                    let mut collect = caller.collect().clone();
                    collect.tag_context.service = service;
                    caller.set_collect(collect);
                }
                Action::Unmatched(on) if is_collect => {
                    let mut collect = caller.collect().clone();
                    collect.collect_unmatched = on;
                    caller.set_collect(collect);
                }
                _ => return,
            }
            drop(caller);
            shown(true);
        }
    });
}

struct OptionsTarget {
    page: usize,
    row: usize,
    collect: PageCollect,
}

/// Default Collect menus change only the captured Options draft, until Apply.
pub(crate) fn bind_options(
    window: &OptionsWindow,
    editor: &Rc<RefCell<Editor>>,
    store: &Arc<Store>,
    active: &Rc<Cell<bool>>,
    show: impl Fn() + Clone + 'static,
) {
    let choices: Rc<RefCell<Choices>> = Rc::default();
    let target: Rc<RefCell<Option<OptionsTarget>>> = Rc::default();
    window.on_collect_cog_opened({
        let weak = window.as_weak();
        let editor = editor.clone();
        let store = store.clone();
        let active = active.clone();
        let target = target.clone();
        let choices = choices.clone();
        move |row| {
            target.borrow_mut().take();
            if !active.get() {
                return;
            }
            let Some(window) = weak.upgrade() else { return };
            let Ok(row) = usize::try_from(row) else {
                return;
            };
            let editor = editor.borrow();
            let rows = editor.rows();
            let Some(Row::Opt {
                value: Value::Collect(collect),
                ..
            }) = rows.get(row)
            else {
                return;
            };
            let mut choices = choices.borrow_mut();
            choices.actions.clear();
            window.set_collect_cog_services(choices.services(&store, &collect.tag_context, true));
            window.set_collect_cog_extra(choices.extra(collect));
            *target.borrow_mut() = Some(OptionsTarget {
                page: editor.page(),
                row,
                collect: (*collect).clone(),
            });
        }
    });
    window.on_collect_cog_chosen({
        let active = active.clone();
        let editor = editor.clone();
        move |row, id| {
            if !active.get() {
                return;
            }
            let Some(action) = choices.borrow().action(id) else {
                return;
            };
            let Some(captured) = target.borrow_mut().take() else {
                return;
            };
            if usize::try_from(row).ok() != Some(captured.row) {
                return;
            }
            let mut editor = editor.borrow_mut();
            if editor.page() != captured.page {
                return;
            }
            let rows = editor.rows();
            let Some(Row::Opt {
                value: Value::Collect(current),
                ..
            }) = rows.get(captured.row)
            else {
                return;
            };
            if *current != captured.collect {
                return;
            }
            let mut collect = (*current).clone();
            drop(rows);
            match action {
                Action::Service(service) => collect.tag_context.service = service,
                Action::Unmatched(on) => collect.collect_unmatched = on,
                Action::Sort(_) => return,
            }
            editor.collect(captured.row, collect);
            drop(editor);
            show();
        }
    });
}

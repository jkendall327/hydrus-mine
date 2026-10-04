//! Native notebook settings, independent measured tab fits and hierarchy rows.
use crate::{MainWindow, PageTreeRow, Pages, TabFit, TabNames};
use hydrus_core::pages::PageKey;
use hydrus_store::settings::{self, TabPresentationSettings};
use slint::{ComponentHandle as _, ModelRc, VecModel};
use std::{cell::RefCell, rc::Rc};

fn begin(text: &str) -> TabFit {
    let count = i32::try_from(text.chars().count()).unwrap_or(i32::MAX);
    TabFit {
        low: -1,
        high: count,
        candidate: count,
        display: "".into(),
        done: false,
    }
}
fn step(text: &str, mut fit: TabFit, measured: f32, room: f32) -> TabFit {
    if fit.done {
        return fit;
    }
    if measured.is_finite() && room.is_finite() && measured <= room.max(0.0) {
        fit.low = fit.candidate;
        fit.display = hydrus_gui_model::tab_presentation::middle_name(
            text,
            usize::try_from(fit.candidate).unwrap_or(0),
        )
        .into();
    } else {
        fit.high = fit.candidate - 1;
    }
    if fit.low >= fit.high {
        fit.done = true;
    } else {
        fit.candidate = fit.low + (fit.high - fit.low + 1) / 2;
    }
    fit
}

pub(crate) fn bind_names(window: &MainWindow) {
    let names = window.global::<TabNames<'_>>();
    names.on_begin(|text| begin(&text));
    names.on_middle(|text, count| {
        hydrus_gui_model::tab_presentation::middle_name(&text, usize::try_from(count).unwrap_or(0))
            .into()
    });
    names.on_step(|text, fit, measured, room| step(&text, fit, measured, room));
}

pub(crate) fn show(window: &MainWindow, pages: &Pages) {
    let settings: TabPresentationSettings = pages.store().read(settings::get).unwrap_or_default();
    window.set_tab_alignment(settings.alignment.code());
    window.set_page_tree_side(settings.tree_side());
    window.set_navigation_tabs_hidden(settings.tabs_hidden());
    window.set_elide_tab_names(settings.elide_names);
    let mut state = pages.page_tree.borrow_mut();
    state.sync(&pages.session().pages, pages.shown().key);
    let rows: Vec<_> = state
        .visible()
        .iter()
        .map(|node| PageTreeRow {
            key: node.key.to_hex().into(),
            name: node.name.as_str().into(),
            depth: node.depth,
            selected: state.current() == Some(node.key),
            branch: node.branch,
            expanded: state.expanded(node.key),
        })
        .collect();
    let current = rows
        .iter()
        .position(|row| row.selected)
        .and_then(|index| i32::try_from(index).ok())
        .unwrap_or(-1);
    window.set_page_tree(ModelRc::new(VecModel::from(rows)));
    window.set_page_tree_current_index(current);
}

pub(crate) fn bind_tree(window: &MainWindow, pages: &Rc<RefCell<Pages>>) {
    window.on_page_tree_selected({
        let pages = pages.clone();
        let weak = window.as_weak();
        move |text| {
            if let (Some(key), Some(window)) = (PageKey::from_hex(&text), weak.upgrade()) {
                let pages = pages.borrow();
                pages.page_tree.borrow_mut().select(key);
                show(&window, &pages);
            }
        }
    });
    window.on_page_tree_toggled({
        let pages = pages.clone();
        let weak = window.as_weak();
        move |text| {
            if let (Some(key), Some(window)) = (PageKey::from_hex(&text), weak.upgrade()) {
                let pages = pages.borrow();
                pages.page_tree.borrow_mut().toggle(key);
                show(&window, &pages);
            }
        }
    });
    window.on_page_tree_navigate({
        let pages = pages.clone();
        let weak = window.as_weak();
        move |action| {
            if let Some(window) = weak.upgrade() {
                let activated = {
                    let pages = pages.borrow();
                    let result = pages.page_tree.borrow_mut().navigate(action);
                    show(&window, &pages);
                    result
                };
                if let Some(key) = activated {
                    window.invoke_page_tree_chosen(key.to_hex().into());
                }
            }
        }
    });
}

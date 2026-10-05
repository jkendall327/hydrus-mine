//! A duplicates page's filtering tab (hydrus-gui-model's
//! `duplicates_filtering`): editing the page's pair search and filter sort,
//! counting its pairs off the UI thread, showing a random potential group in
//! the page, and setting the shown files' relationship after asking.
use std::cell::RefCell;
use std::rc::Rc;

use hydrus_gui_model::duplicates_filtering::{
    self as model, GROUP_MODES, KINDS, PIXEL, SET_BUTTONS, SORTS, Which,
};
use hydrus_search::{TextContext, predicate_text};
use slint::{ComponentHandle as _, ModelRc, SharedString, VecModel};

use crate::page::SearchPage;
use crate::{DuplicatesFiltering, MainWindow, SessionDialog};

fn strings(items: impl IntoIterator<Item = String>) -> ModelRc<SharedString> {
    ModelRc::new(VecModel::from(
        items
            .into_iter()
            .map(SharedString::from)
            .collect::<Vec<_>>(),
    ))
}

thread_local! {
    // (the latest count line, and the question asked, kept while shown)
    static COUNT: RefCell<String> = const { RefCell::new(String::new()) };
    static QUESTION: RefCell<Option<SessionDialog>> = const { RefCell::new(None) };
}

fn index<T: PartialEq>(items: &[(T, &str)], value: &T) -> i32 {
    items
        .iter()
        .position(|(v, _)| v == value)
        .and_then(|i| i32::try_from(i).ok())
        .unwrap_or(0)
}

/// Show the page's filtering tab, if it is a duplicates page.
pub(crate) fn show(window: &MainWindow, page: &SearchPage) {
    let Some(d) = page.duplicates() else {
        return;
    };
    let text = |p: &hydrus_search::Predicate| predicate_text(p, &TextContext::default());
    let directions = model::directions(d.order);
    window.set_duplicates_filtering(DuplicatesFiltering {
        kinds: strings(KINDS.iter().map(|(_, s)| (*s).to_owned())),
        kind: index(&KINDS, &d.search.kind),
        search_1: strings(d.search.search_1.predicates.iter().map(text)),
        search_2: strings(d.search.search_2.predicates.iter().map(text)),
        second_shown: model::second_search_shown(d),
        distance: i32::try_from(d.search.max_hamming_distance).unwrap_or(0),
        distance_enabled: model::distance_enabled(d),
        pixels: strings(PIXEL.iter().map(|(_, s)| (*s).to_owned())),
        pixel: index(&PIXEL, &d.search.pixel_duplicates),
        count: COUNT.with(|c| c.borrow().clone()).into(),
        sorts: strings(SORTS.iter().map(|(_, s)| (*s).to_owned())),
        sort: index(&SORTS, &d.order),
        directions: strings(directions.iter().map(|(s, _)| (*s).to_owned())),
        direction: directions
            .iter()
            .position(|(_, asc)| *asc == d.ascending)
            .and_then(|i| i32::try_from(i).ok())
            .unwrap_or(0),
        groups: strings(GROUP_MODES.iter().map(|(s, _)| (*s).to_owned())),
        group: i32::from(d.group_mode),
        set_buttons: strings(SET_BUTTONS.iter().map(|(s, _)| (*s).to_owned())),
    });
}

/// Count the page's pairs off the UI thread, showing the line when done.
fn recount(window: &MainWindow, page: &SearchPage) {
    let Some(d) = page.duplicates().cloned() else {
        return;
    };
    COUNT.with(|c| *c.borrow_mut() = "initialising\u{2026}".into());
    let store = page.store().clone();
    let weak = window.as_weak();
    let _ = std::thread::Builder::new()
        .name("duplicates-count".into())
        .spawn(move || {
            let line = match model::count(&store, &d) {
                Ok((total, matching)) => model::count_text(total, matching),
                Err(error) => error.to_string(),
            };
            let _ = weak.upgrade_in_event_loop(move |window| {
                COUNT.with(|c| c.borrow_mut().clone_from(&line));
                let mut filtering = window.get_duplicates_filtering();
                filtering.count = line.into();
                window.set_duplicates_filtering(filtering);
            });
        });
}

pub(crate) fn bind(
    window: &MainWindow,
    page: impl Fn() -> Rc<RefCell<SearchPage>> + 'static,
    shown: impl Fn(bool) + 'static,
) {
    let weak = window.as_weak();
    let shown = Rc::new(shown);
    window.on_duplicates_filtering_action(move |what, n| {
        let Some(window) = weak.upgrade() else {
            return;
        };
        let page = page();
        let index = usize::try_from(n).ok();
        let mut search_changed = false;
        {
            let mut p = page.borrow_mut();
            let Some(d) = p.duplicates_mut() else {
                return;
            };
            match what.as_str() {
                "kind" => {
                    if let Some((kind, _)) = index.and_then(|i| KINDS.get(i)) {
                        d.search.kind = *kind;
                        search_changed = true;
                    }
                }
                "pixel" => {
                    if let Some((pixel, _)) = index.and_then(|i| PIXEL.get(i)) {
                        d.search.pixel_duplicates = *pixel;
                        search_changed = true;
                    }
                }
                "distance" => {
                    d.search.max_hamming_distance = u32::try_from(n.clamp(0, 64)).unwrap_or(0);
                    search_changed = true;
                }
                "sort" => {
                    // (every non-random sort offers both directions, so the
                    // direction stays)
                    if let Some((order, _)) = index.and_then(|i| SORTS.get(i)) {
                        d.order = *order;
                    }
                }
                "direction" => {
                    if let Some((_, asc)) = index.and_then(|i| model::directions(d.order).get(i)) {
                        d.ascending = *asc;
                    }
                }
                "group" => d.group_mode = n == 1,
                "add 1" | "add 2" => {
                    let (which, text) = if what == "add 1" {
                        (Which::First, window.get_duplicates_filter_input_1())
                    } else {
                        (Which::Second, window.get_duplicates_filter_input_2())
                    };
                    if !text.trim().is_empty() && model::add_predicates(d, which, &text).is_ok() {
                        if which == Which::First {
                            window.set_duplicates_filter_input_1(SharedString::new());
                        } else {
                            window.set_duplicates_filter_input_2(SharedString::new());
                        }
                        search_changed = true;
                    }
                }
                "remove 1" | "remove 2" => {
                    let which = if what == "remove 1" {
                        Which::First
                    } else {
                        Which::Second
                    };
                    if let Some(i) = index {
                        search_changed = model::remove_predicate(d, which, i).is_some();
                    }
                }
                _ => {}
            }
        }
        match what.as_str() {
            "refresh count" => recount(&window, &page.borrow()),
            "random" => {
                let p = page.borrow();
                let Some(d) = p.duplicates().cloned() else {
                    return;
                };
                let store = p.store().clone();
                drop(p);
                match model::random_group(&store, &d) {
                    Ok(files) => {
                        page.borrow_mut().show_importers_files(files);
                        shown(true);
                    }
                    Err(error) => eprintln!("could not find random potential duplicates: {error}"),
                }
            }
            "set" => {
                let Some((_, relationship)) = index.and_then(|i| SET_BUTTONS.get(i)).copied()
                else {
                    return;
                };
                let p = page.borrow();
                let files = p.results().to_vec();
                let store = p.store().clone();
                drop(p);
                let pairs = model::pairs(&files, relationship).len();
                if pairs == 0 {
                    return;
                }
                let advanced = store
                    .read(hydrus_store::settings::get::<hydrus_store::settings::AdvancedMode>)
                    .is_ok_and(|a| a.0);
                let (message, yes, no) =
                    model::question(relationship, advanced, files.len(), pairs);
                let Ok(dialog) = SessionDialog::new() else {
                    return;
                };
                dialog.set_window_title("Are you sure?".into());
                dialog.set_message(message.into());
                dialog.set_yes_label(yes.into());
                dialog.set_no_label(no.into());
                let page = page.clone();
                let shown = shown.clone();
                let dialog_weak = dialog.as_weak();
                dialog.on_answered(move |answer| {
                    if let Some(d) = dialog_weak.upgrade() {
                        let _ = d.hide();
                    }
                    if !answer {
                        return;
                    }
                    if let Err(error) =
                        model::set_duplicates(&store, &files, relationship, advanced)
                    {
                        eprintln!("could not set the duplicates: {error}");
                        return;
                    }
                    // (as the reference does, then show another random group)
                    let p = page.borrow();
                    let Some(d) = p.duplicates().cloned() else {
                        return;
                    };
                    drop(p);
                    if let Ok(files) = model::random_group(&store, &d) {
                        page.borrow_mut().show_importers_files(files);
                        shown(true);
                    }
                });
                let dialog_weak = dialog.as_weak();
                dialog.on_cancelled(move || {
                    if let Some(d) = dialog_weak.upgrade() {
                        let _ = d.hide();
                    }
                });
                if dialog.show().is_ok() {
                    QUESTION.with(|q| *q.borrow_mut() = Some(dialog));
                }
            }
            _ => {}
        }
        if search_changed {
            recount(&window, &page.borrow());
        }
        show(&window, &page.borrow());
    });
}

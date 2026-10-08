//! A duplicates page's filtering tab (hydrus-gui-model's
//! `duplicates_filtering`): editing the page's pair search and filter sort,
//! counting its pairs off the UI thread a block at a time (play/pause,
//! refresh and the cog), showing a random potential group in
//! the page, and setting the shown files' relationship after asking.
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use hydrus_core::duplicates::DuplicatesSearch;
use hydrus_gui_model::duplicates_count::{
    self as counting, BLOCK_GUIDELINE, COG_FILE_SEARCH_OPTIMISATION, COG_STARTS_PAUSED,
    COG_STOPS_TO_ESTIMATE, Gate, Handle, StoreSource,
};
use hydrus_gui_model::duplicates_filtering::{
    self as model, GROUP_MODES, KINDS, PIXEL, SET_BUTTONS, SORTS, Which,
};
use hydrus_search::{TextContext, predicate_text};
use hydrus_store::duplicates::cache::PairRow;
use hydrus_store::settings::{self, PotentialPairsCountOptions};
use slint::{ComponentHandle as _, ModelRc, SharedString, Timer, TimerMode, VecModel};

use crate::page::SearchPage;
use crate::{DuplicatesFiltering, MainWindow, SessionDialog};

/// The "Are you sure?" question a set button last asked, while it is open
/// (for interaction tests).
pub fn question_opened() -> Option<SessionDialog> {
    QUESTION.with(|q| {
        q.borrow()
            .as_ref()
            .map(slint::ComponentHandle::clone_strong)
    })
}

fn strings(items: impl IntoIterator<Item = String>) -> ModelRc<SharedString> {
    ModelRc::new(VecModel::from(
        items
            .into_iter()
            .map(SharedString::from)
            .collect::<Vec<_>>(),
    ))
}

/// The page's pair count while the duplicates page is shown: the worker
/// counting the page's search, the search it counts and the timer that
/// shows how far it has got.
struct Count {
    handle: Handle<PairRow>,
    search: Arc<Mutex<DuplicatesSearch>>,
    _timer: Timer,
}

thread_local! {
    // (the page's count, and the question asked, kept while shown)
    static COUNT: RefCell<Option<Count>> = const { RefCell::new(None) };
    static QUESTION: RefCell<Option<SessionDialog>> = const { RefCell::new(None) };
}

/// The count's cog options as stored.
fn count_options(page: &SearchPage) -> PotentialPairsCountOptions {
    page.store()
        .read(settings::get::<PotentialPairsCountOptions>)
        .unwrap_or_default()
}

fn model_options(options: PotentialPairsCountOptions) -> counting::Options {
    counting::Options {
        stops_to_estimate: options.stops_to_estimate,
        file_search_optimisation: options.file_search_optimisation,
    }
}

/// The count of the page's search: started when the page is first shown,
/// and counting afresh when the search it counts is not the page's.
fn ensure_count(window: &MainWindow, page: &SearchPage, d: &hydrus_core::pages::DuplicatesPage) {
    COUNT.with(|count| {
        let mut count = count.borrow_mut();
        if let Some(active) = count.as_ref() {
            let mut counted = active.search.lock().unwrap_or_else(PoisonError::into_inner);
            if *counted != d.search {
                let domain_changed = counted.search_1.location != d.search.search_1.location;
                *counted = d.search.clone();
                drop(counted);
                active.handle.search_changed(domain_changed);
            }
            return;
        }
        let options = count_options(page);
        let search = Arc::new(Mutex::new(d.search.clone()));
        let gate = Gate::here();
        let handle = Handle::start(
            gate.as_ref().map_or(BLOCK_GUIDELINE, |g| g.guideline()),
            options.starts_paused,
            // (a test gate searches the pairs in the order given)
            gate.is_none(),
            model_options(options),
            StoreSource::new(page.store().clone(), Arc::clone(&search)),
            gate,
        );
        let weak = window.as_weak();
        let timer = Timer::default();
        timer.start(TimerMode::Repeated, Duration::from_millis(50), move || {
            let Some(window) = weak.upgrade() else {
                return;
            };
            COUNT.with(|count| {
                let count = count.borrow();
                let Some(active) = count.as_ref() else {
                    return;
                };
                let shot = active.handle.snapshot();
                let mut filtering = window.get_duplicates_filtering();
                if filtering.count != shot.label.as_str() || filtering.count_paused != shot.paused {
                    filtering.count = shot.label.into();
                    filtering.count_paused = shot.paused;
                    window.set_duplicates_filtering(filtering);
                }
            });
        });
        *count = Some(Count {
            handle,
            search,
            _timer: timer,
        });
    });
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
    ensure_count(window, page, d);
    let (label, paused) = COUNT.with(|count| {
        count
            .borrow()
            .as_ref()
            .map(|c| c.handle.snapshot())
            .map_or((String::new(), false), |s| (s.label, s.paused))
    });
    let options = count_options(page);
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
        count: label.into(),
        count_paused: paused,
        count_cog: strings([
            COG_STARTS_PAUSED.0.to_owned(),
            COG_STOPS_TO_ESTIMATE.0.to_owned(),
            COG_FILE_SEARCH_OPTIMISATION.0.to_owned(),
        ]),
        count_ticks: ModelRc::new(VecModel::from(vec![
            options.starts_paused,
            options.stops_to_estimate,
            options.file_search_optimisation,
        ])),
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

fn with_count(f: impl FnOnce(&Count)) {
    COUNT.with(|count| {
        if let Some(active) = count.borrow().as_ref() {
            f(active);
        }
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
        {
            let mut p = page.borrow_mut();
            let Some(d) = p.duplicates_mut() else {
                return;
            };
            match what.as_str() {
                "kind" => {
                    if let Some((kind, _)) = index.and_then(|i| KINDS.get(i)) {
                        d.search.kind = *kind;
                    }
                }
                "pixel" => {
                    if let Some((pixel, _)) = index.and_then(|i| PIXEL.get(i)) {
                        d.search.pixel_duplicates = *pixel;
                    }
                }
                "distance" => {
                    d.search.max_hamming_distance = u32::try_from(n.clamp(0, 64)).unwrap_or(0);
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
                    }
                }
                "remove 1" | "remove 2" => {
                    let which = if what == "remove 1" {
                        Which::First
                    } else {
                        Which::Second
                    };
                    if let Some(i) = index {
                        let _ = model::remove_predicate(d, which, i);
                    }
                }
                _ => {}
            }
        }
        match what.as_str() {
            "pause count" => with_count(|c| c.handle.pause_play()),
            "refresh count" => with_count(|c| c.handle.refresh()),
            "count option" => {
                let store = page.borrow().store().clone();
                let mut options = count_options(&page.borrow());
                match index {
                    Some(0) => options.starts_paused = !options.starts_paused,
                    Some(1) => options.stops_to_estimate = !options.stops_to_estimate,
                    Some(2) => options.file_search_optimisation = !options.file_search_optimisation,
                    _ => {}
                }
                if let Err(error) = store.write(move |ctx| settings::set(ctx.conn(), &options)) {
                    eprintln!("could not save the count's options: {error}");
                }
                with_count(|c| c.handle.set_options(model_options(options)));
            }
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
        // (a changed search is seen by `show`, which counts it afresh)
        show(&window, &page.borrow());
    });
}

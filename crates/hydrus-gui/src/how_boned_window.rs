//! Database > how boned am I? (hydrus-gui-model's `how_boned`): a search
//! (file domain and typed predicates) whose statistics load on a worker
//! thread, the latest search replacing older ones, with a stop button.
use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::sync::Arc;

use hydrus_core::ServiceKey;
use hydrus_core::search::context::{FileSearchContext, LocationContext, TagContext};
use hydrus_core::service::builtin_keys;
use hydrus_gui_model::how_boned as model;
use hydrus_search::{TextContext, parse_api_search, predicate_text};
use hydrus_store::Store;
use hydrus_store::content::DomainRoles;
use slint::{ComponentHandle as _, ModelRc, SharedString, VecModel};

use crate::{BonesRow, HowBonedWindow};

pub type Slot = Rc<RefCell<Option<HowBonedWindow>>>;

fn strings(items: impl IntoIterator<Item = String>) -> ModelRc<SharedString> {
    ModelRc::new(VecModel::from(
        items
            .into_iter()
            .map(SharedString::from)
            .collect::<Vec<_>>(),
    ))
}

struct State {
    search: RefCell<FileSearchContext>,
    /// Bumped by each search and by stop: an older result is dropped.
    generation: Arc<std::sync::atomic::AtomicU64>,
    loading: Cell<bool>,
}

fn show(
    window: &HowBonedWindow,
    stats: &hydrus_store::stats::BonedStats,
    search: &FileSearchContext,
) {
    let now = hydrus_core::time::TimestampMs::now().secs();
    let files = model::files_tab(stats, now, jiff::Zoned::now().offset());
    window.set_no_files(
        if files.rows.is_empty() {
            "No files!"
        } else {
            ""
        }
        .into(),
    );
    window.set_files(ModelRc::new(VecModel::from(
        files
            .rows
            .into_iter()
            .map(|row| BonesRow {
                cells: strings(row),
            })
            .collect::<Vec<_>>(),
    )));
    window.set_earliest(files.earliest.unwrap_or_default().into());
    window.set_views(strings(model::views_tab(stats)));
    window.set_duplicates(strings(model::duplicates_tab(stats)));
    window.set_bones_text(
        model::special_message(stats, search)
            .unwrap_or_default()
            .into(),
    );
}

fn loading(window: &HowBonedWindow) {
    window.set_loading_text("loading\u{2026}".into());
    window.set_loading(true);
    window.set_files(ModelRc::default());
    window.set_no_files("".into());
    window.set_earliest("".into());
    window.set_views(ModelRc::default());
    window.set_duplicates(ModelRc::default());
}

/// Open the window (or bring the open one forward).
pub fn open(store: &Arc<Store>, slot: &Slot) -> Result<(), String> {
    if let Some(window) = slot.borrow().as_ref()
        && window.window().is_visible()
    {
        return window.show().map_err(|e| e.to_string());
    }
    let window = HowBonedWindow::new().map_err(|e| e.to_string())?;
    let snapshot = store.snapshot();
    let roles = DomainRoles::new(&snapshot.services).map_err(|e| e.to_string())?;
    let domains: Vec<(ServiceKey, String)> = snapshot
        .services
        .all()
        .filter(|s| {
            roles.local.contains(&s.id)
                || [
                    roles.combined_local_media,
                    roles.local_file_storage,
                    roles.trash,
                ]
                .contains(&s.id)
        })
        .map(|s| (s.key.clone(), s.name.clone()))
        .collect();
    let initial = domains
        .iter()
        .position(|d| d.0.as_bytes() == builtin_keys::COMBINED_LOCAL_FILE_DOMAINS)
        .unwrap_or(0);
    window.set_domains(strings(domains.iter().map(|d| d.1.clone())));
    window.set_domain_index(i32::try_from(initial).unwrap_or(0));
    window.set_duplicates_note(model::DUPLICATES_NOTE.into());
    let state = Rc::new(State {
        search: RefCell::new(FileSearchContext {
            location: LocationContext::new(domains.get(initial).map(|d| d.0.clone()), []),
            tags: TagContext::new(ServiceKey::new(builtin_keys::COMBINED_TAG), true, true),
            predicates: Vec::new(),
        }),
        generation: Arc::default(),
        loading: Cell::new(false),
    });
    let refresh = Rc::new({
        let state = state.clone();
        let weak = window.as_weak();
        let store = store.clone();
        move || {
            let Some(window) = weak.upgrade() else {
                return;
            };
            window.set_predicates(strings(
                state
                    .search
                    .borrow()
                    .predicates
                    .iter()
                    .map(|p| predicate_text(p, &TextContext::default())),
            ));
            loading(&window);
            state.loading.set(true);
            let generation = state
                .generation
                .fetch_add(1, std::sync::atomic::Ordering::SeqCst)
                + 1;
            let counter = state.generation.clone();
            let search = state.search.borrow().clone();
            let store = store.clone();
            let weak = weak.clone();
            std::thread::spawn(move || {
                let result = model::boned(&store, &search);
                let _ = slint::invoke_from_event_loop(move || {
                    if counter.load(std::sync::atomic::Ordering::SeqCst) != generation {
                        return;
                    }
                    let Some(window) = weak.upgrade() else {
                        return;
                    };
                    window.set_loading(false);
                    match result {
                        Ok(stats) => {
                            window.set_loading_text("".into());
                            show(&window, &stats, &search);
                        }
                        Err(error) => window.set_loading_text(error.to_string().into()),
                    }
                });
            });
        }
    });
    window.on_refresh({
        let refresh = refresh.clone();
        move || refresh()
    });
    window.on_cancel_search({
        let state = state.clone();
        let weak = window.as_weak();
        move || {
            state
                .generation
                .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            if let Some(window) = weak.upgrade() {
                window.set_loading(false);
                window.set_loading_text("cancelled!".into());
            }
        }
    });
    window.on_domain_chosen({
        let state = state.clone();
        let refresh = refresh.clone();
        move |index| {
            if let Some((key, _)) = usize::try_from(index).ok().and_then(|i| domains.get(i)) {
                state.search.borrow_mut().location = LocationContext::new([key.clone()], []);
                refresh();
            }
        }
    });
    window.on_enter({
        let state = state.clone();
        let refresh = refresh.clone();
        let weak = window.as_weak();
        move || {
            let Some(window) = weak.upgrade() else {
                return;
            };
            let input = window.get_input();
            if input.trim().is_empty() {
                return;
            }
            if let Ok(predicates) = parse_api_search(&serde_json::json!([input.as_str()])) {
                let mut search = state.search.borrow_mut();
                for predicate in predicates {
                    if !search.predicates.contains(&predicate) {
                        search.predicates.push(predicate);
                    }
                }
                drop(search);
                window.set_input(SharedString::new());
                refresh();
            }
        }
    });
    window.on_remove({
        let state = state.clone();
        let refresh = refresh.clone();
        move |index| {
            let Ok(index) = usize::try_from(index) else {
                return;
            };
            let removed = {
                let mut search = state.search.borrow_mut();
                (index < search.predicates.len()).then(|| search.predicates.remove(index))
            };
            if removed.is_some() {
                refresh();
            }
        }
    });
    window.window().on_close_requested({
        let state = state.clone();
        move || {
            state
                .generation
                .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            slint::CloseRequestResponse::HideWindow
        }
    });
    window.show().map_err(|e| e.to_string())?;
    refresh();
    *slot.borrow_mut() = Some(window);
    Ok(())
}

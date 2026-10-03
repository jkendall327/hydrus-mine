//! A duplicates page's sidebar tabs, bound (`ui/duplicates_page.slint`):
//! the preparation tab shows how far `hydrus serve`'s similar files search
//! has got (hydrus-gui-model's [`preparation`]) and sets its distance,
//! whether it runs, and whether it works hard; the auto-resolution tab
//! lists the rules ([`rule_progress`], [`rule_status`]), pauses and plays
//! them, and resets them, asking first. They read the store afresh each
//! time the page is shown or they change it.

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::rc::Rc;
use std::sync::Arc;

use slint::{ModelRc, SharedString, VecModel};

use hydrus_store::Store;
use hydrus_store::duplicates::auto::{self, AutoResolutionSettings, PairStatus, Rule};
use hydrus_store::settings;
use hydrus_store::similar::{self, SimilarFilesSettings};

use crate::duplicates_page::{
    RESET_QUESTION, Reset, distance_label, preparation, reset_question, rule_progress, rule_status,
};
use crate::list_selection::ListSelection;
use crate::{DuplicatesData, MainWindow, SearchPage, TableRow};

/// What the sidebar's question panel waits on.
enum Asking {
    DeletePairs,
    /// A reset of these rules.
    Reset(Reset, Vec<i64>),
}

/// A rule as read: its id, itself, and its pairs by status.
type ReadRule = (i64, Rule, BTreeMap<PairStatus, u64>);

#[derive(Default)]
struct State {
    store: Option<Arc<Store>>,
    rules: Vec<ReadRule>,
    selection: ListSelection<i64>,
    asking: Option<Asking>,
}

/// The sidebar's state, for the page shown.
#[derive(Default)]
pub(crate) struct Sidebar {
    state: RefCell<State>,
    /// The auto-resolution rules editor's windows.
    pub(crate) rules_editor: crate::auto_resolution_rules_window::Slots,
}

impl Sidebar {
    /// Show the page's sidebar, if it is a duplicates page, read afresh.
    pub(crate) fn show(&self, window: &MainWindow, page: &SearchPage) {
        if page.duplicates().is_none() {
            return;
        }
        let store = page.store().clone();
        let mut state = self.state.borrow_mut();
        if state.store.as_ref().is_none_or(|s| !Arc::ptr_eq(s, &store)) {
            *state = State {
                store: Some(store.clone()),
                ..State::default()
            };
        }
        let read = store.read(|conn| {
            let rules: Vec<ReadRule> = auto::rules(conn)?
                .into_iter()
                .map(|(id, rule)| Ok((id, rule, auto::counts(conn, id)?)))
                .collect::<hydrus_store::Result<_>>()?;
            Ok((
                similar::search_status_counts(conn)?,
                settings::get::<SimilarFilesSettings>(conn)?,
                settings::get::<AutoResolutionSettings>(conn)?,
                rules,
            ))
        });
        let (searched, similar_settings, auto_settings, mut rules) = match read {
            Ok(read) => read,
            Err(e) => {
                eprintln!("could not read the duplicates page's numbers: {e}");
                return;
            }
        };
        rules.sort_by_key(|(_, rule, _)| hydrus_core::sort::human_sort_key(&rule.name));
        let ids: Vec<i64> = rules.iter().map(|r| r.0).collect();
        state.rules = rules;
        // (rules gone are no longer selected)
        let gone: Vec<i64> = state
            .selection
            .in_order(&ids)
            .into_iter()
            .filter(|id| !ids.contains(id))
            .collect();
        for id in gone {
            state.selection.forget(id);
        }
        // (the reference hides the "needs work" percentage when nearly
        // caught up, by default)
        let prep = preparation(&searched, similar_settings.search_distance, true);
        let able = auto_settings.during_active || auto_settings.during_idle;
        let rows: Vec<TableRow> = state
            .rules
            .iter()
            .map(|(id, rule, counts)| {
                let cells: Vec<SharedString> = vec![
                    rule.name.clone().into(),
                    rule_progress(counts).into(),
                    rule_status(rule, counts, able).into(),
                ];
                TableRow {
                    cells: ModelRc::new(VecModel::from(cells)),
                    selected: state.selection.is_selected(*id),
                }
            })
            .collect();
        window.set_duplicates_rules(ModelRc::new(VecModel::from(rows)));
        let (title, message, choices) = match &state.asking {
            None => (String::new(), String::new(), Vec::new()),
            Some(Asking::DeletePairs) => (
                "Are you sure?".to_owned(),
                RESET_QUESTION.to_owned(),
                vec!["yes".to_owned(), "no".to_owned()],
            ),
            Some(Asking::Reset(which, rules)) => (
                "Are you sure?".to_owned(),
                reset_question(*which, rules.len()),
                vec!["yes".to_owned(), "no".to_owned()],
            ),
        };
        let choices: Vec<SharedString> = choices.into_iter().map(Into::into).collect();
        #[allow(clippy::cast_precision_loss)] // (file counts)
        let fraction = prep.gauge.0 as f32 / prep.gauge.1.max(1) as f32;
        window.set_duplicates(DuplicatesData {
            preparation_name: prep.page_name.into(),
            eligible: prep.eligible.into(),
            distance: i32::try_from(similar_settings.search_distance).unwrap_or(0),
            distance_label: distance_label(similar_settings.search_distance).into(),
            searched: prep.searched.into(),
            searched_fraction: fraction,
            can_start: prep.can_start,
            working_hard: similar_settings.work_hard && prep.can_start,
            search_during_idle: similar_settings.during_idle,
            search_during_active: similar_settings.during_active,
            rules_during_idle: auto_settings.during_idle,
            rules_during_active: auto_settings.during_active,
            any_rule_selected: !state.selection.is_empty(),
            asking: state.asking.is_some(),
            asking_title: title.into(),
            asking_message: message.into(),
            asking_choices: ModelRc::new(VecModel::from(choices)),
        });
    }

    /// The rules a reset works on: the selected, or every rule.
    fn reset_targets(&self) -> Vec<i64> {
        let state = self.state.borrow();
        let ids: Vec<i64> = state.rules.iter().map(|r| r.0).collect();
        let selected = state.selection.in_order(&ids);
        if selected.is_empty() { ids } else { selected }
    }

    /// Do what the sidebar asked (`action`'s name and arguments).
    fn act(&self, what: &str, n: i32, ctrl: bool, shift: bool) {
        let Some(store) = self.state.borrow().store.clone() else {
            return;
        };
        let flip_similar = |f: fn(&mut SimilarFilesSettings)| {
            write(&store, move |conn| {
                let mut s: SimilarFilesSettings = settings::get(conn)?;
                f(&mut s);
                settings::set(conn, &s)
            });
        };
        let flip_auto = |f: fn(&mut AutoResolutionSettings)| {
            write(&store, move |conn| {
                let mut s: AutoResolutionSettings = settings::get(conn)?;
                f(&mut s);
                settings::set(conn, &s)
            });
        };
        match what {
            "distance" => {
                let distance = u32::try_from(n).unwrap_or(0);
                write(&store, move |conn| {
                    let mut s: SimilarFilesSettings = settings::get(conn)?;
                    s.search_distance = distance;
                    settings::set(conn, &s)
                });
            }
            "work hard" => {
                let hard = n != 0;
                write(&store, move |conn| {
                    let mut s: SimilarFilesSettings = settings::get(conn)?;
                    s.work_hard = hard;
                    settings::set(conn, &s)
                });
            }
            "search during idle" => flip_similar(|s| s.during_idle = !s.during_idle),
            "search during active" => flip_similar(|s| s.during_active = !s.during_active),
            "rules during idle" => flip_auto(|s| s.during_idle = !s.during_idle),
            "rules during active" => flip_auto(|s| s.during_active = !s.during_active),
            "delete pairs" => self.state.borrow_mut().asking = Some(Asking::DeletePairs),
            "reset search" | "reset test" | "reset denied" => {
                let which = match what {
                    "reset search" => Reset::Search,
                    "reset test" => Reset::Test,
                    _ => Reset::Denied,
                };
                let targets = self.reset_targets();
                if !targets.is_empty() {
                    self.state.borrow_mut().asking = Some(Asking::Reset(which, targets));
                }
            }
            "edit rules" => {
                if let Err(e) = crate::auto_resolution_rules_window::open(
                    &store,
                    &self.rules_editor,
                    Rc::new(|| {}),
                ) {
                    eprintln!("could not open the rules: {e}");
                }
            }
            "rule" => {
                let mut state = self.state.borrow_mut();
                let ids: Vec<i64> = state.rules.iter().map(|r| r.0).collect();
                if let Ok(row) = usize::try_from(n) {
                    state.selection.click(&ids, row, ctrl, shift);
                }
            }
            "pause rules" => {
                let state = self.state.borrow();
                let ids: Vec<i64> = state.rules.iter().map(|r| r.0).collect();
                let flips: Vec<(i64, bool)> = state
                    .selection
                    .in_order(&ids)
                    .into_iter()
                    .filter_map(|id| {
                        state
                            .rules
                            .iter()
                            .find(|r| r.0 == id)
                            .map(|r| (id, !r.1.paused))
                    })
                    .collect();
                drop(state);
                write(&store, move |conn| {
                    for &(id, paused) in &flips {
                        auto::set_paused(conn, id, paused)?;
                    }
                    Ok(())
                });
            }
            "chosen" => {
                let asking = self.state.borrow_mut().asking.take();
                if n == 0 {
                    match asking {
                        Some(Asking::DeletePairs) => write(&store, similar::delete_potential_pairs),
                        Some(Asking::Reset(which, rules)) => write(&store, move |conn| {
                            for &id in &rules {
                                match which {
                                    Reset::Search => auto::reset_search_progress(conn, id)?,
                                    Reset::Test => auto::reset_test_progress(conn, id)?,
                                    Reset::Denied => auto::reset_denied(conn, id)?,
                                }
                            }
                            Ok(())
                        }),
                        None => {}
                    }
                }
            }
            "cancelled" => self.state.borrow_mut().asking = None,
            // ("refresh": shown again below)
            _ => {}
        }
    }
}

/// Write to the store, saying so if it fails.
fn write(
    store: &Store,
    f: impl FnOnce(&rusqlite::Connection) -> hydrus_store::Result<()> + Send + 'static,
) {
    if let Err(e) = store.write(move |ctx| f(ctx.conn())) {
        eprintln!("could not change the duplicates settings: {e}");
    }
}

/// Bind the sidebar's actions on the main window; `page` gives the page
/// shown.
pub(crate) fn bind(
    window: &MainWindow,
    sidebar: &Rc<Sidebar>,
    page: impl Fn() -> Rc<RefCell<SearchPage>> + 'static,
) {
    let weak = slint::ComponentHandle::as_weak(window);
    let sidebar = sidebar.clone();
    window.on_duplicates_action(move |what, n, ctrl, shift| {
        sidebar.act(&what, n, ctrl, shift);
        if let Some(window) = weak.upgrade() {
            sidebar.show(&window, &page().borrow());
        }
    });
}

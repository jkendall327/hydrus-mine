//! The duplicates auto-resolution rule editor's "preview" tab, bound (the
//! reference's `PreviewPanel`; its words are hydrus-gui-model's
//! `auto_resolution_preview`): when shown, the rule as edited is searched
//! for a sample of its pairs and each is tested, off the UI thread, those
//! that pass listed with what would be done to them and those that fail
//! listed apart. A changed search fetches again; changed comparators or
//! actions test again.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use crossbeam_channel::{Receiver, Sender};
use slint::{ComponentHandle as _, ModelRc, VecModel};

use hydrus_core::HashId;
use hydrus_duplicates::engine::{self, PreviewSearch};
use hydrus_store::Store;
use hydrus_store::duplicates::auto::Rule;

use crate::auto_resolution_preview::{
    FETCHING, READY_TO_FETCH, READY_TO_PREVIEW, READY_TO_TEST, TESTING, listed, pass_cell, problem,
    searched, still_to_test,
};
use crate::{AutoResolutionRuleWindow, PairRow};

/// What the worker says, for the run it was started as.
enum Said {
    Searched(u64, Result<PreviewSearch, String>),
    /// A pair tested: passing as A and B (with the cell's text), or not.
    Tested(u64, (HashId, HashId), Option<((HashId, HashId), String)>),
}

#[derive(Default)]
struct State {
    rule: Option<Rule>,
    limit: Option<usize>,
    fetched: Vec<(HashId, HashId)>,
    to_test: usize,
    passed: Vec<((HashId, HashId), String)>,
    failed: Vec<(HashId, HashId)>,
    search_label: String,
    testing: bool,
    thumbs: HashMap<HashId, slint::Image>,
}

/// The preview of a rule editor.
pub(crate) struct Preview {
    store: Arc<Store>,
    state: RefCell<State>,
    /// The run now wanted; a worker's word for an older one is dropped.
    run: Arc<AtomicU64>,
    sender: Sender<Said>,
    receiver: Receiver<Said>,
    timer: slint::Timer,
    /// The duplicate filter opened from a list, while it is open.
    filter: Rc<RefCell<Option<crate::DuplicateFilterWindow>>>,
}

fn now() -> hydrus_search::Clock {
    hydrus_search::Clock::system()
}

impl Preview {
    pub(crate) fn new(
        store: &Arc<Store>,
        filter: Rc<RefCell<Option<crate::DuplicateFilterWindow>>>,
    ) -> Rc<Self> {
        let (sender, receiver) = crossbeam_channel::unbounded();
        Rc::new(Self {
            store: store.clone(),
            state: RefCell::new(State {
                limit: Some(crate::auto_resolution_review::DEFAULT_FETCH),
                search_label: READY_TO_FETCH.into(),
                ..State::default()
            }),
            run: Arc::new(AtomicU64::new(0)),
            sender,
            receiver,
            timer: slint::Timer::default(),
            filter,
        })
    }

    /// The tab shown with the rule as edited (or why there is none): fetch
    /// anew if its search changed, test anew if only its tests or action
    /// did.
    pub(crate) fn shown(
        self: &Rc<Self>,
        window: &AutoResolutionRuleWindow,
        rule: Result<Rule, String>,
    ) {
        let rule = match rule {
            Ok(rule) => rule,
            Err(e) => {
                let mut state = self.state.borrow_mut();
                state.rule = None;
                state.search_label = problem(&e);
                drop(state);
                self.show(window);
                return;
            }
        };
        let old = self.state.borrow_mut().rule.replace(rule.clone());
        match old {
            Some(old) if old.search == rule.search => {
                if old.comparators != rule.comparators
                    || old.action != rule.action
                    || old.delete_a != rule.delete_a
                    || old.delete_b != rule.delete_b
                    || old.custom_merge != rule.custom_merge
                {
                    self.retest(window);
                } else {
                    self.show(window);
                }
            }
            _ => self.refetch(window),
        }
        self.watch(window);
    }

    /// "only sample this many" changed.
    pub(crate) fn set_limit(
        self: &Rc<Self>,
        window: &AutoResolutionRuleWindow,
        limit: Option<usize>,
    ) {
        self.state.borrow_mut().limit = limit;
        self.refetch(window);
    }

    /// Fetch a sample of pairs anew, then test them.
    pub(crate) fn refetch(self: &Rc<Self>, window: &AutoResolutionRuleWindow) {
        let Some(rule) = self.state.borrow().rule.clone() else {
            return;
        };
        let run = self.run.fetch_add(1, Ordering::SeqCst) + 1;
        {
            let mut state = self.state.borrow_mut();
            state.fetched.clear();
            state.passed.clear();
            state.failed.clear();
            state.to_test = 0;
            state.testing = true;
            state.search_label = FETCHING.into();
        }
        let limit = self.state.borrow().limit;
        let (store, sender, current) = (self.store.clone(), self.sender.clone(), self.run.clone());
        std::thread::spawn(move || {
            let found =
                engine::preview_search(&store, &rule, limit, &now()).map_err(|e| e.to_string());
            let pairs = found
                .as_ref()
                .map(|f| f.matched.clone())
                .unwrap_or_default();
            if sender.send(Said::Searched(run, found)).is_err() {
                return;
            }
            test_all(&store, &rule, &pairs, run, &current, &sender);
        });
        self.show(window);
        self.watch(window);
    }

    /// Test the fetched pairs anew.
    pub(crate) fn retest(self: &Rc<Self>, window: &AutoResolutionRuleWindow) {
        let Some(rule) = self.state.borrow().rule.clone() else {
            return;
        };
        let run = self.run.fetch_add(1, Ordering::SeqCst) + 1;
        let pairs = {
            let mut state = self.state.borrow_mut();
            state.passed.clear();
            state.failed.clear();
            state.to_test = state.fetched.len();
            state.testing = true;
            state.fetched.clone()
        };
        let (store, sender, current) = (self.store.clone(), self.sender.clone(), self.run.clone());
        std::thread::spawn(move || test_all(&store, &rule, &pairs, run, &current, &sender));
        self.show(window);
        self.watch(window);
    }

    /// A list's pair double-clicked: the duplicate filter on the list's
    /// pairs from it, round to the start (`GetMediaResultPairsStartingAtIndex`),
    /// as given (`PotentialDuplicatePairFactoryMediaResults`).
    pub(crate) fn activated(&self, passing: bool, row: usize) -> Option<String> {
        let (rule, pairs) = {
            let state = self.state.borrow();
            let pairs: Vec<(HashId, HashId)> = if passing {
                state.passed.iter().map(|(pair, _)| *pair).collect()
            } else {
                state.failed.clone()
            };
            (state.rule.clone()?, pairs)
        };
        if row >= pairs.len() {
            return None;
        }
        let pairs: Vec<(HashId, HashId)> =
            pairs[row..].iter().chain(&pairs[..row]).copied().collect();
        let query = hydrus_duplicates::potentials::PotentialsQuery::from_search(
            &self.store.snapshot(),
            &rule.search,
        );
        let model = query.map_err(anyhow::Error::from).and_then(|query| {
            crate::duplicate_filter::DuplicateFilter::for_pairs(self.store.clone(), query, pairs)
        });
        let mut model = match model {
            Ok(model) => model,
            Err(e) => return Some(e.to_string()),
        };
        let step = model.load_batch();
        if matches!(step, Ok(crate::duplicate_filter::Step::Finished)) {
            return Some(crate::auto_resolution_review::NOTHING_LOCAL_IN_FILTER.to_owned());
        }
        match crate::filter_window::open_filter(model, step, &self.filter) {
            Ok(window) => {
                *self.filter.borrow_mut() = Some(window);
                None
            }
            Err(e) => Some(e.to_string()),
        }
    }

    /// Take in what the worker said, every so often, while it works.
    fn watch(self: &Rc<Self>, window: &AutoResolutionRuleWindow) {
        if self.timer.running() {
            return;
        }
        let me = Rc::downgrade(self);
        let weak = window.as_weak();
        self.timer.start(
            slint::TimerMode::Repeated,
            Duration::from_millis(50),
            move || {
                if let (Some(me), Some(window)) = (me.upgrade(), weak.upgrade()) {
                    me.drain(&window);
                }
            },
        );
    }

    /// What the worker said so far, taken in.
    pub(crate) fn drain(&self, window: &AutoResolutionRuleWindow) {
        let current = self.run.load(Ordering::SeqCst);
        let mut changed = false;
        while let Ok(said) = self.receiver.try_recv() {
            let mut state = self.state.borrow_mut();
            match said {
                Said::Searched(run, found) if run == current => {
                    match found {
                        Ok(found) => {
                            state.search_label = searched(found.searched, found.matched.len());
                            state.to_test = found.matched.len();
                            state.fetched = found.matched;
                        }
                        Err(e) => state.search_label = problem(&e),
                    }
                    state.testing = state.to_test > 0;
                }
                Said::Tested(run, pair, passed) if run == current => {
                    state.to_test = state.to_test.saturating_sub(1);
                    match passed {
                        Some(pass) => state.passed.push(pass),
                        None => state.failed.push(pair),
                    }
                    state.testing = state.to_test > 0;
                }
                _ => continue,
            }
            changed = true;
        }
        if changed {
            self.show(window);
        }
    }

    fn thumb(&self, id: HashId) -> slint::Image {
        if let Some(image) = self.state.borrow().thumbs.get(&id) {
            return image.clone();
        }
        let image = crate::thumbnails::thumbnail(&self.store, id)
            .map(|raster| crate::thumbnails::Pixels::new(&raster).image())
            .unwrap_or_default();
        self.state.borrow_mut().thumbs.insert(id, image.clone());
        image
    }

    fn show(&self, window: &AutoResolutionRuleWindow) {
        let (search_label, to_test, testing, passed, failed, fetched_none) = {
            let state = self.state.borrow();
            (
                state.search_label.clone(),
                state.to_test,
                state.testing,
                state.passed.clone(),
                state.failed.clone(),
                state.rule.is_none() || (state.fetched.is_empty() && !state.testing),
            )
        };
        window.set_preview_search_label(search_label.into());
        window.set_preview_to_test_label(
            if to_test == 0 && fetched_none {
                READY_TO_TEST.to_owned()
            } else {
                still_to_test(to_test)
            }
            .into(),
        );
        let label = |n: usize| {
            if testing && n == 0 {
                TESTING.to_owned()
            } else if fetched_none && n == 0 {
                READY_TO_PREVIEW.to_owned()
            } else {
                listed(n)
            }
        };
        window.set_preview_pass_label(label(passed.len()).into());
        window.set_preview_fail_label(label(failed.len()).into());
        let pass: Vec<PairRow> = passed
            .iter()
            .map(|((a, b), text)| PairRow {
                a: self.thumb(*a),
                b: self.thumb(*b),
                text: text.as_str().into(),
                selected: false,
            })
            .collect();
        window.set_preview_pass_rows(ModelRc::new(VecModel::from(pass)));
        let fail: Vec<PairRow> = failed
            .iter()
            .map(|(a, b)| PairRow {
                a: self.thumb(*a),
                b: self.thumb(*b),
                text: "".into(),
                selected: false,
            })
            .collect();
        window.set_preview_fail_rows(ModelRc::new(VecModel::from(fail)));
    }
}

/// Test `pairs` one by one for run `run`, saying each, until the run is
/// no longer wanted.
fn test_all(
    store: &Store,
    rule: &Rule,
    pairs: &[(HashId, HashId)],
    run: u64,
    current: &AtomicU64,
    sender: &Sender<Said>,
) {
    for &(first, second) in pairs {
        if current.load(Ordering::SeqCst) != run {
            return;
        }
        let tested = match engine::preview_test(store, rule, first, second, &now()) {
            Ok(Some(((a, b), both))) => {
                let merge = crate::auto_resolution_review::pending_summary(store, rule, a, b).ok();
                Some(((a, b), pass_cell(rule, both, merge.as_deref())))
            }
            Ok(None) => None,
            Err(e) => {
                eprintln!("could not test the pair: {e}");
                None
            }
        };
        if sender
            .send(Said::Tested(run, (first, second), tested))
            .is_err()
        {
            return;
        }
    }
}

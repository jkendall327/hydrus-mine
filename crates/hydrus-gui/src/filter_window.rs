//! The duplicate filter's window, bound to its model: the file shown, its
//! comparison with the other, the decisions, and the questions the filter
//! asks (commit the batch? another group? commit before closing?).

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;
use std::sync::Arc;
use std::time::Duration;

use crossbeam_channel::{Receiver, Sender};
use slint::{ComponentHandle, ModelRc, SharedString, VecModel};

use hydrus_core::HashId;
use hydrus_duplicates::content::StoreContent;
use hydrus_duplicates::statements::{self, FAST_KEYS, SLOW_KEYS, Statement};
use hydrus_media::Raster;
use hydrus_search::media::FileFacts;
use hydrus_store::Store;
use hydrus_store::duplicates::merge::MergeOptions;
use hydrus_store::duplicates::{ComparisonScores, PairRelationship};

use crate::duplicate_filter::{Decision, DuplicateFilter, Step};
use crate::playback::Playback;
use crate::thumbnails::Pixels;
use crate::ui::{DuplicateFilterWindow, Statement as StatementRow};

/// What the question shown asks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Asking {
    Nothing,
    /// The batch is done: commit and continue, or go back.
    Commit,
    /// The group was skipped: another, or the same again.
    Group,
    /// Closing with decisions pending: commit, forget, or carry on.
    Close,
    /// Nothing more to do: close.
    Done,
    /// A custom action's decision.
    CustomType,
    /// A custom action's deletions.
    CustomDelete,
}

/// A custom action's decisions, as its question lists them (sorted, as the
/// reference's select dialog sorts them).
const CUSTOM_TYPES: [(&str, PairRelationship); 4] = [
    ("alternates", PairRelationship::Alternate),
    (
        "not related/false positive",
        PairRelationship::FalsePositive,
    ),
    ("same quality", PairRelationship::SameQuality),
    ("this is a better duplicate", PairRelationship::Better),
];

/// A custom action's deletion question's answers, then "forget it".
const CUSTOM_DELETES: [(&str, bool, bool); 4] = [
    ("delete neither", false, false),
    ("delete this one", true, false),
    ("delete the other", false, true),
    ("delete both", true, true),
];

/// A custom action under way: the pair it is for, its decision and its own
/// merge options, if edited.
type Custom = ((HashId, HashId), PairRelationship, Option<MergeOptions>);

struct SlowRequest {
    pair: (HashId, HashId),
    facts: (FileFacts, FileFacts),
    pixel_duplicates: bool,
    scores: ComparisonScores,
}

/// The slow statements (jpeg quality, visual duplicates), made on a thread
/// of their own since they read and analyse the files.
struct SlowStatements {
    requests: Sender<SlowRequest>,
    results: Receiver<((HashId, HashId), Vec<Statement>)>,
}

impl SlowStatements {
    fn new(store: &Arc<Store>) -> Self {
        let (requests, jobs) = crossbeam_channel::unbounded::<SlowRequest>();
        let (done, results) = crossbeam_channel::unbounded();
        let store = Arc::clone(store);
        std::thread::Builder::new()
            .name("comparison".into())
            .spawn(move || {
                let mut content = StoreContent::new(&store);
                for job in &jobs {
                    // (only the latest pair asked for is worth the work)
                    let job = std::iter::once(job).chain(jobs.try_iter()).last();
                    let Some(job) = job else { continue };
                    let made = statements::slow(
                        (job.pair.0, &job.facts.0),
                        (job.pair.1, &job.facts.1),
                        job.pixel_duplicates,
                        &job.scores,
                        &mut content,
                    );
                    if done.send((job.pair, made)).is_err() {
                        break;
                    }
                }
            })
            .expect("starting the comparison thread");
        Self { requests, results }
    }
}

/// Files decoded ahead of showing them, on a thread of their own.
struct Stills {
    requests: Sender<HashId>,
    results: Receiver<(HashId, Option<(Pixels, Raster)>)>,
}

/// A file decoded: as shown, and whole.
type Decoded = (slint::Image, Option<Arc<Raster>>);

fn decoded(raster: Option<Raster>) -> Decoded {
    (
        raster.as_ref().map(crate::image).unwrap_or_default(),
        raster.map(Arc::new),
    )
}

impl Stills {
    fn new(store: &Arc<Store>) -> Self {
        let (requests, jobs) = crossbeam_channel::unbounded::<HashId>();
        let (done, results) = crossbeam_channel::unbounded();
        let store = Arc::clone(store);
        std::thread::Builder::new()
            .name("filter stills".into())
            .spawn(move || {
                for id in jobs {
                    let decoded = crate::viewer::still(&store, id).map(|r| (Pixels::new(&r), r));
                    if done.send((id, decoded)).is_err() {
                        break;
                    }
                }
            })
            .expect("starting the stills thread");
        Self { requests, results }
    }
}

/// How many pairs past the one shown are decoded ahead (the reference's
/// default `duplicate_filter_prefetch_num_pairs`).
const PREFETCH_PAIRS: usize = 3;

struct State {
    viewing_stats: crate::viewing_tracking::CanvasTracker,
    model: DuplicateFilter,
    asking: Asking,
    /// The pair shown, as (the file shown, the other), and its slow
    /// statements once made.
    shown: Option<(HashId, HashId)>,
    statements: Vec<Statement>,
    slow_done: bool,
    /// Files decoded, the pair shown's and those coming up: as shown, and
    /// whole, to draw sharply.
    images: HashMap<HashId, Decoded>,
    /// Files asked of the stills thread and not yet back.
    requested: HashSet<HashId>,
    /// Video, audio and animations play, as in the media viewer.
    playback: Rc<Playback>,
    animator: Rc<crate::animation::Animator>,
    /// The file shown's zoom and position.
    zoomed: crate::zoom::Zoomed,
    /// A custom action under way.
    custom: Option<Custom>,
    /// Its merge options' editor, while open.
    merge_options: crate::merge_options_window::Slot,
}

impl State {
    /// Ask for the files coming up to be decoded, and forget the ones
    /// passed.
    fn prefetch(&mut self, stills: &Stills) {
        let upcoming: HashSet<HashId> = self.model.upcoming(PREFETCH_PAIRS).into_iter().collect();
        self.images.retain(|id, _| upcoming.contains(id));
        for id in upcoming {
            if !self.images.contains_key(&id) && self.requested.insert(id) {
                let _ = stills.requests.send(id);
            }
        }
    }
}

fn now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs() as i64)
}

fn key_order(key: &str) -> usize {
    FAST_KEYS
        .iter()
        .chain(SLOW_KEYS.iter())
        .position(|k| *k == key)
        .unwrap_or(usize::MAX)
}

fn refresh_colours(window: &DuplicateFilterWindow, state: &State) {
    if !state.viewing_stats.active() {
        return;
    }
    let store = state.model.store();
    let settings: hydrus_store::settings::DuplicateColourSettings =
        store.read(hydrus_store::settings::get).unwrap_or_default();
    let native: hydrus_store::settings::ViewerCanvasSettings =
        store.read(hydrus_store::settings::get).unwrap_or_default();
    let pair = state.model.current();
    let transparent = pair.is_some_and(|(shown, _)| {
        state
            .model
            .facts(shown)
            .is_some_and(|facts| facts.has_transparency)
    });
    let colour = hydrus_gui_model::duplicate_colours::background(
        &settings,
        pair.is_some(),
        state.model.showing_file_a(),
    );
    window.set_canvas_background(
        slint::Color::from_rgb_u8(colour.0[0], colour.0[1], colour.0[2]).into(),
    );
    window.set_transparency_mode(hydrus_gui_model::duplicate_colours::transparency(
        &settings,
        transparent,
        native.transparency_greenscreen,
    ));
}

/// Show the state in the window.
fn show(window: &DuplicateFilterWindow, state: &mut State) {
    if !state.viewing_stats.active() {
        return;
    }
    refresh_colours(window, state);
    state
        .viewing_stats
        .show(state.model.current().map(|(shown, _)| shown));
    let Some((shown, other)) = state.model.current() else {
        state.shown = None;
        state.playback.stop();
        state.animator.stop();
        window.set_media(slint::Image::default());
        window.set_index_text("-".into());
        window.set_statements(ModelRc::default());
        window.set_score_text(SharedString::new());
        return;
    };
    let newly_shown = state.shown != Some((shown, other));
    let (image, raster) = state
        .images
        .entry(shown)
        .or_insert_with(|| decoded(crate::viewer::still(state.model.store(), shown)))
        .clone();
    window.set_media(image);
    if newly_shown {
        let store = state.model.store();
        let path = crate::viewer::playable(store, shown);
        let animation = crate::viewer::animation(store, shown);
        // going between a pair's files keeps the zoom and position; a new
        // pair starts at the first's default zoom, centred
        let shape = crate::viewer::shape(store, shown);
        let still = path.is_none() && animation.is_none();
        state
            .zoomed
            .set_still(crate::viewer::still_of(raster, shape, still));
        if state.shown == Some((other, shown)) {
            state.zoomed.switch_to(shape);
        } else {
            state.zoomed.show(shape);
        }
        state.shown = Some((shown, other));
        state.slow_done = false;
        state.statements.clear();
        let (size, frame) = (window.as_weak(), window.as_weak());
        let zoomed = state.zoomed.clone();
        state.playback.play(
            path.as_deref(),
            move || {
                // (rendered at the size shown)
                zoomed.render_size().or_else(|| {
                    let size = size.upgrade()?.window().size();
                    Some((size.width, size.height))
                })
            },
            move |image| {
                if let Some(window) = frame.upgrade() {
                    window.set_media(image);
                }
            },
        );
        let frame = window.as_weak();
        state.animator.play(animation, move |image| {
            if let Some(window) = frame.upgrade() {
                window.set_media(image);
            }
        });
    }
    window.set_index_text(state.model.index_text().into());
    // the file shown against the other: the fast statements, then the slow
    // ones once made
    let mut lines: Vec<Statement> = state
        .model
        .comparison(now())
        .map(|c| c.statements)
        .unwrap_or_default();
    lines.extend(state.statements.iter().cloned());
    lines.sort_by_key(|s| key_order(s.key));
    let score: i32 = lines.iter().map(|s| s.score).sum();
    let rows: Vec<StatementRow> = lines
        .iter()
        .map(|s| StatementRow {
            text: s.text.as_str().into(),
            score: s.score,
        })
        .collect();
    window.set_statements(ModelRc::new(VecModel::from(rows)));
    let mut text = match score {
        s if s > 0 => format!(
            "score: +{}",
            hydrus_core::numbers::human_int(s.unsigned_abs().into())
        ),
        s if s < 0 => format!(
            "score: -{}",
            hydrus_core::numbers::human_int(s.unsigned_abs().into())
        ),
        _ => "no score difference".to_owned(),
    };
    if !state.slow_done {
        text.push('\u{2026}');
    }
    window.set_score_text(text.into());
    window.set_score(score);
}

fn ask(
    window: &DuplicateFilterWindow,
    state: &mut State,
    asking: Asking,
    question: &str,
    answers: &[&str],
) {
    state.asking = asking;
    window.set_question(question.into());
    let answers: Vec<SharedString> = answers.iter().map(|&a| a.into()).collect();
    window.set_answers(ModelRc::new(VecModel::from(answers)));
}

/// Show where a step left the filter.
fn after(window: &DuplicateFilterWindow, state: &mut State, step: anyhow::Result<Step>) {
    match step {
        Ok(Step::Showing) => {
            state.asking = Asking::Nothing;
            window.set_question(SharedString::new());
        }
        Ok(Step::Confirm { question }) => {
            ask(
                window,
                state,
                Asking::Commit,
                &question,
                &["commit and continue", "go back"],
            );
        }
        Ok(Step::SkippedGroup) => ask(
            window,
            state,
            Asking::Group,
            "You appear to have skipped this whole group. Do you want to load up a different one?",
            &["yes", "no"],
        ),
        Ok(Step::Finished) => ask(
            window,
            state,
            Asking::Done,
            "All pairs have been filtered!",
            &["ok"],
        ),
        Ok(Step::Undisplayable) => ask(
            window,
            state,
            Asking::Done,
            "It seems an entire batch of pairs were unable to be displayed. The duplicate filter will now close.",
            &["ok"],
        ),
        Err(e) => ask(
            window,
            state,
            Asking::Done,
            &format!("The duplicate filter failed: {e}"),
            &["ok"],
        ),
    }
    show(window, state);
}

/// Open the filter's window on `model`, whose first batch is loaded with
/// `step`; `slot` holds the window while it is open.
pub(crate) fn open_filter(
    model: DuplicateFilter,
    step: anyhow::Result<Step>,
    slot: &Rc<RefCell<Option<DuplicateFilterWindow>>>,
    exited_after_work: Option<Rc<dyn Fn()>>,
) -> Result<DuplicateFilterWindow, slint::PlatformError> {
    let window = DuplicateFilterWindow::new()?;
    window.set_reviewing(model.reviewing());
    let playback_store = model.store().clone();
    let slow = Rc::new(SlowStatements::new(model.store()));
    let stills = Rc::new(Stills::new(model.store()));
    let settings: hydrus_core::media_viewer::MediaViewerSettings = model
        .store()
        .read(hydrus_store::settings::get)
        .unwrap_or_default();
    let zoomed = crate::zoom_window!(window, settings, |window: &DuplicateFilterWindow| {
        (
            window.get_canvas_width() as i32,
            window.get_canvas_height() as i32,
        )
    });
    crate::bind_zoom!(window, zoomed);
    let state = Rc::new(RefCell::new(State {
        viewing_stats: crate::viewing_tracking::CanvasTracker::new(
            model.store().clone(),
            hydrus_core::CanvasType::DuplicatesFilter,
        ),
        model,
        asking: Asking::Nothing,
        shown: None,
        statements: Vec::new(),
        slow_done: false,
        images: HashMap::new(),
        requested: HashSet::new(),
        playback: Playback::for_store(playback_store.clone()),
        animator: crate::animation::Animator::for_store(playback_store),
        zoomed,
        custom: None,
        merge_options: Rc::default(),
    }));

    // ask for the slow statements of the pair shown, if not yet asked
    let request_slow = {
        let slow = slow.clone();
        move |state: &State| {
            let Some(pair) = state.shown else { return };
            let model = &state.model;
            let (Some(a), Some(b), Some(comparison)) = (
                model.facts(pair.0),
                model.facts(pair.1),
                model.comparison(0),
            ) else {
                return;
            };
            let _ = slow.requests.send(SlowRequest {
                pair,
                facts: (a.clone(), b.clone()),
                pixel_duplicates: comparison.pixel_duplicates,
                scores: *model.scores(),
            });
        }
    };
    let update = {
        let weak = window.as_weak();
        let state = state.clone();
        let request_slow = request_slow.clone();
        let stills = stills.clone();
        move |change: &dyn Fn(&mut State) -> Option<anyhow::Result<Step>>| {
            let Some(window) = weak.upgrade() else { return };
            let mut state = state.borrow_mut();
            if !state.viewing_stats.active() {
                return;
            }
            let before = state.shown;
            match change(&mut state) {
                Some(step) => after(&window, &mut state, step),
                None => show(&window, &mut state),
            }
            if state.shown != before {
                request_slow(&state);
            }
            state.prefetch(&stills);
        }
    };

    {
        let mut s = state.borrow_mut();
        after(&window, &mut s, step);
        request_slow(&s);
        s.prefetch(&stills);
    }

    let collect = Rc::new(slint::Timer::default());
    collect.start(slint::TimerMode::Repeated, Duration::from_millis(30), {
        let weak = window.as_weak();
        let state = state.clone();
        let slow = slow.clone();
        let stills = stills.clone();
        move || {
            let Some(window) = weak.upgrade() else { return };
            refresh_colours(&window, &state.borrow());
            while let Ok((id, decoded)) = stills.results.try_recv() {
                let mut state = state.borrow_mut();
                state.requested.remove(&id);
                let entry = match decoded {
                    Some((pixels, raster)) => (pixels.image(), Some(Arc::new(raster))),
                    None => (slint::Image::default(), None),
                };
                state.images.insert(id, entry);
            }
            while let Ok((pair, made)) = slow.results.try_recv() {
                let mut state = state.borrow_mut();
                if state.shown == Some(pair) {
                    state.statements = made;
                    state.slow_done = true;
                    show(&window, &mut state);
                }
            }
        }
    });

    window.on_decide({
        let update = update.clone();
        let weak = window.as_weak();
        let state = state.clone();
        move |action| {
            if !state.borrow().viewing_stats.active() {
                return;
            }
            // "custom action": its decision asked first
            if action == "custom" {
                let Some(window) = weak.upgrade() else { return };
                let mut state = state.borrow_mut();
                if state.asking == Asking::Nothing && state.model.current().is_some() {
                    let mut answers: Vec<&str> = CUSTOM_TYPES.iter().map(|t| t.0).collect();
                    answers.push("cancel");
                    ask(
                        &window,
                        &mut state,
                        Asking::CustomType,
                        "select duplicate type",
                        &answers,
                    );
                }
                return;
            }
            update(&|state| {
                if state.asking != Asking::Nothing {
                    return None;
                }
                let decision = match action.as_str() {
                    "better-delete" => Decision::BETTER_DELETE_OTHER,
                    "better-keep" => Decision::BETTER_KEEP_BOTH,
                    "same" => Decision::SAME_QUALITY,
                    "alternates" => Decision::ALTERNATES,
                    "false-positive" => Decision::FALSE_POSITIVE,
                    "skip" => Decision::Skip,
                    "approve" => Decision::Review { approved: true },
                    "deny" => Decision::Review { approved: false },
                    "back" => {
                        state.model.back();
                        return None;
                    }
                    _ => return None,
                };
                Some(state.model.decide(decision))
            });
        }
    });
    window.on_toggle_pause({
        let state = state.clone();
        move || {
            let state = state.borrow();
            state.playback.toggle_pause();
            state.animator.toggle_pause();
        }
    });
    window.on_switch_media({
        let update = update.clone();
        move || {
            update(&|state| {
                state.model.switch();
                None
            });
        }
    });
    let close = {
        let weak = window.as_weak();
        let slot = slot.clone();
        let collect = collect.clone();
        let state = state.clone();
        move || {
            let Some(window) = weak.upgrade() else { return };
            state.borrow().viewing_stats.close();
            collect.stop();
            {
                let state = state.borrow();
                state.playback.close();
                state.animator.stop();
                state.zoomed.close();
            }
            if let Some(editor) = state.borrow().merge_options.borrow_mut().take() {
                let _ = editor.hide();
            }
            let _ = window.hide();
            if !slot
                .borrow()
                .as_ref()
                .is_some_and(|current| std::ptr::eq(current.window(), window.window()))
            {
                return;
            }
            slot.borrow_mut().take();
            let done_work = state.borrow().model.done_work();
            if done_work && let Some(exited) = &exited_after_work {
                exited();
            }
        }
    };
    // a custom action's deletions asked, if its pair is still shown
    let ask_delete: Rc<dyn Fn()> = {
        let weak = window.as_weak();
        let state = state.clone();
        Rc::new(move || {
            let Some(window) = weak.upgrade() else { return };
            let mut state = state.borrow_mut();
            if !state.viewing_stats.active() {
                return;
            }
            let pair = state.custom.as_ref().map(|c| c.0);
            if pair.is_none() || pair != state.model.current() || state.asking != Asking::Nothing {
                state.custom = None;
                return;
            }
            let mut answers: Vec<&str> = CUSTOM_DELETES.iter().map(|d| d.0).collect();
            answers.push("forget it");
            ask(
                &window,
                &mut state,
                Asking::CustomDelete,
                "Delete any of the files?",
                &answers,
            );
        })
    };
    window.on_answer({
        let update = update.clone();
        let state = state.clone();
        let close = close.clone();
        move |answer| {
            if !state.borrow().viewing_stats.active() {
                return;
            }
            let asking = state.borrow().asking;
            match (asking, answer) {
                (Asking::CustomType, i) => {
                    let chosen = usize::try_from(i).ok().and_then(|i| CUSTOM_TYPES.get(i));
                    update(&|_| Some(Ok(Step::Showing)));
                    let Some(&(_, relationship)) = chosen else {
                        return;
                    };
                    let (store, pair) = {
                        let state = state.borrow();
                        (state.model.store().clone(), state.model.current())
                    };
                    let Some(pair) = pair else { return };
                    let advanced = store
                        .read(hydrus_store::settings::get::<hydrus_store::settings::AdvancedMode>)
                        .unwrap_or_default()
                        .0;
                    let merges = matches!(
                        relationship,
                        PairRelationship::Better | PairRelationship::SameQuality
                    ) || (advanced && relationship == PairRelationship::Alternate);
                    state.borrow_mut().custom = Some((pair, relationship, None));
                    if !merges {
                        ask_delete();
                        return;
                    }
                    // its merge options, for this decision alone
                    let client: hydrus_store::duplicates::DuplicateMergeSettings =
                        store.read(hydrus_store::settings::get).unwrap_or_default();
                    let options = client
                        .for_relationship(relationship)
                        .cloned()
                        .unwrap_or_default();
                    let applied: Rc<dyn Fn(MergeOptions)> = {
                        let state = state.clone();
                        let ask_delete = ask_delete.clone();
                        Rc::new(move |options| {
                            if let Some(custom) = &mut state.borrow_mut().custom {
                                custom.2 = Some(options);
                            }
                            ask_delete();
                        })
                    };
                    let slot = state.borrow().merge_options.clone();
                    match crate::merge_options_window::open(
                        &store,
                        relationship,
                        &options,
                        true,
                        &slot,
                        applied,
                    ) {
                        Ok(editor) => *slot.borrow_mut() = Some(editor),
                        Err(e) => eprintln!("could not open the merge options: {e}"),
                    }
                }
                (Asking::CustomDelete, i) => {
                    let delete = usize::try_from(i).ok().and_then(|i| CUSTOM_DELETES.get(i));
                    let custom = state.borrow_mut().custom.take();
                    match (delete, custom) {
                        (Some(&(_, delete_a, delete_b)), Some((_, relationship, merge))) => {
                            update(&|state| {
                                Some(state.model.decide_custom(
                                    relationship,
                                    delete_a,
                                    delete_b,
                                    merge.clone(),
                                ))
                            });
                        }
                        _ => update(&|_| Some(Ok(Step::Showing))),
                    }
                }
                (Asking::Commit, 0) => update(&|state| Some(state.model.commit())),
                (Asking::Commit, _) => update(&|state| {
                    state.model.back();
                    Some(Ok(Step::Showing))
                }),
                (Asking::Group, 0) => update(&|state| Some(state.model.new_group())),
                (Asking::Group, _) => update(&|state| Some(state.model.load_batch())),
                (Asking::Close, 0) => {
                    let result = state.borrow_mut().model.commit_pending();
                    match result {
                        Ok(()) => close(),
                        Err(e) => update(&|_| Some(Err(anyhow::anyhow!("{e}")))),
                    }
                }
                (Asking::Close, 1) | (Asking::Done, _) => close(),
                (Asking::Close, _) => update(&|_| Some(Ok(Step::Showing))),
                (Asking::Nothing, _) => {}
            }
        }
    });
    window.on_close_requested({
        let state = state.clone();
        let weak = window.as_weak();
        let close = close.clone();
        move || {
            if !state.borrow().viewing_stats.active() {
                return;
            }
            let pending = state.borrow().model.pending();
            if pending == 0 {
                close();
                return;
            }
            if let Some(window) = weak.upgrade() {
                let question = format!(
                    "commit {} decisions?",
                    hydrus_core::numbers::human_int(pending as u64)
                );
                ask(
                    &window,
                    &mut state.borrow_mut(),
                    Asking::Close,
                    &question,
                    &["commit", "forget", "back to filtering"],
                );
            }
        }
    });
    window.window().on_close_requested({
        let window = window.as_weak();
        move || {
            if let Some(window) = window.upgrade() {
                window.invoke_close_requested();
            }
            slint::CloseRequestResponse::KeepWindowShown
        }
    });
    window.show()?;
    Ok(window)
}

#[cfg(test)]
mod colour_tests {
    use super::*;
    use hydrus_core::{Sha256, service::builtin_keys};
    use hydrus_duplicates::potentials::PotentialsQuery;
    use hydrus_search::{FileSearchContext, LocationContext};
    use hydrus_store::{
        duplicates::{FileScope, PairSearchKind, PixelDuplicates},
        settings::{self, DuplicateColourSettings, ViewerCanvasSettings},
    };
    use serde_json::json;

    #[test]
    fn live_pair_switch_preferences_and_painter_replay_qt_then_retire() {
        let fixture = hydrus_testkit::fixture_json("duplicate_colours.json");
        let manifest = hydrus_testkit::fixture_json("legacy_db/basic.manifest.json");
        let legacy = hydrus_testkit::legacy_fixture("basic");
        let native = tempfile::tempdir().unwrap();
        hydrus_store::import::import_legacy(
            legacy.path(),
            &native.path().join(hydrus_store::store::DB_FILE_NAME),
        )
        .unwrap();
        let store = Store::open(native.path()).unwrap();
        let id = |name: &str| {
            let file = manifest["files"]
                .as_array()
                .unwrap()
                .iter()
                .find(|file| file["name"] == name)
                .unwrap();
            let hash: Sha256 = file["hash"].as_str().unwrap().parse().unwrap();
            store
                .read(|conn| hydrus_store::master::hash_id(conn, &hash))
                .unwrap()
                .unwrap()
        };
        let a = id("png_alpha_00.png");
        let b = id("jpeg_00.jpg");
        let snapshot = store.snapshot();
        let service = snapshot.services.builtin(builtin_keys::MY_FILES).unwrap();
        let search = FileSearchContext {
            location: LocationContext::single(service.key.clone()),
            ..FileSearchContext::default()
        };
        let query = PotentialsQuery {
            scope: FileScope::Domains {
                current: vec![service.id],
                deleted: vec![],
            },
            kind: PairSearchKind::OneFileMatchesOneSearch,
            pixel_duplicates: PixelDuplicates::Allowed,
            max_hamming_distance: 4,
            search_1: search.clone(),
            search_2: search,
        };
        let mut model =
            DuplicateFilter::for_pairs(store.clone(), query.clone(), vec![(a, b)]).unwrap();
        let step = model.load_batch();
        assert_eq!(model.current(), Some((a, b)));
        assert!(model.facts(a).unwrap().has_transparency);
        assert!(!model.facts(b).unwrap().has_transparency);
        let windows = crate::headless::init();
        let slot = Rc::new(RefCell::new(None));
        let window = open_filter(model, step, &slot, None).unwrap();
        *slot.borrow_mut() = Some(window.clone_strong());
        window
            .window()
            .set_size(slint::LogicalSize::new(800.0, 600.0));
        let adapter = windows.get(0).unwrap();
        let mut showing_a = true;
        for case in fixture["canvas"].as_array().unwrap() {
            let preferences = DuplicateColourSettings {
                intensity_a: serde_json::from_value(case["a"].clone()).unwrap(),
                intensity_b: serde_json::from_value(case["b"].clone()).unwrap(),
                checkerboard: case["checker"].as_bool().unwrap(),
                ..DuplicateColourSettings::default()
            };
            let green = case["green"].as_bool().unwrap();
            store
                .write(move |tx| {
                    settings::set(tx.conn(), &preferences)?;
                    let mut native: ViewerCanvasSettings = settings::get(tx.conn())?;
                    native.transparency_greenscreen = green;
                    settings::set(tx.conn(), &native)
                })
                .unwrap();
            // Preferences reach an already open owner, without switching or reopening.
            std::thread::sleep(Duration::from_millis(35));
            slint::platform::update_timers_and_animations();
            let wanted = case["file_a"].as_bool().unwrap();
            if wanted != showing_a {
                window.invoke_switch_media();
                showing_a = wanted;
            }
            let rgb: [u8; 3] = serde_json::from_value(case["colour"].clone()).unwrap();
            assert_eq!(
                window.get_canvas_background(),
                slint::Brush::from(slint::Color::from_rgb_u8(rgb[0], rgb[1], rgb[2]))
            );
            let _ = crate::headless::render(&adapter, 800, 600);
            // Isolate the same background paint as Qt's StaticImage._DrawBackground
            // with a transparent probe; real frozen media metadata chooses the mode.
            window.set_media(slint::Image::from_rgba8(slint::SharedPixelBuffer::<
                slint::Rgba8Pixel,
            >::new(64, 64)));
            window.set_sharp_shown(false);
            window.set_media_x(100.0);
            window.set_media_y(100.0);
            window.set_media_width(64.0);
            window.set_media_height(64.0);
            let pixels = crate::headless::render_snapshot(&adapter, 800, 600);
            for (position, expected) in case["pixels"].as_object().unwrap() {
                let (x, y) = position.split_once(',').unwrap();
                let x = x.parse::<usize>().unwrap() + 100;
                let y = y.parse::<usize>().unwrap() + 100;
                let offset = (y * 800 + x) * 4;
                assert_eq!(
                    json!(pixels[offset..offset + 3]),
                    *expected,
                    "{case}, {position}"
                );
            }
            let outside = (110 * 800 + 80) * 4;
            assert_eq!(
                &pixels[outside..outside + 3],
                &rgb,
                "backdrop is clipped to the image"
            );
        }
        window.invoke_close_requested();
        assert!(slot.borrow().is_none());
        let retired = window.get_canvas_background();
        let preferences = DuplicateColourSettings {
            intensity_a: Some(9),
            intensity_b: Some(9),
            checkerboard: false,
            ..DuplicateColourSettings::default()
        };
        store
            .write(move |tx| settings::set(tx.conn(), &preferences))
            .unwrap();
        let mut model = DuplicateFilter::for_pairs(store.clone(), query, vec![(a, b)]).unwrap();
        let step = model.load_batch();
        let successor = open_filter(model, step, &slot, None).unwrap();
        *slot.borrow_mut() = Some(successor.clone_strong());
        window.invoke_switch_media();
        window.invoke_close_requested();
        std::thread::sleep(Duration::from_millis(35));
        slint::platform::update_timers_and_animations();
        assert_eq!(window.get_canvas_background(), retired);
        assert!(slot.borrow().is_some());
        assert_eq!(
            successor.get_canvas_background(),
            slint::Brush::from(slint::Color::from_rgb_u8(91, 91, 91))
        );
        successor.invoke_close_requested();
        assert!(slot.borrow().is_none());
    }
}

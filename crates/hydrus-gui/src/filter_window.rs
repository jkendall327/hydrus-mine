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
use hydrus_search::media::FileFacts;
use hydrus_store::Store;
use hydrus_store::duplicates::ComparisonScores;

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
}

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
    results: Receiver<(HashId, Option<Pixels>)>,
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
                    let pixels = crate::viewer::still(&store, id).as_ref().map(Pixels::new);
                    if done.send((id, pixels)).is_err() {
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
    model: DuplicateFilter,
    asking: Asking,
    /// The pair shown, as (the file shown, the other), and its slow
    /// statements once made.
    shown: Option<(HashId, HashId)>,
    statements: Vec<Statement>,
    slow_done: bool,
    /// Files decoded, the pair shown's and those coming up.
    images: HashMap<HashId, slint::Image>,
    /// Files asked of the stills thread and not yet back.
    requested: HashSet<HashId>,
    /// Video, audio and animations play, as in the media viewer.
    playback: Rc<Playback>,
    animator: Rc<crate::animation::Animator>,
    /// The file shown's zoom and position.
    zoomed: crate::zoom::Zoomed,
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

/// Show the state in the window.
fn show(window: &DuplicateFilterWindow, state: &mut State) {
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
    if newly_shown {
        // going between a pair's files keeps the zoom and position; a new
        // pair starts at the first's default zoom, centred
        let shape = crate::viewer::shape(state.model.store(), shown);
        if state.shown == Some((other, shown)) {
            state.zoomed.switch_to(shape);
        } else {
            state.zoomed.show(shape);
        }
        state.shown = Some((shown, other));
        state.slow_done = false;
        state.statements.clear();
    }
    let image = state
        .images
        .entry(shown)
        .or_insert_with(|| {
            crate::viewer::still(state.model.store(), shown)
                .as_ref()
                .map(crate::image)
                .unwrap_or_default()
        })
        .clone();
    window.set_media(image);
    if newly_shown {
        let path = crate::viewer::playable(state.model.store(), shown);
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
        state.animator.play(
            crate::viewer::animation(state.model.store(), shown),
            move |image| {
                if let Some(window) = frame.upgrade() {
                    window.set_media(image);
                }
            },
        );
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
) -> Result<DuplicateFilterWindow, slint::PlatformError> {
    let window = DuplicateFilterWindow::new()?;
    let model_dir = model.store().dir().to_path_buf();
    let slow = Rc::new(SlowStatements::new(model.store()));
    let stills = Rc::new(Stills::new(model.store()));
    let settings: hydrus_core::media_viewer::MediaViewerSettings = model
        .store()
        .read(hydrus_store::settings::get)
        .unwrap_or_default();
    let zoomed = crate::zoom::Zoomed::within(
        &window,
        settings,
        |window| {
            (
                window.get_canvas_width() as i32,
                window.get_canvas_height() as i32,
            )
        },
        |window, (x, y, width, height)| {
            window.set_media_x(x as f32);
            window.set_media_y(y as f32);
            window.set_media_width(width as f32);
            window.set_media_height(height as f32);
        },
    );
    crate::bind_zoom!(window, zoomed);
    let state = Rc::new(RefCell::new(State {
        model,
        asking: Asking::Nothing,
        shown: None,
        statements: Vec::new(),
        slow_done: false,
        images: HashMap::new(),
        requested: HashSet::new(),
        playback: Playback::new(model_dir.join("mpv.conf")),
        animator: crate::animation::Animator::new(),
        zoomed,
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
            while let Ok((id, pixels)) = stills.results.try_recv() {
                let mut state = state.borrow_mut();
                state.requested.remove(&id);
                let image = pixels.map(Pixels::image).unwrap_or_default();
                state.images.insert(id, image);
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
        move |action| {
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
            collect.stop();
            state.borrow().playback.close();
            state.borrow().animator.stop();
            if let Some(window) = weak.upgrade() {
                let _ = window.hide();
            }
            slot.borrow_mut().take();
        }
    };
    window.on_answer({
        let update = update.clone();
        let state = state.clone();
        let close = close.clone();
        move |answer| {
            let asking = state.borrow().asking;
            match (asking, answer) {
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

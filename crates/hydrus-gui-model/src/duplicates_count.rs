//! The pair count of a potential duplicate search
//! (`ClientGUIPotentialDuplicatesSearchContext.EditPotentialDuplicatesSearchContextPanel`
//! and `ClientPotentialDuplicatesSearchContext.PotentialDuplicatePairsFragmentarySearch`).
//!
//! The reference does not count a search at once. It fetches the *search
//! space* (every potential pair in the search's file domain), then searches
//! the space a block at a time off the UI thread, showing how far it has got
//! ("4,000/30,000 pairs searched; 1,540 match…"). When the share of pairs
//! found is known well enough it stops and estimates ("30,000 pairs; ~11,000
//! match") unless that option is off; when the hit rate is very low it
//! searches all that is left at once. A play/pause button stops and resumes
//! it, the refresh button fetches the space again, and any change to the
//! search restarts the count over the same space.
//!
//! [`Counter`] is the panel's state and the reference's decisions, step by
//! step as the reference takes them (`pre_work`, the block, `publish`);
//! [`Handle`] runs it on a worker thread over a [`Source`] and is what the
//! window polls ([`Handle::snapshot`]).

use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant};

use hydrus_core::duplicates::DuplicatesSearch;
use hydrus_core::numbers::{human_int, value_range};
use hydrus_duplicates::potentials::{PotentialsQuery, PreparedQuery};
use hydrus_store::Store;
use hydrus_store::duplicates::cache::PairRow;

/// `POTENTIAL_DUPLICATE_PAIRS_BLOCK_SIZE_GUIDELINE`: the first block's size.
pub const BLOCK_GUIDELINE: usize = 4000;

/// `POTENTIAL_PAIRS_REFRESH_TIMEOUT`: a space this old is fetched again.
pub const REFRESH_TIMEOUT: Duration = Duration::from_secs(3600);

/// How long a block should take, in seconds (the reference's `0.5`).
const IDEAL_WORK_SECONDS: f64 = 0.5;

/// The estimate is good enough at this relative error (95% of the time).
const REL_ERROR: f64 = 0.025;

/// The text of the label's tooltip once there are numbers.
pub const TOOLTIP: &str = "The number on the left is how many pairs are in the system; on the right is how many match your current search.\n\nA system:everything search in \"combined local file domains\" at a distance of 8+ should find ~100%. Do not worry if there are a couple of loose pairs you cannot find--they are probably in the trash, waiting to be deleted.";

/// The cog menu's items: label, tooltip.
pub const COG_STARTS_PAUSED: (&str, &str) = (
    "start new potential duplicate pair search panels paused",
    "If you use a lot of these and the CPU lag on reviewing each new panel is annoying, try initialising them to paused.",
);
pub const COG_STOPS_TO_ESTIMATE: (&str, &str) = (
    "optimisation: try to state an estimate of final count rather than counting everything",
    "You can choose to have this panel produce an exact count every time, or stop early and estimate.",
);
pub const COG_FILE_SEARCH_OPTIMISATION: (&str, &str) = (
    "optimisation: allow single slow search optimisation when seeing low hit-rate",
    "If the incremental search is getting a very low hit-rate (like 5 out of 750,000 pairs), it is usually faster for the database to just run the file searches and then cross-reference.\n\nIn complicated edge cases, this optimisation has very bad performance, so if you see a block or two of work and then your search halts for 30+ seconds, you can turn it off here.",
);

/// The cog's two options that change how the count goes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Options {
    pub stops_to_estimate: bool,
    pub file_search_optimisation: bool,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            stops_to_estimate: true,
            file_search_optimisation: true,
        }
    }
}

/// How often the panel told its owner something (`restartedSearch`,
/// `thisSearchHasPairs`, `thisSearchDefinitelyHasNoPairs`).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Signals {
    pub restarted: u32,
    pub has_pairs: u32,
    pub no_pairs: u32,
}

/// What the panel shows and holds at this moment.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Snapshot {
    pub label: String,
    /// The label's tooltip: its own text, or once there are numbers what
    /// they mean ([`TOOLTIP`], wrapped).
    pub tooltip: String,
    pub paused: bool,
    /// The pairs found so far (`_num_potential_duplicate_pairs`).
    pub matches: usize,
    /// The pairs of the space searched so far.
    pub searched: usize,
    pub in_space: usize,
    pub signals: Signals,
}

// --- the search (PotentialDuplicatePairsFragmentarySearch) ---

struct Search<R> {
    initialised: bool,
    fetch_started: bool,
    initialised_at: Option<Instant>,
    space: Vec<R>,
    still: VecDeque<R>,
    hits: usize,
    block_size: usize,
    guideline: usize,
}

impl<R: Clone> Search<R> {
    fn new(guideline: usize) -> Self {
        Self {
            initialised: false,
            fetch_started: false,
            initialised_at: None,
            space: Vec::new(),
            still: VecDeque::new(),
            hits: 0,
            block_size: guideline,
            guideline,
        }
    }

    fn searched(&self) -> usize {
        self.space.len().saturating_sub(self.still.len())
    }

    fn done(&self) -> bool {
        self.still.is_empty()
    }

    /// `SetSearchSpace`: take the space (shuffled in chunks of 32 for a rich
    /// estimate, or as given) and start a search over it.
    fn set_space(&mut self, mut rows: Vec<R>, shuffle: bool, now: Instant) {
        if shuffle {
            rows = randomise_by_chunks(&rows, 32);
        }
        self.still = rows.iter().cloned().collect();
        self.space = rows;
        self.hits = 0;
        self.block_size = self.guideline;
        self.initialised = true;
        self.fetch_started = false;
        self.initialised_at = Some(now);
    }

    /// `ThereIsJustABitLeftBro`: so little is left that it is just done.
    fn bit_left(&self) -> bool {
        let still = self.still.len();
        still < 1024 || (still as f64) < (self.space.len() as f64) * 0.05
    }

    /// `EstimatedNumHits`.
    fn estimated_hits(&self) -> usize {
        let searched = self.searched();
        if searched == 0 {
            return 0;
        }
        (self.hits as f64 * (self.space.len() as f64 / searched as f64)) as usize
    }

    fn is_stale(&self, now: Instant) -> bool {
        self.initialised_at
            .is_some_and(|t| now.saturating_duration_since(t) >= REFRESH_TIMEOUT)
    }

    /// `DoingFileBasedSearchIsOK`: a big space, with the option on.
    fn file_based_search_is_ok(&self, option: bool) -> bool {
        option && self.space.len() > 10_000 && !self.bit_left()
    }

    /// `GetRelativeErrorAt95Certainty`.
    fn relative_error(&self) -> f64 {
        relative_error_at_95(self.hits, self.searched(), self.space.len())
    }

    /// `ThisAppearsToHaveAHitRateLowerThan`.
    fn hit_rate_below(&self, rate: f64) -> bool {
        let (x, n, big_n) = (self.hits, self.searched(), self.space.len());
        if n > big_n || x > n || n == 0 {
            return false;
        }
        let (x, n, big_n) = (x as f64, n as f64, big_n as f64);
        let z = 1.644_853_626_9_f64;
        let z_squared = 2.705_543_453_9_f64;
        let p_hat = x / n;
        let fpc = if big_n > 1.0 && n > 0.0 {
            (big_n - n) / (big_n - 1.0)
        } else {
            1.0
        };
        let var = (p_hat * (1.0 - p_hat) / n) * fpc;
        let den = 1.0 + z_squared / n;
        let num = p_hat + z_squared / (2.0 * n) + z * (var + z_squared / (4.0 * n * n)).sqrt();
        num / den < rate
    }

    /// `NotifyWorkTimeForAutothrottle`.
    fn notify_work_time(&mut self, actual: f64, ideal: f64) {
        let minimum = self.guideline / 10;
        let maximum = self.guideline * 25;
        if actual > ideal * 1.1 {
            self.block_size = minimum.max(self.block_size / 2);
        } else if actual < ideal / 1.1 {
            self.block_size = maximum.min((self.block_size as f64 * 1.1) as usize);
        }
    }
}

/// `ClientGUIFunctions.WrapToolTip`: lines wrapped at 80 characters.
pub fn wrap_tooltip(s: &str) -> String {
    let mut wrapped = Vec::new();
    for line in s.lines() {
        if line.is_empty() {
            wrapped.push(String::new());
            continue;
        }
        let mut current: Vec<&str> = Vec::new();
        let mut chars = 0;
        for word in line.split(' ') {
            if chars + current.len() + word.chars().count() > 80 && !current.is_empty() {
                wrapped.push(current.join(" "));
                current = vec![word];
                chars = word.chars().count();
            } else {
                current.push(word);
                chars += word.chars().count();
            }
        }
        if !current.is_empty() {
            wrapped.push(current.join(" "));
        }
    }
    wrapped.join("\n")
}

/// `HydrusLists.RandomiseListByChunks`: the chunks in a random order.
fn randomise_by_chunks<R: Clone>(rows: &[R], n: usize) -> Vec<R> {
    let mut chunks: Vec<&[R]> = rows.chunks(n).collect();
    for i in (1..chunks.len()).rev() {
        let j = (rand::random::<u64>() % (i as u64 + 1)) as usize;
        chunks.swap(i, j);
    }
    chunks.concat()
}

/// How precise an estimate from `x` hits in `n` pairs searched of `big_n` is,
/// as half the 95% interval over the share found
/// (`GetRelativeErrorAt95Certainty`: the Wilson interval with a finite
/// population correction).
pub fn relative_error_at_95(x: usize, n: usize, big_n: usize) -> f64 {
    const Z: f64 = 1.959_963_984_5;
    if n > big_n || x > n {
        return 1.0;
    }
    if n == big_n {
        return 0.0;
    }
    let (xf, nf, big_nf) = (x as f64, n as f64, big_n as f64);
    let (lo, hi) = if n == 0 {
        (0.0, 1.0)
    } else {
        let p_hat = xf / nf;
        let z2 = Z * Z;
        let denom = 1.0 + z2 / nf;
        let center = p_hat + z2 / (2.0 * nf);
        let adj = Z * (p_hat * (1.0 - p_hat) / nf + z2 / (4.0 * nf * nf)).sqrt();
        (
            ((center - adj) / denom).max(0.0),
            ((center + adj) / denom).min(1.0),
        )
    };
    let fpc = if n <= 1 {
        1.0
    } else {
        ((big_nf - nf) / (big_nf - 1.0)).sqrt()
    };
    let mid = 0.5 * (lo + hi);
    let half = 0.5 * (hi - lo) * fpc;
    let (lo, hi) = ((mid - half).max(0.0), (mid + half).min(1.0));
    let p_hat = if n > 0 { xf / nf } else { 0.0 };
    let half = 0.5 * (hi - lo);
    if p_hat > 0.0 {
        half / p_hat
    } else {
        f64::INFINITY
    }
}

// --- the panel (EditPotentialDuplicatesSearchContextPanel) ---

/// What the panel does next, as `pre_work` decides it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PreWork {
    /// Nothing, until something changes.
    Idle,
    /// Fetch the search space.
    Fetch,
    /// Search the next block.
    Block,
}

/// The panel's count: the search it counts, how many pairs it has found, the
/// pause button and the label. Single threaded; [`Handle`] puts it on a
/// worker.
pub struct Counter<R> {
    guideline: usize,
    shuffle: bool,
    search: Search<R>,
    generation: u64,
    estimate_reached: bool,
    matches: usize,
    paused: bool,
    shown: bool,
    label: String,
    tooltip: String,
    /// The text last set on the label (a tooltip follows only a change).
    last_set_text: String,
    failed: Option<String>,
    signals: Signals,
}

impl<R> std::fmt::Debug for Counter<R> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Counter")
            .field("label", &self.label)
            .field("paused", &self.paused)
            .field("matches", &self.matches)
            .finish_non_exhaustive()
    }
}

impl<R: Clone> Counter<R> {
    /// A panel for a search; `paused` is the starts-paused option, `shuffle`
    /// whether the space is shuffled (the reference always does; recordings
    /// and tests search it as given).
    pub fn new(guideline: usize, paused: bool, shuffle: bool) -> Self {
        Self {
            guideline,
            shuffle,
            search: Search::new(guideline),
            generation: 0,
            estimate_reached: false,
            matches: 0,
            paused,
            shown: false,
            label: String::new(),
            tooltip: String::new(),
            last_set_text: String::new(),
            failed: None,
            signals: Signals::default(),
        }
    }

    pub fn snapshot(&self) -> Snapshot {
        Snapshot {
            label: self.label.clone(),
            tooltip: self.tooltip.clone(),
            paused: self.paused,
            matches: self.matches,
            searched: self.search.searched(),
            in_space: self.search.space.len(),
            signals: self.signals,
        }
    }

    /// The generation of the search being counted; a block's result for
    /// another is dropped (`_count_job_status`).
    pub fn generation(&self) -> u64 {
        self.generation
    }

    /// `PageShown`.
    pub fn show(&mut self) {
        self.shown = true;
    }

    /// `PageHidden`: the count does no work while it is not shown.
    pub fn hide(&mut self) {
        self.shown = false;
    }

    /// `_PausePlayCount`.
    pub fn pause_play(&mut self) {
        self.paused = !self.paused;
    }

    /// Whether the count is paused.
    pub fn paused(&self) -> bool {
        self.paused
    }

    /// `_RefreshPotentialDuplicateIdPairsAndDistances` (the refresh button):
    /// fetch the space again.
    pub fn refresh(&mut self, now: Instant) {
        self.search.initialised = false;
        self.search.fetch_started = false;
        self.search.initialised_at = None;
        self.search.space.clear();
        self.search.still.clear();
        self.search.hits = 0;
        self.restart(now);
    }

    /// The search changed (`_BroadcastValueChanged`): count it again, over
    /// the same space unless its file domain changed.
    pub fn search_changed(&mut self, file_domain_changed: bool, now: Instant) {
        if file_domain_changed {
            self.search.initialised = false;
            self.search.fetch_started = false;
            self.search.initialised_at = None;
            self.search.space.clear();
            self.search.still.clear();
            self.search.hits = 0;
        }
        self.restart(now);
    }

    /// `_RefreshDuplicateCounts`: a new search over the same space.
    fn restart(&mut self, now: Instant) {
        self.signals.restarted += 1;
        self.matches = 0;
        self.generation += 1;
        self.failed = None;
        let mut search = Search::new(self.guideline);
        if self.search.initialised {
            search.set_space(self.search.space.clone(), self.shuffle, now);
        }
        self.search = search;
        self.estimate_reached = false;
    }

    /// The fetch of the space finished (`_InitialisePotentialDuplicatePairs`'s
    /// publish).
    pub fn space_fetched(&mut self, rows: Vec<R>, now: Instant) {
        self.search.set_space(rows, self.shuffle, now);
        self.restart(now);
    }

    /// The fetch or a block failed; its error is the label.
    pub fn fail(&mut self, error: String) {
        self.label.clone_from(&error);
        self.failed = Some(error);
        self.show_label(false);
    }

    /// `_WeAreDoingACountEstimateAndHaveEnough`.
    fn estimating_with_enough(&mut self, options: Options) -> bool {
        if !options.stops_to_estimate || self.search.bit_left() {
            return false;
        }
        if self.estimate_reached {
            return true;
        }
        let enough = self.search.relative_error() <= REL_ERROR;
        if enough {
            self.estimate_reached = true;
        }
        enough
    }

    /// `_UpdateCountLabel`.
    fn update_label(&mut self, options: Options) {
        let mut numbers = false;
        if !self.search.initialised {
            self.label = "initialising\u{2026}".into();
        } else if self.search.space.is_empty() {
            self.label = "no potential pairs in this file domain!".into();
        } else {
            let in_space = self.search.space.len();
            let matches = human_int(self.matches as u64);
            self.label = if self.estimating_with_enough(options) {
                if self.search.done() {
                    format!("{} pairs; {matches} match", human_int(in_space as u64))
                } else {
                    // cut 25,123 to 25,000
                    let mut base = self.search.estimated_hits();
                    let mut multiplier = 1;
                    while base.to_string().len() > 2 {
                        base /= 10;
                        multiplier *= 10;
                    }
                    format!(
                        "{} pairs; ~{} match",
                        human_int(in_space as u64),
                        human_int((base * multiplier) as u64)
                    )
                }
            } else if self.search.done() {
                format!(
                    "{} pairs searched; {matches} match",
                    human_int(in_space as u64)
                )
            } else {
                format!(
                    "{} pairs searched; {matches} match\u{2026}",
                    value_range(self.search.searched() as u64, in_space as u64)
                )
            };
            numbers = true;
        }
        self.show_label(numbers);
    }

    /// `BetterStaticText.setText` (a changed text is its tooltip, wrapped)
    /// and then the numbers' tooltip.
    fn show_label(&mut self, numbers: bool) {
        if self.label != self.last_set_text {
            self.last_set_text.clone_from(&self.label);
            self.tooltip = wrap_tooltip(&self.label);
        }
        if numbers {
            self.tooltip = wrap_tooltip(TOOLTIP);
        }
    }

    /// `pre_work_callable`: what to do next, and the label as that leaves it.
    pub fn pre_work(&mut self, options: Options, now: Instant) -> PreWork {
        if !self.shown || self.failed.is_some() {
            return PreWork::Idle;
        }
        if self.search.initialised && self.search.is_stale(now) {
            self.refresh(now);
            return self.pre_work(options, now);
        }
        if !self.search.initialised {
            if !self.search.fetch_started {
                self.search.fetch_started = true;
                self.update_label(options);
                return PreWork::Fetch;
            }
            return PreWork::Idle;
        }
        if self.estimating_with_enough(options) {
            self.update_label(options);
            return PreWork::Idle;
        }
        if self.search.done() || self.paused {
            self.update_label(options);
            return PreWork::Idle;
        }
        PreWork::Block
    }

    /// The block the database searches: the next one, or all that is left
    /// when the hit rate is low (the file-search optimisation). Its second
    /// part is whether that was chosen.
    pub fn take_block(&mut self, options: Options) -> (Vec<R>, bool) {
        let search = &mut self.search;
        let file_based = search.file_based_search_is_ok(options.file_search_optimisation)
            && search.hit_rate_below(0.01)
            && search.estimated_hits().max(1) * 500 < search.still.len();
        let n = if file_based {
            search.still.len()
        } else {
            search.block_size.min(search.still.len())
        };
        (search.still.drain(..n.max(1)).collect(), file_based)
    }

    /// `publish_callable`: `hits` pairs of a block took `seconds`. The result
    /// of a block for a search that has since changed is dropped.
    pub fn publish(&mut self, generation: u64, hits: usize, seconds: f64, options: Options) {
        if generation != self.generation {
            return;
        }
        self.search.notify_work_time(seconds, IDEAL_WORK_SECONDS);
        self.search.hits += hits;
        if self.matches == 0 && hits > 0 {
            self.signals.has_pairs += 1;
        }
        self.matches += hits;
        if self.search.done() && self.matches == 0 {
            self.signals.no_pairs += 1;
        }
        self.update_label(options);
    }
}

// --- a worker for it ---

/// Where the pairs and their hits come from.
pub trait Source<R>: Send + 'static {
    /// Every potential pair in the search's file domain.
    fn space(&mut self) -> Result<Vec<R>, String>;
    /// How many of `rows` the search finds. `query_generation` changes with
    /// the search, so a source that prepares it (runs its file searches) can
    /// do that again.
    fn hits(&mut self, query_generation: u64, rows: &[R]) -> Result<usize, String>;
}

/// Why a worker is not working.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Parked {
    No,
    /// With nothing to do.
    Idle,
    /// A fetch or a block is chosen and held at the [`Gate`].
    Gate,
}

struct Inner<R> {
    counter: Counter<R>,
    options: Options,
    query_generation: u64,
    dirty: bool,
    parked: Parked,
    /// Whether the worker has reached the [`Gate`] it is parked at.
    at_gate: bool,
    /// How many fetches and blocks have finished.
    epoch: u64,
}

struct Shared<R> {
    inner: Mutex<Inner<R>>,
    wake: Condvar,
    stop: AtomicBool,
}

impl<R> Shared<R> {
    fn lock(&self) -> MutexGuard<'_, Inner<R>> {
        self.inner.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

/// A [`Counter`] and the thread working it. Dropping it stops the thread
/// after the block it is on.
pub struct Handle<R> {
    shared: Arc<Shared<R>>,
}

impl<R> std::fmt::Debug for Handle<R> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Handle").finish_non_exhaustive()
    }
}

impl<R: Clone + Send + 'static> Handle<R> {
    /// Start counting (the page has been shown).
    pub fn start(
        guideline: usize,
        paused: bool,
        shuffle: bool,
        options: Options,
        source: impl Source<R>,
        gate: Option<Arc<Gate>>,
    ) -> Self {
        let mut counter = Counter::new(guideline, paused, shuffle);
        counter.show();
        let shared = Arc::new(Shared {
            inner: Mutex::new(Inner {
                counter,
                options,
                query_generation: 0,
                dirty: true,
                parked: Parked::No,
                at_gate: false,
                epoch: 0,
            }),
            wake: Condvar::new(),
            stop: AtomicBool::new(false),
        });
        let worker = Arc::clone(&shared);
        let _ = std::thread::Builder::new()
            .name("duplicates-count".into())
            .spawn(move || run(&worker, source, gate.as_deref()));
        Self { shared }
    }

    fn poke(&self, change: impl FnOnce(&mut Inner<R>)) {
        let mut inner = self.shared.lock();
        change(&mut inner);
        inner.dirty = true;
        drop(inner);
        self.shared.wake.notify_all();
    }

    pub fn snapshot(&self) -> Snapshot {
        self.shared.lock().counter.snapshot()
    }

    /// The play/pause button.
    pub fn pause_play(&self) {
        self.poke(|i| i.counter.pause_play());
    }

    /// The refresh button.
    pub fn refresh(&self) {
        self.poke(|i| i.counter.refresh(Instant::now()));
    }

    /// The search changed; `file_domain_changed` if the space must be
    /// fetched again.
    pub fn search_changed(&self, file_domain_changed: bool) {
        self.poke(|i| {
            i.query_generation += 1;
            i.counter
                .search_changed(file_domain_changed, Instant::now());
        });
    }

    /// An option of the cog changed (`NotifyCountOptionsChanged`).
    pub fn set_options(&self, options: Options) {
        self.poke(|i| i.options = options);
    }

    /// The page is shown or hidden again.
    pub fn set_shown(&self, shown: bool) {
        self.poke(|i| {
            if shown {
                i.counter.show();
            } else {
                i.counter.hide();
            }
        });
    }

    /// How many fetches and blocks have finished (for tests).
    pub fn epoch(&self) -> u64 {
        self.shared.lock().epoch
    }

    /// Whether the worker has nothing to do until something changes or a
    /// block is let through the [`Gate`] (for tests).
    pub fn is_settled(&self) -> bool {
        let inner = self.shared.lock();
        match inner.parked {
            Parked::No => false,
            Parked::Idle => !inner.dirty,
            Parked::Gate => inner.at_gate,
        }
    }

    /// Wait until [`Handle::is_settled`] (for tests); whether it did within
    /// `timeout`.
    pub fn wait_settled(&self, timeout: Duration) -> bool {
        let start = Instant::now();
        while start.elapsed() < timeout {
            if self.is_settled() {
                return true;
            }
            std::thread::sleep(Duration::from_millis(1));
        }
        false
    }
}

impl<R> Drop for Handle<R> {
    fn drop(&mut self) {
        self.shared.stop.store(true, Ordering::SeqCst);
        self.shared.wake.notify_all();
    }
}

fn run<R: Clone + Send + 'static>(
    shared: &Shared<R>,
    mut source: impl Source<R>,
    gate: Option<&Gate>,
) {
    loop {
        let (generation, query_generation) = {
            let mut inner = shared.lock();
            loop {
                if shared.stop.load(Ordering::SeqCst) {
                    return;
                }
                inner.dirty = false;
                let options = inner.options;
                match inner.counter.pre_work(options, Instant::now()) {
                    PreWork::Fetch => {
                        inner.parked = Parked::Gate;
                        inner.at_gate = false;
                        drop(inner);
                        if let Some(gate) = gate {
                            gate.wait(&shared.stop, Waiting::Space, || {
                                shared.lock().at_gate = true;
                            });
                        }
                        let fetched = source.space();
                        inner = shared.lock();
                        inner.parked = Parked::No;
                        inner.at_gate = false;
                        inner.epoch += 1;
                        match fetched {
                            Ok(rows) => inner.counter.space_fetched(rows, Instant::now()),
                            Err(error) => inner.counter.fail(error),
                        }
                    }
                    PreWork::Block => {
                        inner.parked = Parked::Gate;
                        inner.at_gate = false;
                        break (inner.counter.generation(), inner.query_generation);
                    }
                    PreWork::Idle => {
                        inner.parked = Parked::Idle;
                        while !inner.dirty && !shared.stop.load(Ordering::SeqCst) {
                            inner = shared
                                .wake
                                .wait_timeout(inner, Duration::from_millis(250))
                                .unwrap_or_else(PoisonError::into_inner)
                                .0;
                        }
                    }
                }
            }
        };
        let held = gate.and_then(|g| {
            g.wait(&shared.stop, Waiting::Block, || {
                shared.lock().at_gate = true;
            })
        });
        let (rows, _) = {
            let mut inner = shared.lock();
            inner.parked = Parked::No;
            inner.at_gate = false;
            if inner.counter.generation() != generation {
                // (the search changed while the block waited; it is dropped)
                inner.epoch += 1;
                continue;
            }
            // (the database reads the options as it searches the block)
            let options = inner.options;
            inner.counter.take_block(options)
        };
        let started = Instant::now();
        let result = source.hits(query_generation, &rows);
        let seconds = held.unwrap_or_else(|| started.elapsed().as_secs_f64());
        let mut inner = shared.lock();
        inner.epoch += 1;
        match result {
            Ok(hits) => {
                let options = inner.options;
                inner.counter.publish(generation, hits, seconds, options);
            }
            Err(error) => {
                if inner.counter.generation() == generation {
                    inner.counter.fail(error);
                }
            }
        }
    }
}

/// A switch for tests: a count that has one waits before each fetch and
/// block until the test lets it through, so a test can see the count between
/// two blocks. A window's counts take the gate installed for their thread
/// ([`Gate::install_here`]); other counts have none.
pub struct Gate {
    state: Mutex<GateState>,
    changed: Condvar,
}

/// What waits at a gate.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Waiting {
    /// The fetch of the search space.
    Space,
    /// A block of the search.
    Block,
}

struct GateState {
    held: bool,
    waiting: Option<Waiting>,
    /// A block let through, with the seconds it is to count as taking.
    permit: Option<f64>,
    guideline: usize,
}

thread_local! {
    static HERE: std::cell::RefCell<Option<Arc<Gate>>> = const { std::cell::RefCell::new(None) };
}

impl std::fmt::Debug for Gate {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Gate").finish_non_exhaustive()
    }
}

impl Gate {
    /// A gate that holds work, and a first block of `guideline` pairs.
    pub fn new(guideline: usize) -> Arc<Self> {
        Arc::new(Self {
            state: Mutex::new(GateState {
                held: true,
                waiting: None,
                permit: None,
                guideline,
            }),
            changed: Condvar::new(),
        })
    }

    /// Make this the gate of the counts the windows of this thread start.
    pub fn install_here(self: &Arc<Self>) {
        HERE.with(|here| *here.borrow_mut() = Some(Arc::clone(self)));
    }

    /// The gate of this thread's windows, if a test installed one.
    pub fn here() -> Option<Arc<Self>> {
        HERE.with(|here| here.borrow().clone())
    }

    /// Stop holding: work goes on freely.
    pub fn free(&self) {
        let mut state = self.state.lock().unwrap_or_else(PoisonError::into_inner);
        state.held = false;
        state.permit = None;
        drop(state);
        self.changed.notify_all();
    }

    /// The size of the first block.
    pub fn guideline(&self) -> usize {
        self.state
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .guideline
    }

    /// Whether a fetch or block waits.
    pub fn waiting(&self) -> Option<Waiting> {
        self.state
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .waiting
    }

    /// Let the waiting fetch or block through, counting as having taken
    /// `seconds`; false if none waits.
    pub fn release(&self, seconds: f64) -> bool {
        self.release_waiting(None, seconds)
    }

    /// Let the fetch through, if it waits.
    pub fn release_space(&self) -> bool {
        self.release_waiting(Some(Waiting::Space), 0.0)
    }

    /// Let the block through, if one waits, counting as having taken
    /// `seconds`.
    pub fn release_block(&self, seconds: f64) -> bool {
        self.release_waiting(Some(Waiting::Block), seconds)
    }

    fn release_waiting(&self, kind: Option<Waiting>, seconds: f64) -> bool {
        let mut state = self.state.lock().unwrap_or_else(PoisonError::into_inner);
        // (one already let through is not yet taken: nothing else waits)
        if state.waiting.is_none()
            || state.permit.is_some()
            || (kind.is_some() && state.waiting != kind)
        {
            return false;
        }
        state.permit = Some(seconds);
        drop(state);
        self.changed.notify_all();
        true
    }

    /// Wait for a permit if the gate is held; the seconds a test has the
    /// block count as taking.
    fn wait(&self, stop: &AtomicBool, kind: Waiting, arrived: impl FnOnce()) -> Option<f64> {
        let mut state = self.state.lock().unwrap_or_else(PoisonError::into_inner);
        if !state.held {
            return None;
        }
        state.waiting = Some(kind);
        // (a worker waits here, which is what tests look for)
        let mut arrived = Some(arrived);
        let result = loop {
            if let Some(arrived) = arrived.take() {
                drop(state);
                arrived();
                state = self.state.lock().unwrap_or_else(PoisonError::into_inner);
                continue;
            }
            if let Some(permit) = state.permit.take() {
                break Some(permit);
            }
            if !state.held || stop.load(Ordering::SeqCst) {
                break None;
            }
            state = self
                .changed
                .wait_timeout(state, Duration::from_millis(20))
                .unwrap_or_else(PoisonError::into_inner)
                .0;
        };
        state.waiting = None;
        result
    }
}

/// The store's pairs, counted for a search the window can change.
#[derive(Debug)]
pub struct StoreSource {
    store: Arc<Store>,
    search: Arc<Mutex<DuplicatesSearch>>,
    prepared: Option<(u64, PreparedQuery)>,
}

impl StoreSource {
    pub fn new(store: Arc<Store>, search: Arc<Mutex<DuplicatesSearch>>) -> Self {
        Self {
            store,
            search,
            prepared: None,
        }
    }

    fn query(&self) -> Result<(Arc<hydrus_store::Snapshot>, PotentialsQuery), String> {
        let search = self
            .search
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone();
        let snapshot = self.store.snapshot();
        let query = PotentialsQuery::from_search(&snapshot, &search).map_err(|e| e.to_string())?;
        Ok((snapshot, query))
    }
}

impl Source<PairRow> for StoreSource {
    fn space(&mut self) -> Result<Vec<PairRow>, String> {
        let (snapshot, query) = self.query()?;
        self.store
            .read(|conn| query.space(conn, &snapshot))
            .map_err(|e| e.to_string())
    }

    fn hits(&mut self, query_generation: u64, rows: &[PairRow]) -> Result<usize, String> {
        if self.prepared.as_ref().map(|(g, _)| *g) != Some(query_generation) {
            let (snapshot, query) = self.query()?;
            let prepared = self
                .store
                .read(|conn| query.prepare(conn, &snapshot))
                .map_err(|e| e.to_string())?
                .map_err(|e| e.to_string())?;
            self.prepared = Some((query_generation, prepared));
        }
        let Some((_, prepared)) = &self.prepared else {
            return Ok(0);
        };
        self.store
            .read(|conn| prepared.count_matching(conn, rows))
            .map_err(|e| e.to_string())
    }
}

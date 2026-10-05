//! Real Help > Debug popup producers, owned by one main-window binding.
use std::{
    cell::{Cell, RefCell},
    collections::BTreeSet,
    rc::Rc,
    sync::Arc,
    time::{Duration, Instant},
};

use hydrus_store::{Store, popups};
use slint::ComponentHandle as _;

use crate::MainWindow;

const DELAYED_MESSAGE: &str = "This is a delayed popup message.";
const DELAY: Duration = Duration::from_secs(5);

const WORDS: [&str; 5] = ["test", "a", "longish", "statictext", "m8"];
type Clock = Rc<dyn Fn() -> Duration>;
type Word = Rc<dyn Fn() -> usize>;

#[derive(Clone)]
struct Update {
    due: Duration,
    key: [u8; 32],
    title: bool,
    text: String,
}
struct State {
    window: slint::Weak<MainWindow>,
    store: Arc<Store>,
    binding_active: Rc<Cell<bool>>,
    active: Cell<bool>,
    timer: slint::Timer,
    clock: RefCell<Clock>,
    word: RefCell<Word>,
    pending: RefCell<Vec<Update>>,
    delayed: RefCell<Vec<Duration>>,
    published: RefCell<Rc<dyn Fn()>>,
}
impl State {
    fn live(&self) -> bool {
        self.active.get() && self.binding_active.get() && self.window.upgrade().is_some()
    }
    fn retire(&self) {
        self.active.set(false);
        self.timer.stop();
        self.pending.borrow_mut().clear();
        self.delayed.borrow_mut().clear();
    }
    // One weak single-shot timer follows the earliest owned deadline across
    // both producers; admitting another launch never postpones existing work.
    // Preserve the long producer's 200 ms liveness/dismissal poll even when
    // its only remaining update is a future title (or MainWindow disappears).
    fn arm(self: &Rc<Self>) {
        if !self.live() {
            self.retire();
            return;
        }
        let next = self
            .pending
            .borrow()
            .iter()
            .map(|update| update.due)
            .chain(self.delayed.borrow().iter().copied())
            .min();
        let Some(next) = next else {
            self.timer.stop();
            return;
        };
        let now = (self.clock.borrow().clone())();
        let weak = Rc::downgrade(self);
        self.timer.start(
            slint::TimerMode::SingleShot,
            next.saturating_sub(now).min(Duration::from_millis(200)),
            move || {
                if let Some(state) = weak.upgrade() {
                    state.tick();
                }
            },
        );
    }
    fn tick(self: &Rc<Self>) {
        if !self.live() {
            self.retire();
            return;
        }
        // Dismissal is a live boundary even before the first title update is due.
        let saved_now = hydrus_core::TimestampMs::now().0 / 1000;
        let keys = match self.store.read(|conn| {
            Ok(popups::all(conn, saved_now)?
                .into_iter()
                .map(|job| job.key)
                .collect::<BTreeSet<_>>())
        }) {
            Ok(keys) => keys,
            Err(error) => {
                self.retire();
                eprintln!("Could not read the debug long-text popups: {error}");
                return;
            }
        };
        self.pending
            .borrow_mut()
            .retain(|update| keys.contains(&update.key));
        let now = (self.clock.borrow().clone())();
        let (mut due, pending): (Vec<_>, Vec<_>) = std::mem::take(&mut *self.pending.borrow_mut())
            .into_iter()
            .partition(|update| update.due <= now);
        *self.pending.borrow_mut() = pending;
        let (delayed_due, delayed_pending): (Vec<_>, Vec<_>) =
            std::mem::take(&mut *self.delayed.borrow_mut())
                .into_iter()
                .partition(|due| *due <= now);
        *self.delayed.borrow_mut() = delayed_pending;
        if due.is_empty() && delayed_due.is_empty() {
            self.arm();
            return;
        }
        due.sort_by_key(|update| update.due);
        let now = hydrus_core::TimestampMs::now().0 / 1000;
        let result = self.store.write(move |ctx| {
            let mut removed = BTreeSet::new();
            let mut changed = false;
            for update in due {
                if popups::update(ctx.conn(), &update.key, now, |job| {
                    if update.title {
                        job.status_title = Some(update.text);
                    } else {
                        job.status_text_1 = Some(update.text);
                    }
                })?
                .is_some()
                {
                    changed = true;
                } else {
                    removed.insert(update.key);
                }
            }
            for _ in delayed_due {
                popups::add(
                    ctx.conn(),
                    &popups::Job::text(DELAYED_MESSAGE, now as f64),
                    now,
                )?;
                changed = true;
            }
            Ok((removed, changed))
        });
        match result {
            Ok((removed, changed)) => {
                self.pending
                    .borrow_mut()
                    .retain(|update| !removed.contains(&update.key));
                if changed {
                    let published = self.published.borrow().clone();
                    published();
                }
                self.arm();
            }
            Err(error) => {
                self.retire();
                eprintln!("Could not update the debug long-text popups: {error}");
            }
        }
    }
}
impl Drop for State {
    fn drop(&mut self) {
        self.timer.stop();
    }
}

/// A retained handle cannot schedule or publish after its original owner retires.
#[derive(Clone)]
pub struct Control(Rc<State>);
pub(crate) struct Owner(Control);
impl Drop for Owner {
    fn drop(&mut self) {
        self.0.retire();
    }
}
impl std::fmt::Debug for Control {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DebugLongPopup")
            .field("active", &self.0.active.get())
            .field("pending", &self.pending_updates())
            .field("delayed", &self.pending_delayed_popups())
            .finish_non_exhaustive()
    }
}
impl Control {
    pub(crate) fn owner(&self) -> Owner {
        Owner(self.clone())
    }
    pub(crate) fn new(
        window: &MainWindow,
        store: Arc<Store>,
        binding_active: Rc<Cell<bool>>,
    ) -> Self {
        let started = Instant::now();
        Self(Rc::new(State {
            window: window.as_weak(),
            store,
            binding_active,
            active: Cell::new(true),
            timer: slint::Timer::default(),
            clock: RefCell::new(Rc::new(move || started.elapsed())),
            word: RefCell::new(Rc::new(|| rand::random_range(0..WORDS.len()))),
            pending: RefCell::default(),
            delayed: RefCell::default(),
            published: RefCell::new(Rc::new(|| {})),
        }))
    }
    pub(crate) fn set_published(&self, published: Rc<dyn Fn()>) {
        *self.0.published.borrow_mut() = published;
    }
    /// Replace this owner's monotonic clock before starting a sequence.
    pub fn set_clock(&self, clock: Clock) {
        if self.0.live() && self.0.pending.borrow().is_empty() && self.0.delayed.borrow().is_empty()
        {
            *self.0.clock.borrow_mut() = clock;
        }
    }
    /// Deterministic word choice belongs to the owner, not process preferences.
    pub fn set_word_source(&self, word: Word) {
        if self.0.live() {
            *self.0.word.borrow_mut() = word;
        }
    }
    pub fn pending_updates(&self) -> usize {
        self.0.pending.borrow().len()
    }
    pub fn pending_delayed_popups(&self) -> usize {
        self.0.delayed.borrow().len()
    }
    /// The actual GUI action queues no JobStatus until its five-second deadline.
    pub fn start_delayed_popup(&self) {
        if !self.0.live()
            || !self
                .0
                .window
                .upgrade()
                .is_some_and(|window| window.window().is_visible())
        {
            return;
        }
        let now = (self.0.clock.borrow().clone())();
        self.0.delayed.borrow_mut().push(now + DELAY);
        self.0.arm();
    }
    pub fn timer_running(&self) -> bool {
        self.0.timer.running()
    }
    pub fn retire(&self) {
        self.0.retire();
    }
    /// Drain the same due-update boundary used by the owned GUI timer.
    pub fn tick(&self) {
        self.0.tick();
    }
    /// Qt publishes two immediately dismissible cards, with text then title growth.
    pub fn start(&self) {
        if !self.0.live()
            || !self
                .0
                .window
                .upgrade()
                .is_some_and(|window| window.window().is_visible())
        {
            return;
        }
        let started = (self.0.clock.borrow().clone())();
        let word = self.0.word.borrow().clone();
        let choose = || WORDS[word() % WORDS.len()];
        let now = hydrus_core::TimestampMs::now().0 / 1000;
        let mut updates = Vec::with_capacity(124);
        let mut jobs = Vec::with_capacity(2);
        let mut step = 0;
        for title in [false, true] {
            let mut text = choose().to_owned();
            let job = popups::Job::text(if title { "test long title" } else { &text }, now as f64);
            for _ in 2..64 {
                text.push(' ');
                text.push_str(choose());
                step += 1;
                updates.push(Update {
                    due: started + Duration::from_millis(step * 200),
                    key: job.key,
                    title,
                    text: text.clone(),
                });
            }
            jobs.push(job);
        }
        if let Err(error) = self.0.store.write(move |ctx| {
            for job in jobs {
                popups::add(ctx.conn(), &job, now)?;
            }
            Ok(())
        }) {
            eprintln!("Could not publish the debug long-text popups: {error}");
            return;
        }
        self.0.pending.borrow_mut().extend(updates);
        let published = self.0.published.borrow().clone();
        published();
        self.0.arm();
    }
}

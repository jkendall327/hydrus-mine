//! The manage subscriptions dialog (network > subscriptions…) as the
//! reference's `EditSubscriptionsPanel`: the subscriptions it holds until
//! "apply", its list (sorted, selected as a list selects), and what its
//! buttons do to the selected: pause/resume, scrub delays, check queries
//! now (asking "Check which?" as `DoAliveOrDeadCheck` does), select
//! subscriptions by query text, and delete. Recorded by
//! `oracle/record_subscriptions_list.py`.

use hydrus_core::import_options::ImportOptionsSlice;
use hydrus_core::numbers::human_int;
use hydrus_core::subscriptions::{CheckerOptions, QueryState, SeedTime, SubscriptionSettings};
use hydrus_store::queues::{SeedStatus, StatusCounts};

use crate::edit_subscription::{LogChange, RetryIgnored};
use crate::list_selection::ListSelection;
use crate::subscriptions_list::{QueryFacts, ShortSummary, SubscriptionFacts, subscription_row};

/// A subscription's query as the dialog holds it.
#[derive(Debug, Clone, PartialEq)]
pub struct DialogQuery {
    /// Its queue (its file log), if it has one yet.
    pub queue: Option<i64>,
    pub state: QueryState,
    /// Its file log's seeds by status, and when each was found and
    /// posted.
    pub files: StatusCounts,
    pub seed_times: Vec<SeedTime>,
    /// The notes of its ignored files, for "retry ignored".
    pub ignored_notes: Vec<String>,
    /// What to do to its file log on "apply", in order.
    pub log_changes: Vec<LogChange>,
    /// For a query not in the store yet, the queue whose file log it
    /// starts with a copy of (a duplicated subscription's).
    pub copy_of: Option<i64>,
    /// Imported full history remains detached until the owner applies.
    pub exchange: Option<hydrus_downloader_exchange::subscriptions::Query>,
}

impl DialogQuery {
    /// A query not in the store yet.
    pub fn new(state: QueryState) -> Self {
        Self {
            queue: None,
            state,
            files: StatusCounts::new(),
            seed_times: Vec::new(),
            ignored_notes: Vec::new(),
            log_changes: Vec::new(),
            copy_of: None,
            exchange: None,
        }
    }

    /// How many of its files have this status.
    pub fn count(&self, status: SeedStatus) -> usize {
        self.files.get(&status).copied().unwrap_or(0)
    }

    /// Move `n` files from one status to another in the counts.
    fn move_count(&mut self, from: SeedStatus, to: SeedStatus, n: usize) {
        if n == 0 {
            return;
        }
        let left = self.count(from).saturating_sub(n);
        if left == 0 {
            self.files.remove(&from);
        } else {
            self.files.insert(from, left);
        }
        *self.files.entry(to).or_default() += n;
    }

    /// Never checked, alive, unpaused, its file log emptied (`Reset`).
    pub fn reset(&mut self) {
        let state = &mut self.state;
        state.last_check_time = 0;
        state.next_check_time = 0;
        state.dead = false;
        state.paused = false;
        self.files.clear();
        self.seed_times.clear();
        self.ignored_notes.clear();
        self.log_changes.push(LogChange::Reset);
    }

    /// Its failed files to be tried again, if it has any.
    pub fn retry_failed(&mut self) {
        let n = self.count(SeedStatus::Error);
        if n > 0 {
            self.move_count(SeedStatus::Error, SeedStatus::Unknown, n);
            self.log_changes.push(LogChange::RetryFailed);
        }
    }

    /// Its ignored files that `which` chooses to be tried again, if it has
    /// any ignored.
    pub fn retry_ignored(&mut self, which: RetryIgnored) {
        if self.count(SeedStatus::Vetoed) == 0 {
            return;
        }
        let before = self.ignored_notes.len();
        self.ignored_notes.retain(|note| !which.matches(note));
        let n = before - self.ignored_notes.len();
        self.move_count(SeedStatus::Vetoed, SeedStatus::Unknown, n);
        self.log_changes.push(LogChange::RetryIgnored(which));
    }

    /// Its check times and status reckoned again with these checker
    /// options (`SyncToQueryLogContainer`): DEAD if files come too slowly
    /// (and paused, with no files left to get), and when to check next.
    pub fn sync_to_checker(&mut self, checker: &CheckerOptions, now: i64) {
        let state = &mut self.state;
        if state.check_now {
            state.next_check_time = 0;
            state.dead = false;
            return;
        }
        if checker.is_dead(&self.seed_times, state.last_check_time) {
            state.dead = true;
            if self.files.get(&SeedStatus::Unknown).copied().unwrap_or(0) == 0 {
                state.paused = true;
            }
        }
        state.next_check_time =
            checker.next_check_time(&self.seed_times, state.last_check_time, now);
    }

    /// When its latest file was found (0 for none).
    pub fn latest_added(&self) -> i64 {
        self.seed_times.iter().map(|t| t.created).max().unwrap_or(0)
    }

    /// Check it at the next chance (`CheckNow`): unpaused, alive, due.
    pub fn check_now(&mut self) {
        self.state.check_now = true;
        self.state.paused = false;
        self.state.next_check_time = 0;
        self.state.dead = false;
    }

    /// What its row says, its subscription's checker reckoning its
    /// velocity.
    pub fn facts(&self, settings: &SubscriptionSettings) -> QueryFacts {
        QueryFacts {
            query_text: self.state.query_text.clone(),
            display_name: self.state.display_name.clone(),
            paused: self.state.paused,
            dead: self.state.dead,
            check_now: self.state.check_now,
            last_check_time: self.state.last_check_time,
            next_check_time: self.state.next_check_time,
            files: self.files.clone(),
            latest_added: self.latest_added(),
            velocity: settings.checker.pretty_velocity(
                &self.seed_times,
                self.state.last_check_time,
                false,
            ),
            additional_tags: String::new(),
        }
    }
}

/// A subscription as the dialog holds it.
#[derive(Debug, Clone, PartialEq)]
pub struct DialogSubscription {
    /// The dialog's own name for it, which its selection follows.
    pub key: u64,
    /// Its id in the store, if it is there yet.
    pub id: Option<i64>,
    pub name: String,
    pub settings: SubscriptionSettings,
    pub queries: Vec<DialogQuery>,
    /// The queues of its queries deleted in the edit dialog.
    pub deleted_queries: Vec<i64>,
}

impl DialogSubscription {
    pub fn facts(&self) -> SubscriptionFacts {
        SubscriptionFacts {
            name: self.name.clone(),
            gug_name: self.settings.gug_name.clone(),
            paused: self.settings.paused,
            no_work_until: self.settings.no_work_until,
            no_work_until_reason: self.settings.no_work_until_reason.clone(),
            queries: self
                .queries
                .iter()
                .map(|q| q.facts(&self.settings))
                .collect(),
            import_options: crate::import_options_editor::container_summary(
                &self.settings.import_options,
                &str::to_owned,
            ),
        }
    }

    /// A query not already set to check now (`CanCheckNow`).
    pub fn can_check_now(&self) -> bool {
        self.queries.iter().any(|q| !q.state.check_now)
    }

    /// A query checked at least once (`CanReset`).
    pub fn can_reset(&self) -> bool {
        self.queries.iter().any(|q| q.state.last_check_time != 0)
    }

    /// Delayed until later (`CanScrubDelay`).
    pub fn can_scrub_delay(&self, now: i64) -> bool {
        now <= self.settings.no_work_until
    }

    pub fn scrub_delay(&mut self) {
        self.settings.no_work_until = 0;
        self.settings.no_work_until_reason.clear();
    }

    /// A query whose text has this in it, case and all
    /// (`HasQuerySearchTextFragment`).
    pub fn has_query_text(&self, fragment: &str) -> bool {
        self.queries
            .iter()
            .any(|q| q.state.query_text.contains(fragment))
    }
}

/// The question "delete" asks (the reference's lists' simple delete).
pub const DELETE_QUESTION: &str = "Remove all selected?";

/// Clipboard mode names used by the subscription menu. The reference's
/// subscription callbacks intentionally differ from its shared editor routing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImportOptionsPaste {
    Replace,
    Merge,
    FillIn,
}

impl ImportOptionsPaste {
    /// The actual v688 subscription popup binding, verified by the recorder.
    pub fn from_menu_index(index: usize) -> Option<Self> {
        match index {
            0 | 3 => Some(Self::Replace),
            1 => Some(Self::Merge),
            2 => Some(Self::FillIn),
            _ => None,
        }
    }

    pub fn apply(self, current: &mut ImportOptionsSlice, incoming: &ImportOptionsSlice) {
        match self {
            Self::Replace => current.clone_from(incoming),
            Self::Merge => {
                let mut merged = incoming.clone();
                merged.fill_in(current);
                *current = merged;
            }
            Self::FillIn => current.fill_in(incoming),
        }
    }
}

/// What "select subscriptions" asks for.
pub const SELECT_MESSAGE: &str = "This selects subscriptions based on query text. Please enter some search text, and any subscription that has a query that includes that text will be selected.";

/// A question with buttons to choose from (`SelectFromListButtons`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Choice {
    pub title: String,
    pub message: String,
    pub choices: Vec<String>,
}

/// The dialog: the subscriptions, those deleted, the list's sort and
/// selection.
#[derive(Debug, Clone, Default)]
pub struct Subscriptions {
    pub subscriptions: Vec<DialogSubscription>,
    /// Those deleted that are in the store.
    pub deleted: Vec<i64>,
    /// Those in the store gone from the list into others (merged, or
    /// separated), their queries now the others'.
    pub absorbed: Vec<i64>,
    /// The list's sort: a column and whether ascending.
    pub sort: (usize, bool),
    pub selection: ListSelection<u64>,
    next_key: u64,
}

impl Subscriptions {
    /// The dialog on these subscriptions (with their ids), sorted by name.
    pub fn new(
        subscriptions: Vec<(Option<i64>, String, SubscriptionSettings, Vec<DialogQuery>)>,
    ) -> Self {
        let mut dialog = Self::default();
        for (id, name, settings, queries) in subscriptions {
            dialog.push(id, name, settings, queries);
        }
        dialog.sort = (0, true);
        dialog
    }

    /// Add one, returning its key.
    pub fn push(
        &mut self,
        id: Option<i64>,
        name: String,
        settings: SubscriptionSettings,
        queries: Vec<DialogQuery>,
    ) -> u64 {
        let key = self.next_key;
        self.next_key += 1;
        self.subscriptions.push(DialogSubscription {
            key,
            id,
            name,
            settings,
            queries,
            deleted_queries: Vec::new(),
        });
        key
    }

    /// `name`, or with " (1)", " (2)" and so on after it, the first no
    /// other subscription has, casefolded (`SetNonDupeName`).
    fn non_dupe_name(&self, name: &str, except: Option<u64>) -> String {
        let taken: Vec<String> = self
            .subscriptions
            .iter()
            .filter(|s| Some(s.key) != except)
            .map(|s| hydrus_core::casefold::casefold(&s.name))
            .collect();
        crate::favourites::non_dupe_name(name, &|n| {
            taken.contains(&hydrus_core::casefold::casefold(n))
        })
    }

    /// Add a new subscription as the edit dialog gave it back ("add"),
    /// renamed if its name is taken; it is selected.
    pub fn add_edited(
        &mut self,
        name: &str,
        settings: SubscriptionSettings,
        queries: Vec<DialogQuery>,
    ) -> u64 {
        let name = self.non_dupe_name(name, None);
        let key = self.push(None, name, settings, queries);
        self.selection.select_many(&[key]);
        key
    }

    /// Replace a subscription with what the edit dialog gave back ("edit"):
    /// a changed name is renamed if another has it (the reference counts
    /// the subscription's own old name as taken too, so a change of case
    /// alone gets " (1)"; hydrus-rs doesn't).
    pub fn replace_edited(
        &mut self,
        key: u64,
        name: &str,
        settings: SubscriptionSettings,
        queries: Vec<DialogQuery>,
        deleted_queries: Vec<i64>,
    ) {
        let unchanged = self.get(key).is_some_and(|s| s.name == name);
        let name = if unchanged {
            name.to_owned()
        } else {
            self.non_dupe_name(name, Some(key))
        };
        if let Some(s) = self.get_mut(key) {
            s.name = name;
            s.settings = settings;
            s.queries = queries;
            s.deleted_queries.extend(deleted_queries);
        }
    }

    pub fn get(&self, key: u64) -> Option<&DialogSubscription> {
        self.subscriptions.iter().find(|s| s.key == key)
    }

    pub(crate) fn get_mut(&mut self, key: u64) -> Option<&mut DialogSubscription> {
        self.subscriptions.iter_mut().find(|s| s.key == key)
    }

    /// The subscriptions in the list's order: by the sort column's text,
    /// casefolded (by time for the times, by count for the counts), then
    /// by name.
    pub fn order(&self, now: i64) -> Vec<u64> {
        let short = ShortSummary::default();
        let mut keyed: Vec<(SortKey, String, u64)> = self
            .subscriptions
            .iter()
            .map(|s| {
                let facts = s.facts();
                let row = subscription_row(&facts, now, short);
                (
                    sort_key(&facts, &row, self.sort.0, now),
                    row[0].to_lowercase(),
                    s.key,
                )
            })
            .collect();
        keyed.sort_by(|a, b| a.0.cmp(&b.0).then_with(|| a.1.cmp(&b.1)));
        if !self.sort.1 {
            keyed.reverse();
        }
        keyed.into_iter().map(|k| k.2).collect()
    }

    /// The list's rows, in order: each subscription's key, cells and
    /// whether it is selected.
    pub fn rows(&self, now: i64, short: ShortSummary) -> Vec<(u64, Vec<String>, bool)> {
        self.rows_with_format(
            now,
            short,
            &hydrus_store::settings::GuiFormatting::default(),
        )
    }
    pub fn rows_with_format(
        &self,
        now: i64,
        short: ShortSummary,
        formatting: &hydrus_store::settings::GuiFormatting,
    ) -> Vec<(u64, Vec<String>, bool)> {
        self.order(now)
            .into_iter()
            .filter_map(|key| {
                let s = self.get(key)?;
                Some((
                    key,
                    crate::subscriptions_list::subscription_row_with_format(
                        &s.facts(),
                        now,
                        short,
                        formatting,
                    ),
                    self.selection.is_selected(key),
                ))
            })
            .collect()
    }

    /// The selected, in the list's order.
    pub fn selected(&self, now: i64) -> Vec<u64> {
        self.selection.in_order(&self.order(now))
    }

    /// The clear confirmation uses the selection's current visible order.
    pub fn clear_import_options_question(&self, now: i64) -> Option<String> {
        let names = self
            .selected(now)
            .iter()
            .filter_map(|key| self.get(*key))
            .map(|s| s.name.as_str())
            .collect::<Vec<_>>();
        (!names.is_empty())
            .then(|| format!("Clear all custom import options from {}?", names.join(", ")))
    }

    /// Apply clipboard options only to the snapshotted selection.
    pub fn paste_import_options(
        &mut self,
        keys: &[u64],
        mode: ImportOptionsPaste,
        incoming: &ImportOptionsSlice,
    ) {
        for key in keys {
            if let Some(subscription) = self.subscriptions.iter_mut().find(|s| s.key == *key) {
                mode.apply(&mut subscription.settings.import_options, incoming);
            }
        }
    }

    /// Clearing returns all eight kinds to inheritance.
    pub fn clear_import_options(&mut self, keys: &[u64]) {
        self.paste_import_options(
            keys,
            ImportOptionsPaste::Replace,
            &ImportOptionsSlice::default(),
        );
    }

    pub fn click(&mut self, now: i64, row: usize, ctrl: bool, shift: bool) {
        let order = self.order(now);
        self.selection.click(&order, row, ctrl, shift);
    }

    fn selected_subscriptions(&self, now: i64) -> Vec<&DialogSubscription> {
        self.selected(now)
            .into_iter()
            .filter_map(|k| self.get(k))
            .collect()
    }

    pub fn can_check_now(&self, now: i64) -> bool {
        self.selected_subscriptions(now)
            .iter()
            .any(|s| s.can_check_now())
    }

    pub fn can_reset(&self, now: i64) -> bool {
        self.selected_subscriptions(now)
            .iter()
            .any(|s| s.can_reset())
    }

    pub fn can_scrub_delays(&self, now: i64) -> bool {
        self.selected_subscriptions(now)
            .iter()
            .any(|s| s.can_scrub_delay(now))
    }

    /// Pause or resume each selected (`PauseResume`).
    pub fn pause_resume(&mut self, now: i64) {
        for key in self.selected(now) {
            if let Some(s) = self.get_mut(key) {
                s.settings.paused = !s.settings.paused;
            }
        }
    }

    pub fn scrub_delays(&mut self, now: i64) {
        for key in self.selected(now) {
            if let Some(s) = self.get_mut(key) {
                s.scrub_delay();
            }
        }
    }

    /// Select those with a query that has this text in it
    /// (`SelectSubscriptions`).
    pub fn select_by_text(&mut self, now: i64, fragment: &str) {
        let matching: Vec<u64> = self
            .order(now)
            .into_iter()
            .filter(|&k| self.get(k).is_some_and(|s| s.has_query_text(fragment)))
            .collect();
        self.selection.select_many(&matching);
    }

    /// "duplicate": a copy of each selected, named as its own name not
    /// taken ("name (1)"), each query with a copy of its file log; the
    /// last copy is selected, as the reference adds them one by one.
    pub fn duplicate_selected(&mut self, now: i64) -> Vec<u64> {
        let mut made = Vec::new();
        for key in self.selected(now) {
            let Some(original) = self.get(key).cloned() else {
                continue;
            };
            let queries = original
                .queries
                .into_iter()
                .map(|mut q| {
                    if let Some(query) = &mut q.exchange {
                        hydrus_downloader_exchange::subscriptions::rename_history(
                            query,
                            hydrus_core::pages::PageKey::random().to_hex(),
                        );
                    }
                    DialogQuery {
                        queue: None,
                        copy_of: q.queue.or(q.copy_of),
                        ..q
                    }
                })
                .collect();
            let name = self.non_dupe_name(&original.name, None);
            made.push(self.push(None, name, original.settings, queries));
        }
        self.selection = ListSelection::default();
        self.selection
            .select_many(&made[made.len().saturating_sub(1)..]);
        made
    }

    /// Delete the selected (after [`DELETE_QUESTION`]).
    pub fn delete_selected(&mut self, now: i64) {
        let doomed = self.selected(now);
        for key in &doomed {
            if let Some(id) = self.get(*key).and_then(|s| s.id) {
                self.deleted.push(id);
            }
            self.selection.forget(*key);
        }
        self.subscriptions.retain(|s| !doomed.contains(&s.key));
    }
}

/// What "duplicate" says it did.
pub fn added_message(n: usize) -> String {
    format!("{} objects added!", human_int(n as u64))
}

/// What "reset" asks.
pub const RESET_QUESTION: &str = "Resetting these subscriptions will delete all their remembered urls, meaning when they next run, they will try to download them all over again. This may be expensive in time and data. Only do it if you are willing to wait. Do you want to do it?";

/// What "lowercase" asks.
pub const LOWERCASE_QUESTION: &str = "This will convert the selected subscriptions' queries to lowercase text. \"My_Query\" will become \"my_query\". Most sites do not care about case, but it can help things stay neat.";

/// What "merge" asks first.
pub const MERGE_QUESTION: &str = "Are you sure you want to merge the selected subscriptions? This will combine all selected subscriptions that share the same downloader, wrapping all their different queries into one subscription.\n\nThis is a big operation, so if it does not do what you expect, hit cancel afterwards!\n\nPlease note that all other subscription settings settings (like paused status and file limits and tag options) will be merged as well, so double-check your merged subs' settings afterwards.";

/// What "merge" asks of each group: which is primary.
pub const MERGE_PRIMARY: &str = "select the primary subscription--into which to merge the others";

/// What "merge" says when nothing can merge.
pub const NOT_MERGEABLE: &str =
    "Unfortunately, none of those subscriptions appear to be mergeable!";

/// What "separate" asks of more than two queries, and its answers.
pub const SEPARATE_QUESTION: &str = "Are you sure you want to separate the selected subscriptions? Separating breaks merged subscriptions apart into smaller pieces.";

/// What "separate" asks for: the new subscriptions' base name.
pub const SEPARATE_NAME: &str =
    "Please enter the base name for the new subscriptions. They will be named '[NAME]: query'.";

/// How "separate" breaks a subscription up.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Separate {
    /// The first half of its queries (by text) into "name (A)", the rest
    /// staying as "name (B)".
    Half,
    /// Each query its own subscription, "base: query".
    Whole,
    /// Only these queries (by index) taken out: into one subscription
    /// named "base" (`merged`), or each its own, "base: query"; the
    /// subscription keeps the rest.
    Part { queries: Vec<usize>, merged: bool },
}

/// The answers to [`SEPARATE_QUESTION`]: half, whole, and some.
pub const SEPARATE_CHOICES: [&str; 3] = [
    "break it in half",
    "break it all into single-query subscriptions",
    "only extract some of the subscription",
];

/// The list "only extract some" ticks the queries in (none ticked).
pub const SEPARATE_PICK: &str = "select the queries to extract";

/// What "only extract some" asks of more than one query ticked, and its
/// answers (and "forget it").
pub const SEPARATE_MERGED_QUESTION: &str = "Do you want the extracted queries to be a new merged subscription, or many subscriptions with only one query?";
pub const SEPARATE_MERGED_CHOICES: [&str; 2] = [
    "one new merged subscription",
    "many subscriptions with only one query",
];

/// What "separate" asks for when the queries taken out make one
/// subscription: its name.
pub const SEPARATE_MERGED_NAME: &str = "Please enter the name for the new subscription.";

/// What ticking `picked` of a subscription's `total` queries leads to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Picked {
    /// Every one: separated whole.
    All,
    /// Several: asks whether merged.
    Several,
    /// One, or none: each its own.
    Few,
}

pub fn picked(picked: usize, total: usize) -> Picked {
    if picked == total {
        Picked::All
    } else if picked > 1 {
        Picked::Several
    } else {
        Picked::Few
    }
}

impl Subscriptions {
    fn selected_mut(&mut self, now: i64) -> Vec<&mut DialogSubscription> {
        let keys = self.selected(now);
        self.subscriptions
            .iter_mut()
            .filter(|s| keys.contains(&s.key))
            .collect()
    }

    fn any_selected_query(&self, now: i64, test: impl Fn(&DialogQuery) -> bool) -> bool {
        self.selected_subscriptions(now)
            .iter()
            .any(|s| s.queries.iter().any(&test))
    }

    /// A selected query with uppercase in its text.
    pub fn can_lowercase(&self, now: i64) -> bool {
        self.any_selected_query(now, |q| {
            q.state.query_text != q.state.query_text.to_lowercase()
        })
    }

    pub fn can_retry_failed(&self, now: i64) -> bool {
        self.any_selected_query(now, |q| q.count(SeedStatus::Error) > 0)
    }

    pub fn can_retry_ignored(&self, now: i64) -> bool {
        self.any_selected_query(now, |q| q.count(SeedStatus::Vetoed) > 0)
    }

    /// Two selected share a downloader, the empty ones aside
    /// (`_CanMerge`).
    pub fn can_merge(&self, now: i64) -> bool {
        let selected = self.selected_subscriptions(now);
        let mut downloaders: Vec<&str> = selected
            .iter()
            .filter(|s| !s.queries.is_empty())
            .map(|s| s.settings.gug_name.as_str())
            .collect();
        downloaders.sort_unstable();
        downloaders.dedup();
        downloaders.len() < selected.len()
    }

    /// One selected, with more than one query.
    pub fn can_separate(&self, now: i64) -> bool {
        match self.selected_subscriptions(now).as_slice() {
            [one] => one.queries.len() > 1,
            _ => false,
        }
    }

    /// Reset every query of the selected (after [`RESET_QUESTION`]).
    pub fn reset_selected(&mut self, now: i64) {
        for s in self.selected_mut(now) {
            for q in &mut s.queries {
                q.reset();
            }
        }
    }

    pub fn retry_failed_selected(&mut self, now: i64) {
        for s in self.selected_mut(now) {
            for q in &mut s.queries {
                q.retry_failed();
            }
        }
    }

    pub fn retry_ignored_selected(&mut self, now: i64, which: RetryIgnored) {
        for s in self.selected_mut(now) {
            for q in &mut s.queries {
                q.retry_ignored(which);
            }
        }
    }

    /// Lowercase the selected's queries' texts (after
    /// [`LOWERCASE_QUESTION`]).
    pub fn lowercase_selected(&mut self, now: i64) {
        for s in self.selected_mut(now) {
            for q in &mut s.queries {
                q.state.query_text = q.state.query_text.to_lowercase();
            }
        }
    }

    /// The first selected's checker options, which "overwrite checker
    /// options" starts from.
    pub fn first_selected_checker(&self, now: i64) -> Option<CheckerOptions> {
        self.selected_subscriptions(now)
            .first()
            .map(|s| s.settings.checker.clone())
    }

    /// Give the selected these checker options, their queries' check
    /// times reckoned again where they change.
    pub fn set_checker_selected(&mut self, now: i64, checker: &CheckerOptions) {
        for s in self.selected_mut(now) {
            if s.settings.checker != *checker {
                s.settings.checker = checker.clone();
                for q in &mut s.queries {
                    q.sync_to_checker(checker, now);
                }
            }
        }
    }

    /// Give the selected this downloader ("overwrite downloader").
    pub fn set_downloader_selected(&mut self, now: i64, key: &str, name: &str) {
        for s in self.selected_mut(now) {
            key.clone_into(&mut s.settings.gug_key);
            name.clone_into(&mut s.settings.gug_name);
        }
    }

    /// Break the one selected up (`Separate`), the new subscriptions
    /// named from `base` (for [`Separate::Whole`]) and selected; their
    /// keys.
    pub fn separate(&mut self, now: i64, how: &Separate, base: &str) -> Vec<u64> {
        let [key] = self.selected(now)[..] else {
            return Vec::new();
        };
        let Some(original) = self.get(key).cloned() else {
            return Vec::new();
        };
        let mut made = Vec::new();
        match how {
            Separate::Whole => {
                self.forget(key);
                for q in &original.queries {
                    let name = format!("{base}: {}", q.state.human_name());
                    let name = self.non_dupe_name(&name, None);
                    made.push(self.push(None, name, original.settings.clone(), vec![q.clone()]));
                }
            }
            Separate::Half => {
                // the first half by text; each keeps the order it had, but
                // the last taken leads the new one, as the reference merges
                // them into it
                let mut by_text: Vec<usize> = (0..original.queries.len()).collect();
                by_text.sort_by_key(|&i| {
                    hydrus_core::sort::human_sort_key(&original.queries[i].state.query_text)
                });
                let taken = &by_text[..original.queries.len() / 2];
                let (mut queries, rest): (Vec<_>, Vec<_>) = original
                    .queries
                    .iter()
                    .cloned()
                    .enumerate()
                    .partition(|(i, _)| taken.contains(i));
                if let Some(last) = queries.pop() {
                    queries.insert(0, last);
                }
                let queries: Vec<DialogQuery> = queries.into_iter().map(|(_, q)| q).collect();
                let rest: Vec<DialogQuery> = rest.into_iter().map(|(_, q)| q).collect();
                // (the original's name not counted as taken)
                let a = self.non_dupe_name(&format!("{} (A)", original.name), Some(key));
                made.push(self.push(None, a, original.settings.clone(), queries));
                let b = self.non_dupe_name(&format!("{} (B)", original.name), Some(key));
                if let Some(s) = self.get_mut(key) {
                    s.name = b;
                    s.queries = rest;
                }
                made.push(key);
            }
            Separate::Part { queries, merged } => {
                let (taken, rest): (Vec<_>, Vec<_>) = original
                    .queries
                    .iter()
                    .cloned()
                    .enumerate()
                    .partition(|(i, _)| queries.contains(i));
                let mut taken: Vec<DialogQuery> = taken.into_iter().map(|(_, q)| q).collect();
                let rest: Vec<DialogQuery> = rest.into_iter().map(|(_, q)| q).collect();
                // (the new ones named as if the original had gone)
                if *merged {
                    // the last taken leads, as the reference merges the
                    // others into it
                    if let Some(last) = taken.pop() {
                        taken.insert(0, last);
                    }
                    let name = self.non_dupe_name(base, Some(key));
                    made.push(self.push(None, name, original.settings.clone(), taken));
                } else {
                    for q in taken {
                        let name = format!("{base}: {}", q.state.human_name());
                        let name = self.non_dupe_name(&name, Some(key));
                        made.push(self.push(None, name, original.settings.clone(), vec![q]));
                    }
                }
                // and the original's, after them
                let name = self.non_dupe_name(&original.name, Some(key));
                if let Some(s) = self.get_mut(key) {
                    s.name = name;
                    s.queries = rest;
                }
                made.push(key);
            }
        }
        self.selection.select_many(&made);
        made
    }

    /// Drop a subscription from the list without deleting its queries
    /// (they live on in others).
    fn forget(&mut self, key: u64) {
        if let Some(id) = self.get(key).and_then(|s| s.id) {
            self.absorbed.push(id);
        }
        self.selection.forget(key);
        self.subscriptions.retain(|s| s.key != key);
    }

    /// The selected that can merge, in groups sharing a downloader, each
    /// group by name (`GetMergeable`).
    pub fn merge_groups(&self, now: i64) -> Vec<Vec<u64>> {
        let mut groups: Vec<Vec<u64>> = Vec::new();
        let mut rest = self.selected_subscriptions(now);
        while let Some(primary) = rest.pop() {
            let (same, others): (Vec<_>, Vec<_>) = rest
                .into_iter()
                .partition(|s| s.settings.gug_name == primary.settings.gug_name);
            if !same.is_empty() {
                let mut group: Vec<&DialogSubscription> = vec![primary];
                group.extend(same);
                group.sort_by(|a, b| a.name.cmp(&b.name));
                groups.push(group.iter().map(|s| s.key).collect());
            }
            rest = others;
        }
        groups
    }

    /// Merge a group's others into its primary (`Merge`): their queries
    /// join the primary's, which is renamed `name` (made unique if it
    /// changed); the others go.
    pub fn merge(&mut self, primary: u64, group: &[u64], name: &str) {
        let mut queries = Vec::new();
        for &key in group.iter().filter(|&&k| k != primary) {
            if let Some(s) = self.get(key) {
                queries.extend(s.queries.iter().cloned());
            }
            self.forget(key);
        }
        let unchanged = self.get(primary).is_some_and(|s| s.name == name);
        let name = if unchanged {
            name.to_owned()
        } else {
            self.non_dupe_name(name, Some(primary))
        };
        if let Some(s) = self.get_mut(primary) {
            s.queries.extend(queries);
            s.name = name;
        }
    }
}

/// How a column sorts (`_ConvertSubscriptionToSortTuple`, simplified to
/// what the rows show).
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
enum SortKey {
    Text(String),
    Number(i64),
    Pair(i64, i64),
    Triple(i64, i64, i64),
}

fn sort_key(facts: &SubscriptionFacts, row: &[String], column: usize, now: i64) -> SortKey {
    let queries = &facts.queries;
    // (no time yet sorts as now)
    let or_now = |t: i64| if t == 0 { now } else { t };
    match column {
        2 => SortKey::Triple(
            queries.len() as i64,
            queries.iter().filter(|q| !q.dead && q.paused).count() as i64,
            queries.iter().filter(|q| q.dead).count() as i64,
        ),
        3 => SortKey::Number(or_now(
            queries.iter().map(|q| q.latest_added).max().unwrap_or(-1),
        )),
        4 => SortKey::Number(or_now(
            queries
                .iter()
                .map(|q| q.last_check_time)
                .max()
                .unwrap_or(-1),
        )),
        5 => SortKey::Number(if now > facts.no_work_until {
            0
        } else {
            facts.no_work_until - now
        }),
        6 => {
            let (mut total, mut unknown) = (0i64, 0i64);
            for q in queries {
                for (status, n) in &q.files {
                    total += *n as i64;
                    if *status == hydrus_store::queues::SeedStatus::Unknown {
                        unknown += *n as i64;
                    }
                }
            }
            SortKey::Pair(total, total - unknown)
        }
        7 => SortKey::Number(i64::from(facts.paused)),
        c => SortKey::Text(row.get(c).cloned().unwrap_or_default().to_lowercase()),
    }
}

/// "check queries now": the selected subscriptions' queries set to check
/// at the next chance, asking first about paused subscriptions, DEAD
/// queries and paused queries as `DoAliveOrDeadCheck` does.
#[derive(Debug, Clone)]
pub struct CheckNow {
    subscriptions: Vec<u64>,
    stage: Stage,
    do_paused_subscriptions: bool,
    do_alive: bool,
    do_dead: bool,
    do_paused_queries: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Stage {
    Start,
    AskedPausedSubscriptions,
    CheckDead,
    AskedDead,
    CheckPausedQueries,
    AskedPausedQueries,
    Ready,
}

const CHECK_WHICH: &str = "Check which?";

fn check_which(message: String, choices: &[&str]) -> Choice {
    Choice {
        title: CHECK_WHICH.into(),
        message,
        choices: choices.iter().map(|&c| c.to_owned()).collect(),
    }
}

/// "Check which?" about the DEAD among `total` queries: its answers are
/// check all, the alive, or the dead (or, when all are dead, resurrect
/// them or not).
pub fn dead_question(total: usize, dead: usize) -> Choice {
    let message = format!(
        "Of the {} selected queries, {} are DEAD. Do you want to check these?",
        human_int(total as u64),
        human_int(dead as u64)
    );
    if dead == total {
        check_which(
            message,
            &["yes, resurrect the DEAD queries", "no, leave them DEAD"],
        )
    } else {
        let alive = format!("check the {} ALIVE", human_int((total - dead) as u64));
        let resurrect = format!("resurrect and check the {} DEAD", human_int(dead as u64));
        check_which(message, &["yes, check all of them", &alive, &resurrect])
    }
}

/// What an answer to [`dead_question`] checks: (the alive, the dead).
pub fn dead_answer(all_dead: bool, index: usize) -> (bool, bool) {
    match (all_dead, index) {
        (true, 0) => (false, true),
        (true, _) => (false, false),
        (false, 0) => (true, true),
        (false, 1) => (true, false),
        (false, _) => (false, true),
    }
}

/// "Check which?" about the paused among `total` queries: the first
/// answer checks them too.
pub fn paused_queries_question(total: usize, paused: usize) -> Choice {
    let message = format!(
        "Of the {} selected queries, {} are paused. Do you want to unpause and check them?",
        human_int(total as u64),
        human_int(paused as u64)
    );
    if paused == total {
        check_which(message, &["yes, check them", "no, leave them alone"])
    } else {
        check_which(
            message,
            &[
                "yes check paused queries",
                "no just what is currently unpaused",
            ],
        )
    }
}

impl CheckNow {
    /// On the selected.
    pub fn new(dialog: &Subscriptions, now: i64) -> Self {
        Self {
            subscriptions: dialog.selected(now),
            stage: Stage::Start,
            do_paused_subscriptions: true,
            do_alive: true,
            do_dead: true,
            do_paused_queries: true,
        }
    }

    fn chosen<'a>(&self, dialog: &'a Subscriptions) -> Vec<&'a DialogSubscription> {
        self.subscriptions
            .iter()
            .filter_map(|&k| dialog.get(k))
            .collect()
    }

    /// The queries asked about: those of the subscriptions chosen, less
    /// the alive or the dead as answered (`filtered`).
    fn queries<'a>(&self, dialog: &'a Subscriptions, filtered: bool) -> Vec<&'a DialogQuery> {
        self.chosen(dialog)
            .into_iter()
            .filter(|s| self.do_paused_subscriptions || !s.settings.paused)
            .flat_map(|s| s.queries.iter())
            .filter(|q| {
                !filtered
                    || if q.state.dead {
                        self.do_dead
                    } else {
                        self.do_alive
                    }
            })
            .collect()
    }

    /// The question waiting on an answer, if one is.
    pub fn question(&self, dialog: &Subscriptions) -> Option<Choice> {
        let choice = check_which;
        match self.stage {
            Stage::AskedPausedSubscriptions => {
                let subs = self.chosen(dialog);
                let paused = subs.iter().filter(|s| s.settings.paused).count();
                let message = format!(
                    "Of the {} selected subscriptions, {} are paused. Do you want to unpause these paused subs and check their queries?",
                    human_int(subs.len() as u64),
                    human_int(paused as u64)
                );
                Some(if paused == subs.len() {
                    choice(
                        message,
                        &[
                            "yes, unpause them and check their queries",
                            "no, leave them alone",
                        ],
                    )
                } else {
                    choice(
                        message,
                        &[
                            "yes, check queries within paused subs",
                            "no, just check within unpaused subs",
                        ],
                    )
                })
            }
            Stage::AskedDead => {
                let queries = self.queries(dialog, false);
                let dead = queries.iter().filter(|q| q.state.dead).count();
                Some(dead_question(queries.len(), dead))
            }
            Stage::AskedPausedQueries => {
                let queries = self.queries(dialog, true);
                let paused = queries.iter().filter(|q| q.state.paused).count();
                Some(paused_queries_question(queries.len(), paused))
            }
            _ => None,
        }
    }

    /// On to the next question, if there is one; `None` when ready to
    /// [`apply`](Self::apply).
    pub fn next(&mut self, dialog: &Subscriptions) -> Option<Choice> {
        loop {
            match self.stage {
                Stage::Start => {
                    let any_paused = self.chosen(dialog).iter().any(|s| s.settings.paused);
                    self.stage = if any_paused {
                        Stage::AskedPausedSubscriptions
                    } else {
                        Stage::CheckDead
                    };
                }
                Stage::CheckDead => {
                    let any_dead = self.queries(dialog, false).iter().any(|q| q.state.dead);
                    self.stage = if any_dead {
                        Stage::AskedDead
                    } else {
                        Stage::CheckPausedQueries
                    };
                }
                Stage::CheckPausedQueries => {
                    let any_paused = self.queries(dialog, true).iter().any(|q| q.state.paused);
                    self.stage = if any_paused {
                        Stage::AskedPausedQueries
                    } else {
                        Stage::Ready
                    };
                }
                Stage::Ready => return None,
                Stage::AskedPausedSubscriptions | Stage::AskedDead | Stage::AskedPausedQueries => {
                    return self.question(dialog);
                }
            }
        }
    }

    /// The question asked answered with the choice at `index`.
    pub fn answer(&mut self, dialog: &Subscriptions, index: usize) {
        match self.stage {
            Stage::AskedPausedSubscriptions => {
                self.do_paused_subscriptions = index == 0;
                self.stage = Stage::CheckDead;
            }
            Stage::AskedDead => {
                let queries = self.queries(dialog, false);
                let all = queries.iter().all(|q| q.state.dead);
                (self.do_alive, self.do_dead) = dead_answer(all, index);
                self.stage = Stage::CheckPausedQueries;
            }
            Stage::AskedPausedQueries => {
                self.do_paused_queries = index == 0;
                self.stage = Stage::Ready;
            }
            _ => {}
        }
    }

    /// Set the queries chosen to check now; a subscription with any so
    /// set is resumed and its delay scrubbed.
    pub fn apply(self, dialog: &mut Subscriptions) {
        for key in &self.subscriptions {
            let Some(s) = dialog.get_mut(*key) else {
                continue;
            };
            if !self.do_paused_subscriptions && s.settings.paused {
                continue;
            }
            let mut any = false;
            for q in &mut s.queries {
                let wanted = if q.state.dead {
                    self.do_dead
                } else {
                    self.do_alive
                };
                if !wanted || (!self.do_paused_queries && q.state.paused) {
                    continue;
                }
                q.check_now();
                any = true;
            }
            if any {
                s.settings.paused = false;
                s.scrub_delay();
            }
        }
    }
}

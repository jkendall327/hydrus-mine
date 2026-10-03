//! The edit subscription dialog (the reference's `EditSubscriptionPanel`),
//! opened from the manage subscriptions dialog: a subscription's name, its
//! delay, its downloader, its queries list with that list's buttons
//! (add, copy and paste queries, edit, delete, pause/play, retry, check
//! now, reset) and the questions they ask, and its settings, held until
//! "apply". Recorded by `oracle/record_edit_subscription.py`.
//!
//! Changes to a query's file log (reset, retry) are kept as
//! [`LogChange`]s for whoever saves the dialog to carry out on the store;
//! the counts the list shows change at once.

use hydrus_core::numbers::human_int;
use hydrus_core::sort::human_sort_key;
use hydrus_core::subscriptions::{QueryState, SubscriptionSettings};
use hydrus_store::queues::SeedStatus;

use crate::list_selection::ListSelection;
use crate::subscriptions_dialog::{
    Choice, DialogQuery, dead_answer, dead_question, paused_queries_question,
};
use crate::subscriptions_list::{ShortSummary, delta_exact, full_human_name, query_row};

/// The dialog's title.
pub const TITLE: &str = "edit subscription";

/// The query editor's title.
pub const QUERY_TITLE: &str = "edit subscription query";

/// What "reset" asks.
pub const RESET_QUESTION: &str = "Resetting these queries will delete all their cached urls, meaning when the subscription next runs, they will have to download all those links over again. This may be expensive in time and data. Only do this if you know what it means. Do you want to do it?";

/// What pasting an empty clipboard says.
pub const EMPTY_CLIPBOARD: &str = "The clipboard did not seem to have anything in it!";

/// Which ignored files "retry ignored" retries
/// (`GetRetryIgnoredParam`): all, or those whose note matches.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RetryIgnored {
    All,
    /// Notes starting "403".
    Forbidden,
    /// Notes starting "404".
    NotFound,
    /// Notes ending "blacklisted!".
    Blacklisted,
}

impl RetryIgnored {
    /// The choices, in the reference's order, with their labels.
    pub const CHOICES: [(Self, &'static str); 4] = [
        (Self::All, "retry all"),
        (Self::Forbidden, "retry 403s"),
        (Self::NotFound, "retry 404s"),
        (Self::Blacklisted, "retry blacklisted"),
    ];

    /// The question's title.
    pub const TITLE: &'static str = "select what to retry";

    /// Whether an ignored file with this note is retried (the reference's
    /// regexes `^403`, `^404`, `blacklisted!$`).
    pub fn matches(self, note: &str) -> bool {
        match self {
            Self::All => true,
            Self::Forbidden => note.starts_with("403"),
            Self::NotFound => note.starts_with("404"),
            Self::Blacklisted => note.ends_with("blacklisted!"),
        }
    }
}

/// A change to a query's file log, made on "apply".
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogChange {
    /// Empty it.
    Reset,
    /// Failed files to be tried again.
    RetryFailed,
    /// Ignored files to be tried again.
    RetryIgnored(RetryIgnored),
}

/// A query in the dialog.
#[derive(Debug, Clone, PartialEq)]
pub struct EditQuery {
    /// The dialog's own name for it, which its selection follows.
    pub key: u64,
    pub query: DialogQuery,
}

impl EditQuery {
    fn text(&self) -> &str {
        &self.query.state.query_text
    }

    fn human_name(&self) -> String {
        full_human_name(
            &self.query.state.query_text,
            self.query.state.display_name.as_deref(),
        )
    }
}

/// What pasting queries found, and what it asks.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Paste {
    /// Nothing on the clipboard (a warning).
    Empty,
    /// Nothing to do (an information message).
    Nothing(String),
    /// A question: its message, its yes answers (the first adds the new
    /// and revives the DEAD, a second adds only the new) and its no.
    Ask {
        message: String,
        yeses: Vec<String>,
        no: String,
        plan: PastePlan,
    },
}

/// What a yes to pasting does.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PastePlan {
    /// The new query texts.
    pub new: Vec<String>,
    /// The DEAD queries pasted again.
    pub dead: Vec<u64>,
}

/// The dialog.
#[derive(Debug, Clone)]
pub struct EditSubscription {
    pub name: String,
    pub settings: SubscriptionSettings,
    pub queries: Vec<EditQuery>,
    /// Queries deleted that are in the store, by queue.
    pub deleted: Vec<i64>,
    /// The queries list's sort: a column and whether ascending.
    pub sort: (usize, bool),
    pub selection: ListSelection<u64>,
    next_key: u64,
}

impl EditSubscription {
    /// The dialog on a subscription, its queries sorted by name.
    pub fn new(name: String, settings: SubscriptionSettings, queries: Vec<DialogQuery>) -> Self {
        let mut dialog = Self {
            name,
            settings,
            queries: Vec::new(),
            deleted: Vec::new(),
            sort: (0, true),
            selection: ListSelection::default(),
            next_key: 0,
        };
        for query in queries {
            dialog.push(query);
        }
        dialog
    }

    /// Add a query; its key.
    pub fn push(&mut self, query: DialogQuery) -> u64 {
        let key = self.next_key;
        self.next_key += 1;
        self.queries.push(EditQuery { key, query });
        key
    }

    /// The subscription as edited: its name, settings and queries, in the
    /// list's order, and the queues of the queries deleted.
    pub fn into_parts(
        self,
        now: i64,
    ) -> (String, SubscriptionSettings, Vec<DialogQuery>, Vec<i64>) {
        let order = self.order(now);
        let mut queries = self.queries;
        queries.sort_by_key(|q| order.iter().position(|&k| k == q.key));
        (
            self.name,
            self.settings,
            queries.into_iter().map(|q| q.query).collect(),
            self.deleted,
        )
    }

    pub fn get(&self, key: u64) -> Option<&EditQuery> {
        self.queries.iter().find(|q| q.key == key)
    }

    fn get_mut(&mut self, key: u64) -> Option<&mut EditQuery> {
        self.queries.iter_mut().find(|q| q.key == key)
    }

    /// The line under the name: "no recent errors", or when it is delayed
    /// until and why (`_UpdateDelayText`).
    pub fn delay_text(&self, now: i64) -> String {
        if now > self.settings.no_work_until {
            "no recent errors".into()
        } else {
            format!(
                "delayed--retrying {} because: {}",
                delta_exact(self.settings.no_work_until, now),
                self.settings.no_work_until_reason
            )
        }
    }

    /// The downloader button's label, `found` saying whether the client
    /// has the downloader named.
    pub fn downloader_label(&self, found: bool) -> String {
        if self.settings.gug_name.is_empty() {
            "no downloader set".into()
        } else if found {
            self.settings.gug_name.clone()
        } else {
            format!("not found: {}", self.settings.gug_name)
        }
    }

    /// The queries in the list's order: by the sort column's text,
    /// casefolded, then by text.
    pub fn order(&self, now: i64) -> Vec<u64> {
        let mut keyed: Vec<(String, String, u64)> = self
            .queries
            .iter()
            .map(|q| {
                let row = query_row(&q.query.facts(&self.settings), now, ShortSummary::default());
                let cell = row.get(self.sort.0).cloned().unwrap_or_default();
                (cell.to_lowercase(), q.text().to_owned(), q.key)
            })
            .collect();
        keyed.sort();
        if !self.sort.1 {
            keyed.reverse();
        }
        keyed.into_iter().map(|k| k.2).collect()
    }

    /// The list's rows, in order: each query's key, cells and whether it
    /// is selected.
    pub fn rows(&self, now: i64, short: ShortSummary) -> Vec<(u64, Vec<String>, bool)> {
        self.order(now)
            .into_iter()
            .filter_map(|key| {
                let q = self.get(key)?;
                Some((
                    key,
                    query_row(&q.query.facts(&self.settings), now, short),
                    self.selection.is_selected(key),
                ))
            })
            .collect()
    }

    /// The selected, in the list's order.
    pub fn selected(&self, now: i64) -> Vec<u64> {
        self.selection.in_order(&self.order(now))
    }

    pub fn click(&mut self, now: i64, row: usize, ctrl: bool, shift: bool) {
        let order = self.order(now);
        self.selection.click(&order, row, ctrl, shift);
    }

    /// Select the queries with these texts, and only them.
    pub fn select_texts(&mut self, texts: &[&str]) {
        let keys: Vec<u64> = self
            .queries
            .iter()
            .filter(|q| texts.contains(&q.text()))
            .map(|q| q.key)
            .collect();
        self.selection.select_many(&keys);
    }

    fn selected_queries(&self, now: i64) -> Vec<&EditQuery> {
        self.selected(now)
            .into_iter()
            .filter_map(|k| self.get(k))
            .collect()
    }

    pub fn can_check_now(&self, now: i64) -> bool {
        self.selected_queries(now)
            .iter()
            .any(|q| !q.query.state.check_now)
    }

    /// A selected query checked at least once.
    pub fn can_reset(&self, now: i64) -> bool {
        self.selected_queries(now)
            .iter()
            .any(|q| q.query.state.last_check_time != 0)
    }

    pub fn can_retry_failed(&self, now: i64) -> bool {
        self.selected_queries(now)
            .iter()
            .any(|q| q.query.count(SeedStatus::Error) > 0)
    }

    pub fn can_retry_ignored(&self, now: i64) -> bool {
        self.selected_queries(now)
            .iter()
            .any(|q| q.query.count(SeedStatus::Vetoed) > 0)
    }

    /// The query texts, lowercased (not casefolded, as the reference),
    /// with the names of the queries that have each.
    fn lower_texts(&self) -> Vec<(String, String)> {
        let mut found: Vec<(String, String)> = Vec::new();
        let mut queries: Vec<&EditQuery> = self.queries.iter().collect();
        queries.sort_by_key(|q| q.key);
        for q in queries {
            let lower = q.text().to_lowercase();
            let name = q.human_name();
            match found.iter_mut().find(|(l, _)| *l == lower) {
                Some((_, names)) => {
                    names.push_str(", ");
                    names.push_str(&name);
                }
                None => found.push((lower, name)),
            }
        }
        found
    }

    fn name_for_text(&self, text: &str) -> Option<String> {
        let lower = text.to_lowercase();
        self.lower_texts()
            .into_iter()
            .find(|(l, _)| *l == lower)
            .map(|(_, n)| n)
    }

    /// Add a query with this state (from the query editor), selecting it,
    /// or the warning that refuses it: a query with its text is there
    /// already.
    pub fn add_query(&mut self, state: QueryState) -> Result<u64, String> {
        if let Some(name) = self.name_for_text(&state.query_text) {
            return Err(format!(
                "You already have a query for \"{name}\", so nothing new has been added."
            ));
        }
        let key = self.push(DialogQuery::new(state));
        self.selection.select_many(&[key]);
        Ok(key)
    }

    /// Replace a query's state with its edit, or the warning that refuses
    /// it: another query has the new text already. (The reference also
    /// refuses a change of case alone; hydrus-rs doesn't.)
    pub fn edit_query(&mut self, key: u64, state: QueryState) -> Result<(), String> {
        let Some(old) = self.get(key) else {
            return Ok(());
        };
        if state.query_text != old.query.state.query_text {
            let lower = state.query_text.to_lowercase();
            let clash: Vec<String> = self
                .queries
                .iter()
                .filter(|q| q.key != key && q.text().to_lowercase() == lower)
                .map(EditQuery::human_name)
                .collect();
            if !clash.is_empty() {
                return Err(format!(
                    "You already have a query for \"{}\"! The edit you just made will not be saved.",
                    clash.join(", ")
                ));
            }
        }
        if let Some(q) = self.get_mut(key) {
            q.query.state = state;
        }
        Ok(())
    }

    /// The selected queries' texts, a line each, for "copy queries".
    pub fn copy_queries(&self, now: i64) -> String {
        self.selected_queries(now)
            .iter()
            .map(|q| q.text().to_owned())
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// What pasting this clipboard text finds and asks (`_PasteQueries`).
    pub fn paste(&self, clipboard: &str) -> Paste {
        let pasted: Vec<&str> = clipboard
            .lines()
            .map(str::trim)
            .filter(|l| !l.is_empty())
            .collect();
        if pasted.is_empty() {
            return Paste::Empty;
        }
        let known = self.lower_texts();
        let mut existing_lower: Vec<String> = Vec::new();
        let mut existing_names: Vec<String> = Vec::new();
        let mut new: Vec<String> = Vec::new();
        for text in &pasted {
            let lower = text.to_lowercase();
            match known.iter().find(|(l, _)| *l == lower) {
                Some((_, name)) => {
                    push_unique(&mut existing_lower, lower);
                    push_unique(&mut existing_names, name.clone());
                }
                None => push_unique(&mut new, (*text).to_owned()),
            }
        }
        let mut dead: Vec<&EditQuery> = self
            .queries
            .iter()
            .filter(|q| q.query.state.dead && existing_lower.contains(&q.text().to_lowercase()))
            .collect();
        dead.sort_by_key(|q| q.key);
        let mut dead_names: Vec<String> = dead.iter().map(|q| q.human_name()).collect();
        human_sort(&mut dead_names);
        human_sort(&mut existing_names);
        let working: Vec<String> = existing_names
            .into_iter()
            .filter(|n| !dead_names.contains(n))
            .collect();
        human_sort(&mut new);

        let mut message = if pasted.len() == 1 {
            "I pulled one text from the clipboard. ".to_owned()
        } else {
            format!(
                "I pulled {} texts from the clipboard.\n\n",
                human_int(pasted.len() as u64)
            )
        };
        let mut block = |names: &[String], others: usize, labels: [&str; 4]| {
            if names.is_empty() {
                return;
            }
            let label = match (others == 0, names.len() == 1) {
                (true, true) => labels[0],
                (true, false) => labels[1],
                (false, true) => labels[2],
                (false, false) => labels[3],
            };
            message.push_str(label);
            message.push_str(&insertable_summary(names));
            message.push_str("\n\n");
        };
        block(
            &new,
            working.len() + dead_names.len(),
            [
                "It is new:",
                "They are all new:",
                "This is new:",
                "These are new:",
            ],
        );
        block(
            &working,
            new.len() + dead_names.len(),
            [
                "It is already working in the subscription:",
                "They are all already working in the subscription:",
                "This is already working in the subscription:",
                "These are already working in the subscription:",
            ],
        );
        block(
            &dead_names,
            new.len() + working.len(),
            [
                "It is already in the subscription but is DEAD:",
                "They are all already in the subscription but are DEAD:",
                "This is already in the subscription but is DEAD:",
                "These are already in the subscription but are DEAD:",
            ],
        );
        let (question, yeses): (&str, &[&str]) = match (new.is_empty(), dead.is_empty()) {
            (false, false) => (
                "Would you like to add the new queries and revive the DEAD?",
                &["do it", "add the new, but do not revive the DEAD"],
            ),
            (false, true) => ("Would you like to add these new queries?", &["do it"]),
            (true, false) => ("Would you like to revive these DEAD?", &["do it"]),
            (true, true) => ("So there are no actions to take.", &[]),
        };
        message.push_str(question);
        if yeses.is_empty() {
            return Paste::Nothing(message);
        }
        Paste::Ask {
            message,
            yeses: yeses.iter().map(|&y| y.to_owned()).collect(),
            no: "hold off".into(),
            plan: PastePlan {
                new,
                dead: dead.iter().map(|q| q.key).collect(),
            },
        }
    }

    /// Carry out a pasting answered yes; `revive` false for "add the new,
    /// but do not revive the DEAD". The new queries are selected.
    pub fn apply_paste(&mut self, plan: &PastePlan, revive: bool) {
        let mut added = Vec::new();
        for text in &plan.new {
            added.push(self.push(DialogQuery::new(QueryState::new(text.clone()))));
        }
        if !added.is_empty() {
            self.selection.select_many(&added);
        }
        if revive {
            for &key in &plan.dead {
                if let Some(q) = self.get_mut(key) {
                    q.query.check_now();
                }
            }
        }
    }

    /// Pause or play each selected.
    pub fn pause_play(&mut self, now: i64) {
        for key in self.selected(now) {
            if let Some(q) = self.get_mut(key) {
                q.query.state.paused = !q.query.state.paused;
            }
        }
    }

    /// Delete the selected (after "Remove all selected?").
    pub fn delete_selected(&mut self, now: i64) {
        let doomed = self.selected(now);
        for key in &doomed {
            if let Some(queue) = self.get(*key).and_then(|q| q.query.queue) {
                self.deleted.push(queue);
            }
            self.selection.forget(*key);
        }
        self.queries.retain(|q| !doomed.contains(&q.key));
    }

    /// Reset the selected (after [`RESET_QUESTION`]): never checked,
    /// alive, unpaused, their file logs emptied.
    pub fn reset_selected(&mut self, now: i64) {
        for key in self.selected(now) {
            if let Some(q) = self.get_mut(key) {
                q.query.reset();
            }
        }
    }

    /// Retry the selected's failed files; the delay is scrubbed.
    pub fn retry_failed(&mut self, now: i64) {
        for key in self.selected(now) {
            if let Some(q) = self.get_mut(key) {
                q.query.retry_failed();
            }
        }
        self.settings.no_work_until = 0;
    }

    /// Retry the selected's ignored files that `which` chooses; the delay
    /// is scrubbed.
    pub fn retry_ignored(&mut self, now: i64, which: RetryIgnored) {
        for key in self.selected(now) {
            if let Some(q) = self.get_mut(key) {
                q.query.retry_ignored(which);
            }
        }
        self.settings.no_work_until = 0;
    }

    /// New checker options, its queries' check times reckoned again
    /// (`_CheckerOptionsUpdated`).
    pub fn set_checker(&mut self, checker: hydrus_core::subscriptions::CheckerOptions, now: i64) {
        for q in &mut self.queries {
            q.query.sync_to_checker(&checker, now);
        }
        self.settings.checker = checker;
    }
}

fn push_unique(list: &mut Vec<String>, item: String) {
    if !list.contains(&item) {
        list.push(item);
    }
}

/// Sort as the reference sorts for people (`HumanTextSort`), ties by text.
fn human_sort(texts: &mut [String]) {
    texts.sort_by(|a, b| {
        human_sort_key(a)
            .cmp(&human_sort_key(b))
            .then_with(|| a.cmp(b))
    });
}

/// Some names to put in a message (`ConvertManyStringsToNiceInsertable
/// HumanSummary` with no trailing whitespace, already sorted): one quoted
/// on the line, a few a line each, many run together in lines of up to
/// 64 characters, at most 24 lines.
pub fn insertable_summary(texts: &[String]) -> String {
    const LONGEST: usize = 64;
    const MOST_LINES: usize = 24;
    if let [one] = texts {
        return format!(" \"{one}\"");
    }
    let body = if texts.len() <= 4 {
        texts.join("\n")
    } else {
        let mut lines: Vec<String> = Vec::new();
        let mut line = String::new();
        let mut rest = texts.iter();
        while let Some(text) = rest.next() {
            if line.is_empty() {
                line.clone_from(text);
                continue;
            }
            let longer = format!("{line}, {text}");
            if longer.chars().count() <= LONGEST {
                line = longer;
                continue;
            }
            lines.push(std::mem::take(&mut line));
            if lines.len() >= MOST_LINES {
                let others = 1 + rest.count();
                lines.push(format!("and {} others", human_int(others as u64)));
                break;
            }
            line.clone_from(text);
        }
        if !line.is_empty() {
            lines.push(line);
        }
        lines.join("\n")
    };
    format!("\n\n{body}")
}

/// "check now" on the selected queries: the "Check which?" questions
/// about DEAD and paused queries (`DoAliveOrDeadCheck` with no
/// subscriptions), then each chosen set to check now, and the delay
/// scrubbed.
#[derive(Debug, Clone)]
pub struct CheckQueriesNow {
    queries: Vec<u64>,
    stage: Stage,
    do_alive: bool,
    do_dead: bool,
    do_paused: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Stage {
    Start,
    AskedDead,
    CheckPaused,
    AskedPaused,
    Ready,
}

impl CheckQueriesNow {
    /// On the selected.
    pub fn new(dialog: &EditSubscription, now: i64) -> Self {
        Self {
            queries: dialog.selected(now),
            stage: Stage::Start,
            do_alive: true,
            do_dead: true,
            do_paused: true,
        }
    }

    fn chosen<'a>(&self, dialog: &'a EditSubscription, filtered: bool) -> Vec<&'a EditQuery> {
        self.queries
            .iter()
            .filter_map(|&k| dialog.get(k))
            .filter(|q| {
                !filtered
                    || if q.query.state.dead {
                        self.do_dead
                    } else {
                        self.do_alive
                    }
            })
            .collect()
    }

    /// The question waiting on an answer, if one is.
    pub fn question(&self, dialog: &EditSubscription) -> Option<Choice> {
        match self.stage {
            Stage::AskedDead => {
                let queries = self.chosen(dialog, false);
                let dead = queries.iter().filter(|q| q.query.state.dead).count();
                Some(dead_question(queries.len(), dead))
            }
            Stage::AskedPaused => {
                let queries = self.chosen(dialog, true);
                let paused = queries.iter().filter(|q| q.query.state.paused).count();
                Some(paused_queries_question(queries.len(), paused))
            }
            _ => None,
        }
    }

    /// On to the next question, if there is one; `None` when ready to
    /// [`apply`](Self::apply).
    pub fn next(&mut self, dialog: &EditSubscription) -> Option<Choice> {
        loop {
            match self.stage {
                Stage::Start => {
                    let any_dead = self
                        .chosen(dialog, false)
                        .iter()
                        .any(|q| q.query.state.dead);
                    self.stage = if any_dead {
                        Stage::AskedDead
                    } else {
                        Stage::CheckPaused
                    };
                }
                Stage::CheckPaused => {
                    let any_paused = self
                        .chosen(dialog, true)
                        .iter()
                        .any(|q| q.query.state.paused);
                    self.stage = if any_paused {
                        Stage::AskedPaused
                    } else {
                        Stage::Ready
                    };
                }
                Stage::Ready => return None,
                Stage::AskedDead | Stage::AskedPaused => return self.question(dialog),
            }
        }
    }

    /// The question asked answered with the choice at `index`.
    pub fn answer(&mut self, dialog: &EditSubscription, index: usize) {
        match self.stage {
            Stage::AskedDead => {
                let all = self
                    .chosen(dialog, false)
                    .iter()
                    .all(|q| q.query.state.dead);
                (self.do_alive, self.do_dead) = dead_answer(all, index);
                self.stage = Stage::CheckPaused;
            }
            Stage::AskedPaused => {
                self.do_paused = index == 0;
                self.stage = Stage::Ready;
            }
            _ => {}
        }
    }

    /// Set the queries chosen to check now, and scrub the delay.
    pub fn apply(self, dialog: &mut EditSubscription) {
        for key in &self.queries {
            let Some(q) = dialog.get_mut(*key) else {
                continue;
            };
            let state = &q.query.state;
            let wanted = if state.dead {
                self.do_dead
            } else {
                self.do_alive
            };
            if !wanted || (!self.do_paused && state.paused) {
                continue;
            }
            q.query.check_now();
        }
        dialog.settings.no_work_until = 0;
    }
}

/// An import options button's label (`_UpdateLabelAndToolTip`): "import
/// options (all default)", "(tags set)" for one kind set, or the kinds
/// set listed, cut to 48 characters.
pub fn import_options_label(options: &hydrus_core::import_options::ImportOptionsSlice) -> String {
    let kinds: Vec<&str> = [
        (options.prefetch.is_some(), "prefetch logic"),
        (options.file_filtering.is_some(), "file filtering"),
        (options.tag_filtering.is_some(), "tag filtering"),
        (options.locations.is_some(), "locations"),
        (options.tags.is_some(), "tags"),
        (options.notes.is_some(), "notes"),
        (options.presentation.is_some(), "presentation"),
        (options.external_programs.is_some(), "external programs"),
    ]
    .into_iter()
    .filter_map(|(set, name)| set.then_some(name))
    .collect();
    let label = match kinds.as_slice() {
        [] => "import options (all default)".to_owned(),
        [one] => format!("import options ({one} set)"),
        many => format!("import options ({})", many.join(", ")),
    };
    // (`ElideText`)
    if label.chars().count() > 48 {
        let mut cut: String = label.chars().take(47).collect();
        cut.push('\u{2026}');
        cut
    } else {
        label
    }
}

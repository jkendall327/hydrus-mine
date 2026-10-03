//! The manage subscriptions dialog's "deduplicate" (the reference's
//! `EditSubscriptionsPanel.DedupeAll`): queries with the same text on the
//! same downloader, in one subscription or several, brought down to one,
//! asking question by question which kind, which downloader, which texts
//! and which subscription keeps them. Recorded by
//! `oracle/record_subscriptions_dedupe.py`.

use std::collections::BTreeSet;

use hydrus_core::numbers::{human_int, value_range};

use crate::subscriptions_dialog::{Choice, DialogQuery, DialogSubscription, Subscriptions};

/// A question "deduplicate" asks.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Question {
    /// Buttons to choose from (`SelectFromListButtons`).
    Choice(Choice),
    YesNo(String),
    /// Two yeses and a "no" (`GetYesYesNo`).
    YesYesNo {
        message: String,
        yeses: Vec<String>,
        no: String,
    },
    /// Texts to tick, all ticked to start (`SelectMultipleFromList`).
    Multiple {
        title: String,
        choices: Vec<String>,
    },
    /// Said, and the work ends.
    Warning(String),
}

/// An answer to a [`Question`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Answer {
    /// The choice (or yes) at this index.
    Index(usize),
    /// A yes/no.
    Yes(bool),
    /// The texts ticked.
    Texts(Vec<String>),
    Cancel,
}

const NO_DUPLICATES: &str =
    "There are no apparent duplicates, the dupe data will now be recalculated.";

/// Each downloader's duplicate query texts (in the order first found),
/// and each subscription's.
#[derive(Debug, Clone, Default)]
struct DupeData {
    by_downloader: Vec<(String, BTreeSet<String>)>,
    by_subscription: Vec<(u64, BTreeSet<String>)>,
}

/// The duplicates among the dialog's subscriptions, in the list's order,
/// by text, or casefolded (`_RegenDupeData`).
fn dupe_data(dialog: &Subscriptions, now: i64, enforce_case: bool) -> DupeData {
    let mut counts: Vec<((String, String), Vec<u64>)> = Vec::new();
    for key in dialog.order(now) {
        let Some(s) = dialog.get(key) else {
            continue;
        };
        for q in &s.queries {
            let text = if enforce_case {
                q.state.query_text.clone()
            } else {
                q.state.query_text.to_lowercase()
            };
            let at = (s.settings.gug_name.clone(), text);
            match counts.iter_mut().find(|(k, _)| *k == at) {
                Some((_, subs)) => subs.push(key),
                None => counts.push((at, vec![key])),
            }
        }
    }
    let mut data = DupeData::default();
    for ((gug, text), subs) in counts {
        if subs.len() <= 1 {
            continue;
        }
        match data.by_downloader.iter_mut().find(|(g, _)| *g == gug) {
            Some((_, texts)) => {
                texts.insert(text.clone());
            }
            None => data
                .by_downloader
                .push((gug, BTreeSet::from([text.clone()]))),
        }
        for key in subs {
            match data.by_subscription.iter_mut().find(|(k, _)| *k == key) {
                Some((_, texts)) => {
                    texts.insert(text.clone());
                }
                None => data
                    .by_subscription
                    .push((key, BTreeSet::from([text.clone()]))),
            }
        }
    }
    data
}

/// "deduplicate" is enabled when there are duplicates of either kind.
pub fn can_dedupe(dialog: &Subscriptions, now: i64) -> bool {
    !dupe_data(dialog, now, false).by_downloader.is_empty()
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Stage {
    Case,
    OnlyCaseless,
    Downloader(Vec<String>),
    OneText,
    Texts,
    Select,
    /// The subscriptions offered, each with the texts it can keep.
    Master(Vec<(u64, BTreeSet<String>)>),
    More,
    Done,
}

/// "deduplicate", question by question.
#[derive(Debug, Clone)]
pub struct Dedupe {
    now: i64,
    stage: Stage,
    enforce_case: bool,
    downloader: String,
    /// The texts still to deduplicate.
    wanted: BTreeSet<String>,
}

fn text(q: &DialogQuery, enforce_case: bool) -> String {
    if enforce_case {
        q.state.query_text.clone()
    } else {
        q.state.query_text.to_lowercase()
    }
}

/// Keep one query for each of `texts`, the one with the most files, the
/// queries ordered most files first (`DedupeQueryTexts`).
fn dedupe_within(s: &mut DialogSubscription, texts: &BTreeSet<String>, enforce_case: bool) {
    let mut queries = std::mem::take(&mut s.queries);
    let files = |q: &DialogQuery| q.files.values().sum::<usize>();
    queries.sort_by_key(|q| std::cmp::Reverse(files(q)));
    let mut seen = BTreeSet::new();
    for q in queries {
        let t = text(&q, enforce_case);
        if texts.contains(&t) && seen.contains(&t) {
            s.deleted_queries.extend(q.queue);
            continue;
        }
        seen.insert(t);
        s.queries.push(q);
    }
}

/// Drop the queries with `texts` (`RemoveQueryTexts`).
fn remove_texts(s: &mut DialogSubscription, texts: &BTreeSet<String>, enforce_case: bool) {
    let (gone, kept): (Vec<DialogQuery>, Vec<DialogQuery>) = std::mem::take(&mut s.queries)
        .into_iter()
        .partition(|q| texts.contains(&text(q, enforce_case)));
    s.queries = kept;
    s.deleted_queries
        .extend(gone.into_iter().filter_map(|q| q.queue));
}

impl Dedupe {
    /// Pressed: the first question.
    pub fn start(dialog: &Subscriptions, now: i64) -> (Self, Question) {
        let mut dedupe = Self {
            now,
            stage: Stage::Done,
            enforce_case: false,
            downloader: String::new(),
            wanted: BTreeSet::new(),
        };
        let cased = !dupe_data(dialog, now, true).by_downloader.is_empty();
        let caseless = !dupe_data(dialog, now, false).by_downloader.is_empty();
        let question = if cased && caseless {
            dedupe.stage = Stage::Case;
            Question::Choice(Choice {
                title: "Caseless or cased?".into(),
                message: "Which kind of duplication are we going to do?".into(),
                choices: vec![
                    "do a normal upper/lower case deduplication".into(),
                    "only do exact text deduplication".into(),
                ],
            })
        } else if caseless {
            dedupe.stage = Stage::OnlyCaseless;
            Question::YesNo("There are no exact text duplicates. Only upper/lower case deduplication is available, merging queries like \"my_query\" and \"My_Query\". Is this ok?".into())
        } else {
            Question::Warning(NO_DUPLICATES.into())
        };
        (dedupe, question)
    }

    fn data(&self, dialog: &Subscriptions) -> DupeData {
        dupe_data(dialog, self.now, self.enforce_case)
    }

    fn end(&mut self) -> Option<Question> {
        self.stage = Stage::Done;
        None
    }

    fn warn(&mut self, message: &str) -> Question {
        self.stage = Stage::Done;
        Question::Warning(message.into())
    }

    /// Which downloader, the kind chosen.
    fn ask_downloader(&mut self, dialog: &Subscriptions) -> Question {
        let data = self.data(dialog);
        match &data.by_downloader[..] {
            [] => self.warn(NO_DUPLICATES),
            [(gug, _)] => {
                self.downloader.clone_from(gug);
                self.ask_texts(dialog)
            }
            several => {
                self.stage = Stage::Downloader(several.iter().map(|(g, _)| g.clone()).collect());
                Question::Choice(Choice {
                    title: "Which downloader to work on?".into(),
                    message: "Multiple downloaders have duplicate queries. Which would you like to dedupe now?".into(),
                    choices: several
                        .iter()
                        .map(|(g, texts)| {
                            format!("{g} ({} duplicates)", human_int(texts.len() as u64))
                        })
                        .collect(),
                })
            }
        }
    }

    fn potential(&self, dialog: &Subscriptions) -> BTreeSet<String> {
        self.data(dialog)
            .by_downloader
            .into_iter()
            .find(|(g, _)| *g == self.downloader)
            .map(|(_, texts)| texts)
            .unwrap_or_default()
    }

    /// Which texts, the downloader chosen.
    fn ask_texts(&mut self, dialog: &Subscriptions) -> Question {
        let potential = self.potential(dialog);
        match potential.len() {
            0 => self.warn("Strangely, there are actually no apparent duplicates for this downloader, the dupe data will now be recalculated. Let hydev know about this, please."),
            1 => {
                self.stage = Stage::OneText;
                let only = potential.iter().next().cloned().unwrap_or_default();
                Question::YesNo(format!(
                    "The downloader \"{}\" has a single duplicate query \"{only}\". Is this ok to dedupe?",
                    self.downloader
                ))
            }
            n => {
                self.stage = Stage::Texts;
                Question::YesYesNo {
                    message: format!(
                        "There are {} duplicate query texts for the downloader \"{}\". Would you like to dedupe them all, or select which to do?",
                        human_int(n as u64),
                        self.downloader
                    ),
                    yeses: vec!["do them all".into(), "select which to do".into()],
                    no: "forget it".into(),
                }
            }
        }
    }

    /// Which subscription keeps the texts still wanted.
    fn ask_master(&mut self, dialog: &Subscriptions) -> Question {
        let data = self.data(dialog);
        let mut offered: Vec<(u64, BTreeSet<String>)> = data
            .by_subscription
            .into_iter()
            .filter(|(key, _)| {
                dialog
                    .get(*key)
                    .is_some_and(|s| s.settings.gug_name == self.downloader)
            })
            .map(|(key, texts)| (key, texts.intersection(&self.wanted).cloned().collect()))
            .filter(|(_, can): &(u64, BTreeSet<String>)| !can.is_empty())
            .collect();
        let name = |key: u64| dialog.get(key).map(|s| s.name.clone()).unwrap_or_default();
        match offered.len() {
            0 => self.warn("Strangely, there are actually no subscriptions that can do dedupe work for the selected duplicates, the dupe data will now be recalculated. Let hydev know about this, please."),
            1 => {
                let message = format!(
                    "Subscription \"{}\" will now dedupe the queries within itself. Is this ok?",
                    name(offered[0].0)
                );
                self.stage = Stage::Master(offered);
                Question::YesNo(message)
            }
            _ => {
                offered.sort_by_key(|(key, _)| name(*key));
                let wanted = self.wanted.len();
                let choices = offered
                    .iter()
                    .map(|(key, can)| {
                        if can.len() == wanted {
                            format!("{} (has all duplicate queries)", name(*key))
                        } else {
                            format!(
                                "{} (has {} duplicate queries)",
                                name(*key),
                                value_range(can.len() as u64, wanted as u64)
                            )
                        }
                    })
                    .collect();
                self.stage = Stage::Master(offered);
                Question::Choice(Choice {
                    title: "Which sub to retain the queries on?".into(),
                    message: "Which subscription is going to keep the queries? If the subscription cannot dedupe them all, you will be able to select another, to do the rest, in a moment.".into(),
                    choices,
                })
            }
        }
    }

    /// `master` keeps `texts`; the downloader's other subscriptions lose
    /// them. Then, if texts are left, offer to go on.
    fn keep(
        &mut self,
        dialog: &mut Subscriptions,
        master: u64,
        texts: &BTreeSet<String>,
    ) -> Option<Question> {
        let enforce_case = self.enforce_case;
        let deduping: Vec<u64> = self
            .data(dialog)
            .by_subscription
            .into_iter()
            .map(|(key, _)| key)
            .filter(|key| {
                dialog
                    .get(*key)
                    .is_some_and(|s| s.settings.gug_name == self.downloader)
            })
            .collect();
        for key in deduping {
            if let Some(s) = dialog.get_mut(key) {
                if key == master {
                    dedupe_within(s, texts, enforce_case);
                } else {
                    remove_texts(s, texts, enforce_case);
                }
            }
        }
        self.wanted = self.wanted.difference(texts).cloned().collect();
        if self.wanted.is_empty() {
            return self.end();
        }
        self.stage = Stage::More;
        Some(Question::YesNo(format!(
            "Dedupe was done. There are still {} queries that can be deduped between other subscriptions. Want to do them now?",
            human_int(self.wanted.len() as u64)
        )))
    }

    /// The question asked answered; the next question, if any.
    pub fn answer(&mut self, dialog: &mut Subscriptions, answer: &Answer) -> Option<Question> {
        let yes = matches!(answer, Answer::Yes(true) | Answer::Index(0));
        match (std::mem::replace(&mut self.stage, Stage::Done), answer) {
            (_, Answer::Cancel) => self.end(),
            (Stage::Case, Answer::Index(i)) => {
                self.enforce_case = *i == 1;
                Some(self.ask_downloader(dialog))
            }
            (Stage::OnlyCaseless, _) if yes => {
                self.enforce_case = false;
                Some(self.ask_downloader(dialog))
            }
            (Stage::Downloader(gugs), Answer::Index(i)) => match gugs.get(*i) {
                Some(gug) => {
                    self.downloader.clone_from(gug);
                    Some(self.ask_texts(dialog))
                }
                None => self.end(),
            },
            (Stage::OneText, _) if yes => {
                self.wanted = self.potential(dialog);
                Some(self.ask_master(dialog))
            }
            (Stage::Texts, Answer::Index(0)) => {
                self.wanted = self.potential(dialog);
                Some(self.ask_master(dialog))
            }
            (Stage::Texts, Answer::Index(1)) => {
                self.stage = Stage::Select;
                Some(Question::Multiple {
                    title: "Select which query texts to dedupe".into(),
                    choices: self.potential(dialog).into_iter().collect(),
                })
            }
            (Stage::Select, Answer::Texts(texts)) => {
                if texts.is_empty() {
                    return self.end();
                }
                self.wanted = texts.iter().cloned().collect();
                Some(self.ask_master(dialog))
            }
            (Stage::Master(offered), _) => {
                let chosen = match (offered.len(), answer) {
                    (1, _) if yes => Some(0),
                    (n, Answer::Index(i)) if n > 1 => Some(*i),
                    _ => None,
                };
                match chosen.and_then(|i| offered.get(i)) {
                    Some((key, texts)) => {
                        let (key, texts) = (*key, texts.clone());
                        self.keep(dialog, key, &texts)
                    }
                    None => self.end(),
                }
            }
            (Stage::More, _) if yes => Some(self.ask_master(dialog)),
            _ => self.end(),
        }
    }
}

//! Siblings and parents editors. Each service retains its own workspace and
//! staged proposals; Apply commits them together. Conflict and cycle links
//! are automatically removed as in the reference's TagPairActionContext.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use hydrus_core::{ContentStatus, ServiceId, ServiceType, Tag, TagId};
use hydrus_store::Store;
use hydrus_store::content::tag_relations::{self, RelationAction, RelationUpdate};
pub use hydrus_store::display::RelationKind;

/// A stored (less ideal, ideal) or (child, parent) pair.
pub type Pair = (String, String);

/// A question raised by an editing action. No edit is staged until every
/// question has an answer; `None` cancels that part of the operation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Question {
    pub message: String,
    pub yes: String,
    pub no: String,
    pub reason: bool,
}

/// A relationship row, with its status prefix and explanatory note.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Row {
    pub pair: Pair,
    pub status: String,
    pub note: String,
    pub selected: bool,
}

#[derive(Debug, Clone, Default)]
struct State {
    current: BTreeSet<Pair>,
    pending: BTreeSet<Pair>,
    petitioned: BTreeSet<Pair>,
    reasons: BTreeMap<Pair, String>,
}

impl State {
    fn effective(&self) -> BTreeSet<Pair> {
        self.current
            .union(&self.pending)
            .filter(|p| !self.petitioned.contains(*p))
            .cloned()
            .collect()
    }
    fn listed(&self) -> BTreeSet<Pair> {
        self.current
            .union(&self.pending)
            .chain(&self.petitioned)
            .cloned()
            .collect()
    }
}

#[derive(Debug, Clone)]
struct Service {
    id: ServiceId,
    name: String,
    local: bool,
    original: State,
    state: State,
    left: BTreeSet<String>,
    right: BTreeSet<String>,
    workspace: BTreeSet<String>,
    show_all: bool,
    show_pending: bool,
    whole_chain: bool,
    selected: BTreeSet<Pair>,
    anchor: Option<Pair>,
}

/// A dialog's service pages and staged relationship changes.
#[derive(Debug)]
pub struct Relationships {
    store: Arc<Store>,
    kind: RelationKind,
    services: Vec<Service>,
    service: usize,
    sort: usize,
    ascending: bool,
}

impl Relationships {
    /// Load editable local tag services followed by tag repositories.
    /// A failed load is reported instead of appearing as an empty service.
    pub fn new(store: Arc<Store>, kind: RelationKind) -> hydrus_store::Result<Self> {
        let snapshot = store.snapshot();
        let mut services = Vec::new();
        for ty in [ServiceType::LocalTag, ServiceType::TagRepository] {
            for service in snapshot.services.of_type(ty) {
                let state = load(&store, kind, service.id)?;
                services.push(Service {
                    id: service.id,
                    name: service.name.clone(),
                    local: ty == ServiceType::LocalTag,
                    original: state.clone(),
                    state,
                    left: BTreeSet::new(),
                    right: BTreeSet::new(),
                    workspace: BTreeSet::new(),
                    show_all: false,
                    show_pending: false,
                    whole_chain: kind == RelationKind::Siblings,
                    selected: BTreeSet::new(),
                    anchor: None,
                });
            }
        }
        if services.is_empty() {
            return Err(hydrus_store::StoreError::Invalid(
                "there are no editable tag services".into(),
            ));
        }
        Ok(Self {
            store,
            kind,
            services,
            service: 0,
            sort: 2,
            ascending: true,
        })
    }

    /// Store whose graph and tag presentation are being edited.
    pub fn store(&self) -> &Arc<Store> {
        &self.store
    }
    /// The kind of relationship being edited.
    pub fn kind(&self) -> RelationKind {
        self.kind
    }
    /// Service names in the same local-then-repository order as the reference.
    pub fn service_key(&self, index: usize) -> Option<hydrus_core::ServiceKey> {
        self.store
            .snapshot()
            .services
            .get(self.services.get(index)?.id)
            .ok()
            .map(|s| s.key.clone())
    }
    pub fn service_names(&self) -> Vec<String> {
        self.services.iter().map(|s| s.name.clone()).collect()
    }
    /// Index of the current service page.
    pub fn service(&self) -> usize {
        self.service
    }
    /// Switch pages while preserving their inputs, workspace and staged changes.
    pub fn choose_service(&mut self, index: usize) {
        if index < self.services.len() {
            self.service = index;
        }
    }
    /// Explain which services apply these relationships and when changes appear.
    pub fn sync_status(&self) -> hydrus_store::Result<String> {
        let source = &self.services[self.service];
        let snapshot = self.store.snapshot();
        let application = self
            .store
            .read(|conn| hydrus_store::display::load_application(conn, &snapshot.services))?;
        let mut names = snapshot
            .services
            .tag_services()
            .filter(|s| application.sources(self.kind, s.id).contains(&source.id))
            .map(|s| format!("\"{}\"", s.name))
            .collect::<Vec<_>>();
        names.sort();
        let noun = if self.kind == RelationKind::Siblings {
            "siblings"
        } else {
            "parents"
        };
        let mut status = if names.is_empty() {
            format!(
                "No services currently apply these {noun}. Changes here will have no effect unless {} application is changed later.",
                if self.kind == RelationKind::Siblings {
                    "sibling"
                } else {
                    "parent"
                }
            )
        } else {
            format!(
                "{} apply these {noun}. Changes stay here until Apply; tag displays update immediately after Apply.",
                names.join(", ")
            )
        };
        if !source.local {
            status.push_str(" Repository changes remain pending until uploaded.");
        }
        Ok(status)
    }
    /// The service's selected tags on either side of the input.
    pub fn inputs(&self) -> (Vec<String>, Vec<String>) {
        let s = &self.services[self.service];
        (
            s.left.iter().cloned().collect(),
            s.right.iter().cloned().collect(),
        )
    }
    /// Enter cleaned tags in a side's selection, toggling existing tags.
    /// A sibling has one ideal; a parent input may contain several parents.
    pub fn enter_tags(&mut self, right: bool, text: &str) -> Result<(), String> {
        self.enter_tag_text(right, text, false)
    }
    /// Clipboard entry only adds tags to the side's selection.
    pub fn paste_tags(&mut self, right: bool, tags: &[String]) -> Result<(), String> {
        self.enter_tag_text(right, &tags.join("\n"), true)
    }
    fn enter_tag_text(&mut self, right: bool, text: &str, only_add: bool) -> Result<(), String> {
        let tags: Vec<String> = text
            .lines()
            .filter(|s| !s.trim().is_empty())
            .map(|s| {
                Tag::new(s)
                    .map(|t| t.as_str().to_owned())
                    .ok_or_else(|| format!("\"{s}\" is not a valid tag"))
            })
            .collect::<Result<_, _>>()?;
        let s = &mut self.services[self.service];
        if right && self.kind == RelationKind::Siblings {
            s.right.clear();
        }
        for tag in tags
            .into_iter()
            .take(if right && self.kind == RelationKind::Siblings {
                1
            } else {
                usize::MAX
            })
        {
            s.workspace.insert(tag.clone());
            let (this, other) = if right {
                (&mut s.right, &mut s.left)
            } else {
                (&mut s.left, &mut s.right)
            };
            other.remove(&tag);
            if only_add || !this.remove(&tag) {
                this.insert(tag);
            }
        }
        Ok(())
    }
    /// Clear a tag from an input's preview.
    pub fn remove_input(&mut self, right: bool, tag: &str) {
        let s = &mut self.services[self.service];
        if right {
            s.right.remove(tag);
        } else {
            s.left.remove(tag);
        }
    }
    /// Whether both sides hold an uncommitted relationship.
    pub fn can_add(&self) -> bool {
        let s = &self.services[self.service];
        !s.left.is_empty() && !s.right.is_empty()
    }
    /// Add every selected left/right combination after answering any questions.
    pub fn add(&mut self, answers: &[Option<String>]) -> Result<(), Question> {
        let s = &self.services[self.service];
        let pairs = s
            .left
            .iter()
            .flat_map(|a| s.right.iter().map(move |b| (a.clone(), b.clone())))
            .collect();
        self.enter_pairs(pairs, true, answers)?;
        let s = &mut self.services[self.service];
        s.left.clear();
        s.right.clear();
        Ok(())
    }
    /// Import alternating lines of left/right tags, ignoring existing pairs.
    /// Returns whether an unmatched final line was ignored, as the reference does.
    pub fn import(&mut self, text: &str, answers: &[Option<String>]) -> Result<bool, Question> {
        let lines: Vec<_> = text.lines().filter(|l| !l.trim().is_empty()).collect();
        let pairs = lines
            .chunks_exact(2)
            .filter_map(|c| {
                Some((
                    Tag::new(c[0])?.as_str().to_owned(),
                    Tag::new(c[1])?.as_str().to_owned(),
                ))
            })
            .collect();
        self.enter_pairs(pairs, true, answers)?;
        Ok(lines.len() % 2 == 1)
    }
    /// Toggle the selected relationships (delete, rescind pend, or rescind petition).
    pub fn delete(&mut self, answers: &[Option<String>]) -> Result<(), Question> {
        let pairs = self.services[self.service]
            .selected
            .iter()
            .cloned()
            .collect();
        self.enter_pairs(pairs, false, answers)
    }
    /// Stage pairs, automatically removing sibling conflicts and final links
    /// that would close a cycle. Answers are replayable for asynchronous dialogs.
    pub fn enter_pairs(
        &mut self,
        pairs: Vec<Pair>,
        only_add: bool,
        answers: &[Option<String>],
    ) -> Result<(), Question> {
        let pairs = pairs
            .into_iter()
            .filter_map(|(a, b)| {
                Some((
                    Tag::new(&a)?.as_str().to_owned(),
                    Tag::new(&b)?.as_str().to_owned(),
                ))
            })
            .collect::<Vec<_>>();
        let mut service = self.services[self.service].clone();
        let mut answer = Answers {
            values: answers.iter(),
            replacement: false,
        };
        // Invalid self-pairs cannot replace an existing valid relationship.
        let mut cleaned = Vec::new();
        for pair in pairs {
            if pair.0 == pair.1 {
                answer.ask(Question {
                    message: format!("Cannot add self-referencing relationship {}->{}. A tag cannot replace or parent itself.", pair.0, pair.1),
                    yes: "OK".into(), no: "cancel".into(), reason: false,
                })?;
            } else {
                cleaned.push(pair);
            }
        }
        let pairs = cleaned;
        if self.kind == RelationKind::Siblings {
            let conflicts = service
                .state
                .effective()
                .into_iter()
                .filter(|p| pairs.iter().any(|new| new.0 == p.0 && new.1 != p.1))
                .collect::<Vec<_>>();
            toggle_group(
                &mut service,
                &conflicts,
                false,
                true,
                Some("TO BE AUTO-PETITIONED"),
                &mut answer,
            )?;
        }
        for pair in &pairs {
            service.workspace.extend([pair.0.clone(), pair.1.clone()]);
            let active = service.state.effective();
            // Repeal the final link of every path back to the new left tag.
            if !active.contains(pair) {
                let mut next = vec![pair.1.clone()];
                let mut seen = BTreeSet::new();
                while let Some(tag) = next.pop() {
                    if !seen.insert(tag.clone()) {
                        continue;
                    }
                    let active = service.state.effective();
                    for link in active.iter().filter(|p| p.0 == tag) {
                        if link.1 == pair.0 {
                            toggle(
                                &mut service,
                                link,
                                false,
                                true,
                                Some("TO BE AUTO-PETITIONED"),
                                &mut answer,
                            )?;
                        } else {
                            next.push(link.1.clone());
                        }
                    }
                }
            }
        }
        let mut ordered = pairs;
        ordered.sort_by_key(|p| hydrus_core::sort::human_sort_key(&p.1));
        toggle_group(&mut service, &ordered, only_add, false, None, &mut answer)?;
        let effective = service.state.effective();
        if !effective.is_subset(&self.services[self.service].state.effective())
            && invalid_graph(&effective, self.kind)
        {
            answer.ask(Question {
                message: "The relationships include a cycle or conflicting sibling ideals. No changes have been staged. Enter these pairs separately to resolve the links.".into(),
                yes: "OK".into(), no: "cancel".into(), reason: false,
            })?;
            return Ok(());
        }
        self.services[self.service] = service;
        Ok(())
    }
    /// List filters on the chosen service.
    pub fn filters(&self) -> (bool, bool, bool) {
        let s = &self.services[self.service];
        (s.show_all, s.show_pending, s.whole_chain)
    }
    /// Choose show-all, pending/petitioned groups, and whole-chain filtering.
    pub fn set_filters(&mut self, all: bool, pending: bool, whole: bool) {
        let s = &mut self.services[self.service];
        s.show_all = all;
        s.show_pending = pending;
        s.whole_chain = whole || self.kind == RelationKind::Siblings;
    }
    /// Forget old workspace tags, retaining currently entered tags.
    pub fn wipe_workspace(&mut self) {
        let s = &mut self.services[self.service];
        s.workspace = s.left.union(&s.right).cloned().collect();
    }
    /// The current workspace, for its tooltip.
    pub fn workspace(&self) -> Vec<String> {
        self.services[self.service]
            .workspace
            .iter()
            .cloned()
            .collect()
    }
    /// The selected sort column and direction.
    pub fn sorting(&self) -> (usize, bool) {
        (self.sort, self.ascending)
    }
    /// Sort the relationship list by a column, keeping its selection.
    pub fn sort(&mut self, column: usize, ascending: bool) {
        self.sort = column.min(3);
        self.ascending = ascending;
    }
    /// Rows sorted by the chosen column, with remembered workspace filters.
    pub fn rows(&self) -> Vec<Row> {
        let s = &self.services[self.service];
        let listed = s.state.listed();
        let mut tags = s.workspace.clone();
        if s.show_pending && !s.show_all {
            for p in s.state.pending.union(&s.state.petitioned) {
                tags.extend([p.0.clone(), p.1.clone()]);
            }
        }
        let visible = pertinent(&listed, &tags, s.show_all, s.whole_chain);
        let mut rows: Vec<_> = visible
            .into_iter()
            .map(|pair| {
                let pending = s.state.pending.contains(&pair);
                let petitioned = s.state.petitioned.contains(&pair);
                let mut note = s
                    .state
                    .reasons
                    .get(&pair)
                    .map_or_else(String::new, |r| format!("Reason: {r}"));
                if !pending && !petitioned {
                    note.clear();
                }
                if self.kind == RelationKind::Siblings && s.left.contains(&pair.0) {
                    note = if s.right.contains(&pair.1) {
                        "Already exists.".into()
                    } else if !petitioned {
                        format!(
                            "{}CONFLICT: {} be auto-{} on add.",
                            if s.right.is_empty() { "POSSIBLE " } else { "" },
                            if s.right.is_empty() { "May" } else { "Will" },
                            if pending {
                                "rescinded"
                            } else {
                                "petitioned/deleted"
                            }
                        )
                    } else {
                        note
                    };
                }
                Row {
                    selected: s.selected.contains(&pair),
                    pair,
                    status: if pending {
                        "(+) "
                    } else if petitioned {
                        "(-) "
                    } else {
                        ""
                    }
                    .into(),
                    note,
                }
            })
            .collect();
        rows.sort_by(|a, b| {
            let key = |r: &Row| match self.sort {
                0 => r.status.clone(),
                2 => r.pair.1.clone(),
                3 => r.note.clone(),
                _ => r.pair.0.clone(),
            };
            let order = hydrus_core::sort::human_sort_key(&key(a))
                .cmp(&hydrus_core::sort::human_sort_key(&key(b)))
                .then(a.pair.cmp(&b.pair));
            if self.ascending {
                order
            } else {
                order.reverse()
            }
        });
        rows
    }
    /// Apply Qt extended selection to a displayed row.
    pub fn click(&mut self, row: usize, ctrl: bool, shift: bool) {
        let rows = self.rows();
        let Some(clicked) = rows.get(row).map(|r| r.pair.clone()) else {
            return;
        };
        let s = &mut self.services[self.service];
        let anchor = s
            .anchor
            .as_ref()
            .and_then(|a| rows.iter().position(|r| &r.pair == a))
            .filter(|_| shift);
        if !ctrl {
            s.selected.clear();
        }
        if let Some(anchor) = anchor {
            s.selected.extend(
                rows[anchor.min(row)..=anchor.max(row)]
                    .iter()
                    .map(|r| r.pair.clone()),
            );
        } else {
            if !s.selected.remove(&clicked) {
                s.selected.insert(clicked.clone());
            }
            s.anchor = Some(clicked);
        }
    }
    /// Whether any visible relationship is selected.
    pub fn has_selection(&self) -> bool {
        self.rows().iter().any(|r| r.selected)
    }
    /// Export selected pairs as alternating lines, in displayed order.
    pub fn export(&self) -> String {
        self.rows()
            .into_iter()
            .filter(|r| r.selected)
            .flat_map(|r| [r.pair.0, r.pair.1])
            .collect::<Vec<_>>()
            .join("\n")
    }
    /// The exact OK question if the current input contains a complete pair.
    pub fn apply_question(&self) -> Option<Question> {
        self.can_add().then(|| Question {
            message: "Are you sure you want to OK? You have an uncommitted pair.".into(),
            yes: "yes".into(),
            no: "no".into(),
            reason: false,
        })
    }
    /// Changes to be applied, by service and content action.
    pub fn updates(&self) -> Vec<RelationUpdate> {
        let mut updates = Vec::new();
        for s in &self.services {
            let mut push = |pair: &Pair, action| {
                updates.push(RelationUpdate {
                    service: s.id,
                    left: Tag::new(&pair.0).expect("cleaned input"),
                    right: Tag::new(&pair.1).expect("cleaned input"),
                    action,
                });
            };
            if s.local {
                for p in &s.state.petitioned {
                    push(p, RelationAction::Delete);
                }
                for p in &s.state.pending {
                    push(p, RelationAction::Add);
                }
            } else {
                for p in s.original.pending.difference(&s.state.pending) {
                    push(p, RelationAction::RescindPend);
                }
                for p in s.original.petitioned.difference(&s.state.petitioned) {
                    push(p, RelationAction::RescindPetition);
                }
                for p in s.state.petitioned.difference(&s.original.petitioned) {
                    push(
                        p,
                        RelationAction::Petition(
                            s.state.reasons.get(p).cloned().unwrap_or_default(),
                        ),
                    );
                }
                for p in s.state.pending.difference(&s.original.pending) {
                    push(
                        p,
                        RelationAction::Pend(s.state.reasons.get(p).cloned().unwrap_or_default()),
                    );
                }
            }
        }
        updates
    }
    /// Write all staged service pages atomically. Cancel simply drops the editor.
    pub fn apply(&self) -> hydrus_store::Result<()> {
        tag_relations::apply(&self.store, self.kind, self.updates())
    }
}

struct Answers<'a> {
    values: std::slice::Iter<'a, Option<String>>,
    replacement: bool,
}
impl Answers<'_> {
    fn ask(&mut self, q: Question) -> Result<Option<String>, Question> {
        self.values.next().cloned().ok_or(q)
    }
}

fn toggle(
    service: &mut Service,
    pair: &Pair,
    only_add: bool,
    only_remove: bool,
    forced: Option<&str>,
    answers: &mut Answers<'_>,
) -> Result<(), Question> {
    toggle_group(
        service,
        std::slice::from_ref(pair),
        only_add,
        only_remove,
        forced,
        answers,
    )
}

fn pair_strings(pairs: &[Pair], comma: bool) -> String {
    if pairs.len() > 10 {
        return "The many pairs you entered.".into();
    }
    pairs
        .iter()
        .map(|p| format!("{}->{}", p.0, p.1))
        .collect::<Vec<_>>()
        .join(if comma { ", " } else { "\n" })
}

fn toggle_group(
    s: &mut Service,
    pairs: &[Pair],
    only_add: bool,
    only_remove: bool,
    forced: Option<&str>,
    answers: &mut Answers<'_>,
) -> Result<(), Question> {
    let mut pend = Vec::new();
    let mut petition = Vec::new();
    let mut rescind_pend = Vec::new();
    let mut rescind_petition = Vec::new();
    for pair in pairs.iter().filter(|p| p.0 != p.1) {
        if s.state.pending.contains(pair) {
            if !only_add {
                rescind_pend.push(pair.clone());
            }
        } else if s.state.petitioned.contains(pair) {
            if !only_remove {
                rescind_petition.push(pair.clone());
            }
        } else if s.original.current.contains(pair) {
            if !only_add {
                petition.push(pair.clone());
            }
        } else if !only_remove {
            pend.push(pair.clone());
        }
    }
    for (removing, group) in [(false, pend), (true, petition)] {
        if group.is_empty() {
            continue;
        }
        let reason = if let Some(reason) = forced {
            Some(reason.to_owned())
        } else if s.local {
            Some(
                if removing {
                    "removed by user"
                } else {
                    "added by user"
                }
                .into(),
            )
        } else {
            answers.ask(Question {
                message: format!("Enter a reason for:\n\n{}\n\n{}", pair_strings(&group, false), if removing { "to be removed. You will see the delete as soon as you upload, but a janitor will review your petition to decide if all users should receive it as well." } else { "To be added. A janitor will review your petition." }),
                yes: "OK".into(), no: "cancel".into(), reason: true,
            })?
        };
        if let Some(mut reason) = reason {
            if reason == "TO BE AUTO-PETITIONED" {
                answers.replacement = true;
            } else if answers.replacement
                || s.state
                    .reasons
                    .values()
                    .any(|r| r == "TO BE AUTO-PETITIONED")
            {
                reason = if s.local {
                    "REPLACEMENT: by user".into()
                } else {
                    format!("REPLACEMENT: {reason}")
                };
                for r in s.state.reasons.values_mut() {
                    if r == "TO BE AUTO-PETITIONED" {
                        r.clone_from(&reason);
                    }
                }
            }
            for pair in group {
                s.state.reasons.insert(pair.clone(), reason.clone());
                if removing {
                    s.state.petitioned.insert(pair);
                } else {
                    s.state.pending.insert(pair);
                }
            }
        }
    }
    for (pending, group) in [(true, rescind_pend), (false, rescind_petition)] {
        if group.is_empty() {
            continue;
        }
        let state = if pending { "pending" } else { "petitioned" };
        let message = if group.len() == 1 {
            format!("The pair {} is {state}.", pair_strings(&group, false))
        } else {
            format!(
                "The pairs:\n\n{}\n\nAre {state}.",
                pair_strings(&group, !pending)
            )
        };
        let accepted = answers.ask(Question {
            message,
            yes: if pending {
                "rescind the pend"
            } else {
                "rescind the petition"
            }
            .into(),
            no: "do nothing".into(),
            reason: false,
        })?;
        if accepted.is_some() {
            let set = if pending {
                &mut s.state.pending
            } else {
                &mut s.state.petitioned
            };
            for pair in group {
                set.remove(&pair);
            }
        }
    }
    Ok(())
}

fn pertinent(
    pairs: &BTreeSet<Pair>,
    tags: &BTreeSet<String>,
    all: bool,
    whole: bool,
) -> BTreeSet<Pair> {
    if all {
        return pairs.clone();
    }
    let mut left = tags.clone();
    let mut right = tags.clone();
    let mut seen = BTreeSet::new();
    let mut out = BTreeSet::new();
    while !left.is_empty() || !right.is_empty() {
        seen.extend(left.iter().chain(&right).cloned());
        let mut next_left = BTreeSet::new();
        let mut next_right = BTreeSet::new();
        for pair in pairs {
            if right.contains(&pair.0)
                || left.contains(&pair.1)
                || (whole && (left.contains(&pair.0) || right.contains(&pair.1)))
            {
                out.insert(pair.clone());
                if (whole || left.contains(&pair.1)) && !seen.contains(&pair.0) {
                    next_left.insert(pair.0.clone());
                }
                if (whole || right.contains(&pair.0)) && !seen.contains(&pair.1) {
                    next_right.insert(pair.1.clone());
                }
            }
        }
        left = next_left;
        right = next_right;
    }
    out
}

fn invalid_graph(pairs: &BTreeSet<Pair>, kind: RelationKind) -> bool {
    let mut links: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
    let mut incoming: BTreeMap<&str, usize> = BTreeMap::new();
    for (a, b) in pairs {
        links.entry(a).or_default().push(b);
        incoming.entry(a).or_default();
        *incoming.entry(b).or_default() += 1;
    }
    if kind == RelationKind::Siblings && links.values().any(|v| v.len() > 1) {
        return true;
    }
    let mut ready = incoming
        .iter()
        .filter(|(_, n)| **n == 0)
        .map(|(t, _)| *t)
        .collect::<Vec<_>>();
    let mut visited = 0;
    while let Some(tag) = ready.pop() {
        visited += 1;
        for next in links.get(tag).into_iter().flatten() {
            let count = incoming
                .get_mut(next)
                .expect("every link target was counted");
            *count -= 1;
            if *count == 0 {
                ready.push(*next);
            }
        }
    }
    visited != incoming.len()
}

fn load(store: &Store, kind: RelationKind, service: ServiceId) -> hydrus_store::Result<State> {
    store.read(|conn| {
        let (table, left, right) = tag_relations::columns(kind);
        let mut stmt = conn.prepare(&format!(
            "SELECT status,{left},{right} FROM {table} WHERE service_id=?1 AND status != ?2"
        ))?;
        let rows = stmt
            .query_map(
                rusqlite::params![service, ContentStatus::Deleted.code()],
                |r| {
                    Ok((
                        r.get::<_, u8>(0)?,
                        r.get::<_, TagId>(1)?,
                        r.get::<_, TagId>(2)?,
                    ))
                },
            )?
            .collect::<Result<Vec<_>, _>>()?;
        let ids = rows
            .iter()
            .flat_map(|(_, a, b)| [*a, *b])
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect::<Vec<_>>();
        let tags = hydrus_store::master::tags(conn, &ids)?;
        let mut state = State::default();
        for (status, a, b) in rows {
            let pair = (tags[&a].as_str().to_owned(), tags[&b].as_str().to_owned());
            match ContentStatus::from_code(status) {
                Some(ContentStatus::Current) => {
                    state.current.insert(pair);
                }
                Some(ContentStatus::Pending) => {
                    state.pending.insert(pair);
                }
                Some(ContentStatus::Petitioned) => {
                    state.petitioned.insert(pair);
                }
                _ => {}
            }
        }
        // The reference displays reasons supplied during this edit, rather
        // than loading old proposal reasons. Persisted reasons remain intact.
        Ok(state)
    })
}

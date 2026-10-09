//! The duplicates auto-resolution rules editor, bound
//! (`ui/auto_resolution_rules.slint`): the "edit rules" list (add, add
//! suggested, edit, delete; "apply" writes the rules), a rule's editor
//! (its search, comparators and action), and the comparator editors, an
//! OR's or AND's opening another for each of its comparators. What they
//! say is hydrus-gui-model's [`auto_resolution_rules`]
//! (crate::auto_resolution_rules).

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::rc::Rc;
use std::sync::Arc;

use slint::{ComponentHandle as _, ModelRc, SharedString, VecModel};

use hydrus_core::search::number::NumberOp;
use hydrus_search::TextContext;
use hydrus_store::Store;
use hydrus_store::duplicates::auto::{self as store_rules, Comparator, Rule, RuleAction};

use crate::auto_resolution_rules::{
    ACTION_CHOICES, ACTION_TEXT, CANNOT_DETERMINE_BETTER, COMPARISON_TEXT, ONE_FILE_TESTS,
    OPERATION_CHOICES, PAIR_SEARCH_CHOICES, PIXEL_CHOICES, PROPERTIES, RULES_ORDER_NOTE,
    RULES_WARNING, RuleEdit, SEARCH_TEXT, VISUAL_CHOICES, action_text, comparator_choices,
    comparator_summary, looking_choices, new_rule, operator_choices, operator_label,
    parse_predicate_lines, predicate_lines, property_text, row, rule_can_determine_better,
    same_operator, visual_text,
};
use crate::folders::Named;
use crate::{AutoResolutionRuleWindow, AutoResolutionRulesWindow, ComparatorWindow, TableRow};

/// The comparator editors open, innermost last, each with its own number.
pub type ComparatorStack = Rc<RefCell<Vec<(u64, ComparatorWindow)>>>;

/// The windows while they are open.
#[derive(Clone, Default)]
pub struct Slots {
    pub list: Rc<RefCell<Option<AutoResolutionRulesWindow>>>,
    pub rule: Rc<RefCell<Option<AutoResolutionRuleWindow>>>,
    /// The comparator editors open, innermost last, each with its own
    /// number.
    pub comparators: ComparatorStack,
    /// A rule's custom merge options' editor.
    pub merge_options: crate::merge_options_window::Slot,
    /// The rule's searches' location list.
    pub locations: Rc<RefCell<Option<crate::LocationsWindow>>>,
    /// The duplicate filter opened from the preview's lists.
    pub preview_filter: Rc<RefCell<Option<crate::DuplicateFilterWindow>>>,
    /// Opens a page of files (the preview lists' "show in a new page").
    pub open_files: Rc<RefCell<Option<crate::auto_resolution_review_window::OpenFiles>>>,
    /// The rules list's png export.
    pub png: crate::png_export_window::Slots,
}

impl std::fmt::Debug for Slots {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Slots")
            .field("list", &self.list.borrow().is_some())
            .field("rule", &self.rule.borrow().is_some())
            .field("png", &self.png.has_open())
            .field("comparators", &self.comparators.borrow().len())
            .field("merge_options", &self.merge_options.borrow().is_some())
            .field("locations", &self.locations.borrow().is_some())
            .field("preview_filter", &self.preview_filter.borrow().is_some())
            .field("open_files", &self.open_files.borrow().is_some())
            .finish()
    }
}

fn strings(items: Vec<String>) -> ModelRc<SharedString> {
    let items: Vec<SharedString> = items.into_iter().map(Into::into).collect();
    ModelRc::new(VecModel::from(items))
}

fn text_context(store: &Store) -> TextContext {
    let snapshot = store.snapshot();
    let viewing = store.read(hydrus_store::settings::get).unwrap_or_default();
    TextContext::from_store(&snapshot.services, &viewing)
}

fn index(i: usize) -> i32 {
    i32::try_from(i).unwrap_or(-1)
}

fn kind_labels() -> Vec<String> {
    comparator_choices().into_iter().map(|c| c.0).collect()
}

// the comparator editors ----------------------------------------------------

/// Edit a comparator in its editor (a pair test has none: it comes back
/// as it is); "apply" gives it to `done`.
fn edit_comparator(
    store: &Arc<Store>,
    comparator: Comparator,
    stack: &ComparatorStack,
    done: Rc<dyn Fn(Comparator)>,
) {
    if matches!(comparator, Comparator::Pair(_)) {
        done(comparator);
        return;
    }
    let number = stack.borrow().last().map_or(0, |(n, _)| n + 1);
    match open_comparator(store, comparator, number, stack, done) {
        Ok(window) => stack.borrow_mut().push((number, window)),
        Err(e) => eprintln!("could not open the comparator editor: {e}"),
    }
}

struct ComparatorState {
    comparator: Comparator,
    context: TextContext,
    errors: Vec<String>,
    /// An OR's or AND's selected comparator.
    selected: Option<usize>,
}

fn subs(comparator: &mut Comparator) -> Option<&mut Vec<Comparator>> {
    match comparator {
        Comparator::Or(subs) | Comparator::And(subs) => Some(subs),
        _ => None,
    }
}

/// Show the editor's fields from the comparator (all, as it opens or
/// changes kind of field).
fn show_comparator(window: &ComparatorWindow, state: &mut ComparatorState) {
    let context = &state.context;
    match &state.comparator {
        Comparator::OneFileMetadata {
            looking_at,
            predicates,
        } => {
            let choices = looking_choices(true);
            window.set_looking_choices(strings(choices.iter().map(|c| c.0.to_owned()).collect()));
            window.set_looking(index(
                choices.iter().position(|c| c.1 == *looking_at).unwrap_or(0),
            ));
            if window.get_predicates().is_empty() {
                window.set_predicates(predicate_lines(predicates, context).into());
            }
        }
        Comparator::OneFileHardcoded { looking_at, test } => {
            let choices = looking_choices(false);
            window.set_looking_choices(strings(choices.iter().map(|c| c.0.to_owned()).collect()));
            window.set_looking(index(
                choices.iter().position(|c| c.1 == *looking_at).unwrap_or(0),
            ));
            window.set_test_choices(strings(
                ONE_FILE_TESTS.iter().map(|t| t.0.to_owned()).collect(),
            ));
            window.set_test(index(
                ONE_FILE_TESTS
                    .iter()
                    .position(|t| t.1 == *test)
                    .unwrap_or(0),
            ));
        }
        Comparator::RelativeFileInfo {
            property,
            test,
            multiplier,
            delta,
        } => {
            window.set_property_choices(strings(
                PROPERTIES
                    .iter()
                    .map(|p| property_text(*p).to_owned())
                    .collect(),
            ));
            window.set_property(index(
                PROPERTIES.iter().position(|p| p == property).unwrap_or(0),
            ));
            let ops = operator_choices(*property);
            window.set_operator_choices(strings(
                ops.iter().map(|o| operator_label(*o, *property)).collect(),
            ));
            window.set_operator(index(
                ops.iter()
                    .position(|o| same_operator(*o, test.op))
                    .unwrap_or(0),
            ));
            let time = is_time(*property);
            let (approximate, label, range) = match test.op {
                NumberOp::ApproxPercent { percent } => {
                    (true, "\u{b1} percent: ", i64::from(percent))
                }
                NumberOp::ApproxAbsolute { tolerance } => (
                    true,
                    if time {
                        "\u{b1} milliseconds: "
                    } else {
                        "\u{b1} range: "
                    },
                    i64::try_from(tolerance).unwrap_or(i64::MAX),
                ),
                _ => (false, "", 0),
            };
            window.set_approximate(approximate);
            window.set_range_label(label.into());
            window.set_range(i32::try_from(range).unwrap_or(i32::MAX));
            window.set_multiplier(format!("{multiplier:.2}").into());
            window.set_delta_label(
                if time {
                    "delta in milliseconds (optional): "
                } else {
                    "delta (optional): "
                }
                .into(),
            );
            window.set_delta(i32::try_from(*delta).unwrap_or(0));
            window.set_warning(
                if *property == hydrus_core::search::comparable::Comparable::Framerate {
                    "Framerate is an approximation (e.g. it might be calculated as 30.05fps for one file but 29.98fps for another), so be careful with < and > here. Best to try for some padding, like \"A framerate > 1.1x B\"."
                } else {
                    ""
                }
                .into(),
            );
        }
        Comparator::VisualDuplicates { confidence } => {
            window.set_visual_choices(strings(
                VISUAL_CHOICES
                    .iter()
                    .map(|c| visual_text(*c).to_owned())
                    .collect(),
            ));
            window.set_visual(index(
                VISUAL_CHOICES
                    .iter()
                    .position(|c| c == confidence)
                    .unwrap_or(1),
            ));
        }
        Comparator::Or(list) | Comparator::And(list) => {
            window.set_subs(strings(
                list.iter()
                    .map(|c| comparator_summary(c, context))
                    .collect(),
            ));
            window.set_sub_selected(state.selected.map_or(-1, index));
            window.set_sub_kinds(strings(kind_labels()));
        }
        Comparator::Pair(_) => {}
    }
    show_comparator_summary(window, state);
}

fn show_comparator_summary(window: &ComparatorWindow, state: &ComparatorState) {
    window.set_summary(comparator_summary(&state.comparator, &state.context).into());
    window.set_errors(state.errors.join("\n").into());
}

fn is_time(property: hydrus_core::search::comparable::Comparable) -> bool {
    use hydrus_core::search::comparable::Comparable as C;
    matches!(
        property,
        C::ImportTime | C::ModifiedTime | C::LastViewedTime | C::ArchivedTime | C::Duration
    )
}

/// Read the editor's fields into the comparator; whether its fields need
/// showing again (a property changed, and with it the operators).
fn read_comparator(window: &ComparatorWindow, state: &mut ComparatorState) -> bool {
    state.errors.clear();
    let mut reshow = false;
    match &mut state.comparator {
        Comparator::OneFileMetadata {
            looking_at,
            predicates,
        } => {
            if let Some(c) = usize::try_from(window.get_looking())
                .ok()
                .and_then(|i| looking_choices(true).get(i).copied())
            {
                *looking_at = c.1;
            }
            match parse_predicate_lines(&window.get_predicates()) {
                Ok(p) => *predicates = p,
                Err(e) => state.errors.push(e),
            }
        }
        Comparator::OneFileHardcoded { looking_at, test } => {
            if let Some(c) = usize::try_from(window.get_looking())
                .ok()
                .and_then(|i| looking_choices(false).get(i).copied())
            {
                *looking_at = c.1;
            }
            if let Some(t) = usize::try_from(window.get_test())
                .ok()
                .and_then(|i| ONE_FILE_TESTS.get(i))
            {
                *test = t.1;
            }
        }
        Comparator::RelativeFileInfo {
            property,
            test,
            multiplier,
            delta,
        } => {
            let chosen = usize::try_from(window.get_property())
                .ok()
                .and_then(|i| PROPERTIES.get(i).copied())
                .unwrap_or(*property);
            if chosen == *property {
                let ops = operator_choices(*property);
                if let Some(op) = usize::try_from(window.get_operator())
                    .ok()
                    .and_then(|i| ops.get(i).copied())
                {
                    let range = window.get_range().max(0);
                    test.op = match op {
                        NumberOp::ApproxPercent { percent } => NumberOp::ApproxPercent {
                            percent: if same_operator(op, test.op) {
                                u32::try_from(range).unwrap_or(percent)
                            } else {
                                percent
                            },
                        },
                        NumberOp::ApproxAbsolute { tolerance } => NumberOp::ApproxAbsolute {
                            tolerance: if same_operator(op, test.op) {
                                u64::try_from(range).unwrap_or(tolerance)
                            } else {
                                tolerance
                            },
                        },
                        other => other,
                    };
                    reshow = !same_operator(op, test.op)
                        || matches!(
                            op,
                            NumberOp::ApproxPercent { .. } | NumberOp::ApproxAbsolute { .. }
                        );
                }
                match window.get_multiplier().trim().parse::<f64>() {
                    Ok(m) => *multiplier = m,
                    Err(_) => state.errors.push("The multiplier is not a number.".into()),
                }
                *delta = i64::from(window.get_delta());
            } else {
                // (a new property: its first operator)
                *property = chosen;
                test.op = operator_choices(chosen)[0];
                reshow = true;
            }
        }
        Comparator::VisualDuplicates { confidence } => {
            if let Some(c) = usize::try_from(window.get_visual())
                .ok()
                .and_then(|i| VISUAL_CHOICES.get(i))
            {
                *confidence = *c;
            }
        }
        Comparator::Or(_) | Comparator::And(_) | Comparator::Pair(_) => {}
    }
    reshow
}

fn comparator_title(comparator: &Comparator) -> &'static str {
    match comparator {
        Comparator::OneFileMetadata { .. } => "edit one-file metadata conditional comparator",
        Comparator::OneFileHardcoded { .. } => "edit one-file hardcoded comparator",
        Comparator::RelativeFileInfo { .. } => "edit relative comparator",
        Comparator::VisualDuplicates { .. } => "edit visual duplicates comparator",
        Comparator::Or(_) => "edit OR comparator",
        Comparator::And(_) => "edit AND comparator",
        Comparator::Pair(_) => "",
    }
}

fn comparator_kind(comparator: &Comparator) -> &'static str {
    match comparator {
        Comparator::OneFileMetadata { .. } => "search",
        Comparator::OneFileHardcoded { .. } => "hardcoded",
        Comparator::RelativeFileInfo { .. } => "relative",
        Comparator::VisualDuplicates { .. } => "visual",
        Comparator::Or(_) | Comparator::And(_) => "list",
        Comparator::Pair(_) => "",
    }
}

fn open_comparator(
    store: &Arc<Store>,
    comparator: Comparator,
    number: u64,
    stack: &ComparatorStack,
    done: Rc<dyn Fn(Comparator)>,
) -> Result<ComparatorWindow, String> {
    let window = crate::app_title::new::<crate::ComparatorWindow>().map_err(|e| e.to_string())?;
    window.set_window_title(comparator_title(&comparator).into());
    window.set_kind(comparator_kind(&comparator).into());
    let state = Rc::new(RefCell::new(ComparatorState {
        comparator,
        context: text_context(store),
        errors: Vec::new(),
        selected: None,
    }));
    let close = {
        let weak = window.as_weak();
        let stack = stack.clone();
        move || {
            if let Some(window) = weak.upgrade() {
                let _ = window.hide();
            }
            stack.borrow_mut().retain(|(n, _)| *n != number);
        }
    };
    window.on_changed({
        let weak = window.as_weak();
        let state = state.clone();
        move || {
            let Some(window) = weak.upgrade() else {
                return;
            };
            let mut state = state.borrow_mut();
            if read_comparator(&window, &mut state) {
                show_comparator(&window, &mut state);
            } else {
                show_comparator_summary(&window, &state);
            }
        }
    });
    window.on_sub_clicked({
        let weak = window.as_weak();
        let state = state.clone();
        move |i| {
            let Some(window) = weak.upgrade() else {
                return;
            };
            let mut state = state.borrow_mut();
            state.selected = usize::try_from(i).ok();
            show_comparator(&window, &mut state);
        }
    });
    // (the sub-comparators' own editors, each giving back into this one)
    let sub_done = {
        let weak = window.as_weak();
        let state = state.clone();
        move |at: Option<usize>| -> Rc<dyn Fn(Comparator)> {
            let weak = weak.clone();
            let state = state.clone();
            Rc::new(move |edited: Comparator| {
                let Some(window) = weak.upgrade() else {
                    return;
                };
                let mut state = state.borrow_mut();
                if let Some(list) = subs(&mut state.comparator) {
                    match at.filter(|&i| i < list.len()) {
                        Some(i) => list[i] = edited,
                        None => list.push(edited),
                    }
                }
                show_comparator(&window, &mut state);
            })
        }
    };
    let sub_done = Rc::new(sub_done);
    window.on_sub_add({
        let weak = window.as_weak();
        let store = store.clone();
        let stack = stack.clone();
        let sub_done = sub_done.clone();
        move || {
            let Some(window) = weak.upgrade() else {
                return;
            };
            let Some((_, _, comparator)) = usize::try_from(window.get_sub_kind())
                .ok()
                .and_then(|i| comparator_choices().into_iter().nth(i))
            else {
                return;
            };
            edit_comparator(&store, comparator, &stack, sub_done(None));
        }
    });
    window.on_sub_edit({
        let state = state.clone();
        let store = store.clone();
        let stack = stack.clone();
        let sub_done = sub_done.clone();
        move || {
            let chosen = {
                let mut state = state.borrow_mut();
                let at = state.selected;
                at.and_then(|i| {
                    subs(&mut state.comparator)
                        .and_then(|l| l.get(i).cloned())
                        .map(|c| (i, c))
                })
            };
            if let Some((i, comparator)) = chosen {
                edit_comparator(&store, comparator, &stack, sub_done(Some(i)));
            }
        }
    });
    window.on_sub_delete({
        let weak = window.as_weak();
        let state = state.clone();
        move || {
            let Some(window) = weak.upgrade() else {
                return;
            };
            let mut state = state.borrow_mut();
            let at = state.selected.take();
            if let (Some(i), Some(list)) = (at, subs(&mut state.comparator))
                && i < list.len()
            {
                list.remove(i);
            }
            show_comparator(&window, &mut state);
        }
    });
    window.on_apply({
        let weak = window.as_weak();
        let state = state.clone();
        let close = close.clone();
        move || {
            let Some(window) = weak.upgrade() else {
                return;
            };
            let comparator = {
                let mut state = state.borrow_mut();
                read_comparator(&window, &mut state);
                if !state.errors.is_empty() {
                    show_comparator_summary(&window, &state);
                    return;
                }
                state.comparator.clone()
            };
            close();
            done(comparator);
        }
    });
    window.on_cancel({
        let close = close.clone();
        move || close()
    });
    window.window().on_close_requested({
        let close = close.clone();
        move || {
            close();
            slint::CloseRequestResponse::HideWindow
        }
    });
    show_comparator(&window, &mut state.borrow_mut());
    window.show().map_err(|e| e.to_string())?;
    Ok(window)
}

// the rule editor -------------------------------------------------------------

struct RuleState {
    rule: Rule,
    context: TextContext,
    errors: Vec<String>,
    selected: Option<usize>,
    /// The decision the custom merge options are for: they start again
    /// from the client's when the action changes (`_UpdateActionControls`).
    merge_for: Option<hydrus_store::duplicates::PairRelationship>,
}

fn show_rule(window: &AutoResolutionRuleWindow, state: &RuleState, fields: bool) {
    let rule = &state.rule;
    let context = &state.context;
    let mut comparators: Vec<String> = rule
        .comparators
        .iter()
        .map(|c| comparator_summary(c, context))
        .collect();
    // (in the rule's order, as the reference's list keeps them)
    comparators.truncate(rule.comparators.len());
    window.set_comparators(strings(comparators));
    window.set_comparator_selected(state.selected.map_or(-1, index));
    window.set_errors(state.errors.join("\n").into());
    if !fields {
        return;
    }
    window.set_name(rule.name.as_str().into());
    window.set_paused(rule.paused);
    window.set_operation(index(
        OPERATION_CHOICES
            .iter()
            .position(|c| c.1 == rule.mode)
            .unwrap_or(0),
    ));
    window.set_no_pending_limit(rule.max_pending_pairs.is_none());
    window.set_pending_limit(
        rule.max_pending_pairs
            .map_or(512, |n| i32::try_from(n).unwrap_or(i32::MAX)),
    );
    let search = &rule.search;
    window.set_pair_search(index(
        PAIR_SEARCH_CHOICES
            .iter()
            .position(|c| c.1 == search.kind)
            .unwrap_or(0),
    ));
    window.set_search_1(predicate_lines(&search.search_1.predicates, context).into());
    window.set_search_2(predicate_lines(&search.search_2.predicates, context).into());
    window.set_pixel(index(
        PIXEL_CHOICES
            .iter()
            .position(|c| c.1 == search.pixel_duplicates)
            .unwrap_or(1),
    ));
    window.set_distance(i32::try_from(search.max_hamming_distance).unwrap_or(0));
    window.set_action(index(
        ACTION_CHOICES
            .iter()
            .position(|a| *a == rule.action)
            .unwrap_or(0),
    ));
    window.set_delete_a(rule.delete_a);
    window.set_delete_b(rule.delete_b);
    window.set_default_merge(rule.custom_merge.is_none());
}

fn relationship(action: RuleAction) -> hydrus_store::duplicates::PairRelationship {
    use hydrus_store::duplicates::PairRelationship as P;
    match action {
        RuleAction::FalsePositive => P::FalsePositive,
        RuleAction::SameQuality => P::SameQuality,
        RuleAction::Alternate => P::Alternate,
        RuleAction::Better | RuleAction::Worse => P::Better,
    }
}

fn read_rule(window: &AutoResolutionRuleWindow, store: &Store, state: &mut RuleState) {
    state.errors.clear();
    let rule = &mut state.rule;
    window.get_name().trim().clone_into(&mut rule.name);
    rule.paused = window.get_paused();
    if let Some(c) = usize::try_from(window.get_operation())
        .ok()
        .and_then(|i| OPERATION_CHOICES.get(i))
    {
        rule.mode = c.1;
    }
    rule.max_pending_pairs = (!window.get_no_pending_limit())
        .then(|| u32::try_from(window.get_pending_limit().max(1)).unwrap_or(1));
    if let Some(c) = usize::try_from(window.get_pair_search())
        .ok()
        .and_then(|i| PAIR_SEARCH_CHOICES.get(i))
    {
        rule.search.kind = c.1;
    }
    for (text, search, which) in [
        (
            window.get_search_1(),
            &mut rule.search.search_1,
            "the search",
        ),
        (
            window.get_search_2(),
            &mut rule.search.search_2,
            "the second search",
        ),
    ] {
        match parse_predicate_lines(&text) {
            Ok(predicates) => search.predicates = predicates,
            Err(e) => state.errors.push(format!("Problem with {which}: {e}")),
        }
    }
    if let Some(c) = usize::try_from(window.get_pixel())
        .ok()
        .and_then(|i| PIXEL_CHOICES.get(i))
    {
        rule.search.pixel_duplicates = c.1;
    }
    rule.search.max_hamming_distance = u32::try_from(window.get_distance().max(0)).unwrap_or(0);
    if let Some(a) = usize::try_from(window.get_action())
        .ok()
        .and_then(|i| ACTION_CHOICES.get(i))
    {
        rule.action = *a;
    }
    rule.delete_a = window.get_delete_a();
    rule.delete_b = window.get_delete_b();
    let decision = relationship(rule.action);
    if window.get_default_merge() {
        rule.custom_merge = None;
        state.merge_for = None;
    } else if rule.custom_merge.is_none() || state.merge_for != Some(decision) {
        // (the client's options for the action, to start from)
        let client: hydrus_store::duplicates::DuplicateMergeSettings =
            store.read(hydrus_store::settings::get).unwrap_or_default();
        rule.custom_merge = client.for_relationship(decision).cloned();
        state.merge_for = Some(decision);
    }
}

/// Edit a rule in its editor; "apply" (once it is good) gives it to
/// `done`.
fn open_rule(
    store: &Arc<Store>,
    rule: Rule,
    title: &str,
    slots: &Slots,
    done: Rc<dyn Fn(Rule)>,
) -> Result<AutoResolutionRuleWindow, String> {
    let window =
        crate::app_title::new::<crate::AutoResolutionRuleWindow>().map_err(|e| e.to_string())?;
    window.set_window_title(title.into());
    window.set_operation_choices(strings(
        OPERATION_CHOICES.iter().map(|c| c.0.to_owned()).collect(),
    ));
    window.set_search_text(SEARCH_TEXT.into());
    window.set_comparison_text(COMPARISON_TEXT.into());
    window.set_action_text(ACTION_TEXT.into());
    window.set_pair_search_choices(strings(
        PAIR_SEARCH_CHOICES.iter().map(|c| c.0.to_owned()).collect(),
    ));
    window.set_pixel_choices(strings(
        PIXEL_CHOICES.iter().map(|c| c.0.to_owned()).collect(),
    ));
    window.set_action_choices(strings(
        ACTION_CHOICES
            .iter()
            .map(|a| action_text(*a).to_owned())
            .collect(),
    ));
    window.set_comparator_kinds(strings(kind_labels()));
    window.set_location(
        crate::domains::location_label(&store.snapshot().services, &rule.search.search_1.location)
            .into(),
    );
    let merge_for = rule
        .custom_merge
        .is_some()
        .then(|| relationship(rule.action));
    let state = Rc::new(RefCell::new(RuleState {
        rule,
        context: text_context(store),
        errors: Vec::new(),
        selected: None,
        merge_for,
    }));
    // the pair count of the rule's search, as the reference's panel shows it
    let count =
        crate::rule_count::RuleCount::start(&window, store, state.borrow().rule.search.clone());
    window.on_count_action({
        let weak = window.as_weak();
        let count = count.clone();
        move |what, n| {
            if let Some(window) = weak.upgrade() {
                count.action(&window, &what, n);
            }
        }
    });
    let close = {
        let weak = window.as_weak();
        let slot = slots.rule.clone();
        move || {
            if let Some(window) = weak.upgrade() {
                let _ = window.hide();
            }
            slot.borrow_mut().take();
        }
    };
    window.on_changed({
        let weak = window.as_weak();
        let state = state.clone();
        let store = store.clone();
        let count = count.clone();
        move || {
            let Some(window) = weak.upgrade() else {
                return;
            };
            let mut state = state.borrow_mut();
            read_rule(&window, &store, &mut state);
            show_rule(&window, &state, false);
            count.search_changed(&state.rule.search);
        }
    });
    // the searches' location, chosen in the locations list
    window.on_edit_location({
        let weak = window.as_weak();
        let state = state.clone();
        let store = store.clone();
        let count = count.clone();
        let slot = slots.locations.clone();
        move || {
            let Some(window) = weak.upgrade() else {
                return;
            };
            let current = {
                let mut state = state.borrow_mut();
                read_rule(&window, &store, &mut state);
                state.rule.search.search_1.location.clone()
            };
            let chosen: Rc<dyn Fn(hydrus_search::LocationContext)> = {
                let weak = window.as_weak();
                let state = state.clone();
                let store = store.clone();
                let count = count.clone();
                Rc::new(move |location| {
                    let Some(window) = weak.upgrade() else {
                        return;
                    };
                    window.set_location(
                        crate::domains::location_label(&store.snapshot().services, &location)
                            .into(),
                    );
                    let mut state = state.borrow_mut();
                    // (both searches search the one location)
                    state.rule.search.search_2.location = location.clone();
                    state.rule.search.search_1.location = location;
                    count.search_changed(&state.rule.search);
                })
            };
            if let Err(e) = crate::locations_window::open(&slot, store.clone(), &current, chosen) {
                eprintln!("could not open the locations list: {e}");
            }
        }
    });
    // the custom merge options, in their editor
    window.on_edit_merge({
        let weak = window.as_weak();
        let state = state.clone();
        let store = store.clone();
        let slot = slots.merge_options.clone();
        move || {
            let Some(window) = weak.upgrade() else {
                return;
            };
            let (decision, options) = {
                let mut state = state.borrow_mut();
                read_rule(&window, &store, &mut state);
                let Some(options) = state.rule.custom_merge.clone() else {
                    return;
                };
                (relationship(state.rule.action), options)
            };
            let applied: Rc<dyn Fn(hydrus_store::duplicates::merge::MergeOptions)> = {
                let state = state.clone();
                Rc::new(move |options| state.borrow_mut().rule.custom_merge = Some(options))
            };
            match crate::merge_options_window::open(
                &store, decision, &options, false, &slot, applied,
            ) {
                Ok(editor) => *slot.borrow_mut() = Some(editor),
                Err(e) => eprintln!("could not open the merge options: {e}"),
            }
        }
    });
    window.on_comparator_clicked({
        let weak = window.as_weak();
        let state = state.clone();
        move |i| {
            let Some(window) = weak.upgrade() else {
                return;
            };
            let mut state = state.borrow_mut();
            state.selected = usize::try_from(i).ok();
            show_rule(&window, &state, false);
        }
    });
    let comparator_done = {
        let weak = window.as_weak();
        let state = state.clone();
        move |at: Option<usize>| -> Rc<dyn Fn(Comparator)> {
            let weak = weak.clone();
            let state = state.clone();
            Rc::new(move |edited: Comparator| {
                let Some(window) = weak.upgrade() else {
                    return;
                };
                let mut state = state.borrow_mut();
                let list = &mut state.rule.comparators;
                match at.filter(|&i| i < list.len()) {
                    Some(i) => list[i] = edited,
                    None => list.push(edited),
                }
                // (what was wrong may be put right)
                state.errors.clear();
                show_rule(&window, &state, false);
            })
        }
    };
    let comparator_done = Rc::new(comparator_done);
    window.on_comparator_add({
        let weak = window.as_weak();
        let store = store.clone();
        let stack = slots.comparators.clone();
        let comparator_done = comparator_done.clone();
        move || {
            let Some(window) = weak.upgrade() else {
                return;
            };
            if let Some((_, _, comparator)) = usize::try_from(window.get_comparator_kind())
                .ok()
                .and_then(|i| comparator_choices().into_iter().nth(i))
            {
                edit_comparator(&store, comparator, &stack, comparator_done(None));
            }
        }
    });
    window.on_comparator_edit({
        let state = state.clone();
        let store = store.clone();
        let stack = slots.comparators.clone();
        let comparator_done = comparator_done.clone();
        move || {
            let chosen = {
                let state = state.borrow();
                state
                    .selected
                    .and_then(|i| state.rule.comparators.get(i).cloned().map(|c| (i, c)))
            };
            if let Some((i, comparator)) = chosen {
                edit_comparator(&store, comparator, &stack, comparator_done(Some(i)));
            }
        }
    });
    window.on_comparator_delete({
        let weak = window.as_weak();
        let state = state.clone();
        move || {
            let Some(window) = weak.upgrade() else {
                return;
            };
            let mut state = state.borrow_mut();
            if let Some(i) = state.selected.take()
                && i < state.rule.comparators.len()
            {
                state.rule.comparators.remove(i);
            }
            show_rule(&window, &state, false);
        }
    });
    // the list's export (0 clipboard, 1 png), import (2 clipboard, 3 pngs)
    // and duplicate (4), as `AddImportExportButtons` gives the reference's
    window.on_comparator_exchange({
        let weak = window.as_weak();
        let state = state.clone();
        let store = store.clone();
        let slots = slots.clone();
        move |mode| {
            use hydrus_gui_model::auto_resolution_exchange as ex;
            let Some(window) = weak.upgrade() else {
                return;
            };
            let selected = {
                let state = state.borrow();
                state
                    .selected
                    .and_then(|i| state.rule.comparators.get(i).cloned())
            };
            let add = |comparators: Vec<Comparator>, say: bool| {
                let n = comparators.len();
                {
                    let mut state = state.borrow_mut();
                    state.errors.clear();
                    state.rule.comparators.extend(comparators);
                    state.selected = state.rule.comparators.len().checked_sub(1);
                    show_rule(&window, &state, false);
                }
                if say && n > 0 {
                    crate::debug_actions::message("Information", &ex::added(n));
                }
            };
            let load = |text: &str, from_file: bool| match ex::import_comparators_text(
                text,
                &scales(&store),
            ) {
                Ok(imported) => {
                    if !imported.refused.is_empty() {
                        crate::debug_actions::message(
                            "Warning",
                            &ex::refused_message_for(&imported.refused, ex::COMPARATOR_TYPE),
                        );
                    }
                    add(imported.comparators, true);
                }
                Err(e) => import_failed(from_file, &e),
            };
            match mode {
                0 => {
                    if let Some(c) = selected {
                        crate::copy_to_clipboard(&ex::export_comparators_text(&[c]));
                    }
                }
                1 => {
                    if let Some(c) = selected
                        && let Err(e) = crate::png_export_window::open(
                            &slots.png,
                            &store,
                            ex::export_comparators_text(&[c]),
                            Rc::new(|| {}),
                        )
                    {
                        crate::debug_actions::message(ex::PROBLEM_TITLE, &e);
                    }
                }
                2 => match crate::clipboard_text() {
                    Ok(Some(text)) => load(&text, false),
                    Ok(None) => {}
                    Err(e) => crate::debug_actions::message(
                        ex::PROBLEM_TITLE,
                        &format!("Problem loading from clipboard: {e}"),
                    ),
                },
                3 => match crate::png_export_window::import_text_with_title(ex::PNG_PICKER_TITLE) {
                    Ok(Some(text)) => load(&text, true),
                    Ok(None) => {}
                    Err(e) => import_failed(true, &e),
                },
                4 => {
                    if let Some(c) = selected {
                        add(vec![c], false);
                    }
                }
                _ => {}
            }
        }
    });
    // the preview: the rule as edited, or why it can't be had
    let preview =
        crate::auto_resolution_preview_window::Preview::new(store, slots.preview_filter.clone());
    let edited = {
        let state = state.clone();
        let store = store.clone();
        move |window: &AutoResolutionRuleWindow| -> Result<Rule, String> {
            let mut state = state.borrow_mut();
            read_rule(window, &store, &mut state);
            let mut errors = state.errors.clone();
            if state.rule.action == RuleAction::Better
                && !rule_can_determine_better(&state.rule.comparators)
            {
                errors.push(CANNOT_DETERMINE_BETTER.into());
            }
            if errors.is_empty() {
                Ok(state.rule.clone())
            } else {
                Err(errors.join(" "))
            }
        }
    };
    window.on_preview_shown({
        let weak = window.as_weak();
        let preview = preview.clone();
        move || {
            if let Some(window) = weak.upgrade() {
                preview.shown(&window, edited(&window));
            }
        }
    });
    window.on_preview_refetch({
        let weak = window.as_weak();
        let preview = preview.clone();
        move || {
            if let Some(window) = weak.upgrade() {
                preview.refetch(&window);
            }
        }
    });
    window.on_preview_retest({
        let weak = window.as_weak();
        let preview = preview.clone();
        move || {
            if let Some(window) = weak.upgrade() {
                preview.retest(&window);
            }
        }
    });
    for passing in [true, false] {
        let preview = preview.clone();
        let weak = window.as_weak();
        let activated = move |row: i32| {
            let Some(window) = weak.upgrade() else {
                return;
            };
            if let Some(said) = usize::try_from(row)
                .ok()
                .and_then(|row| preview.activated(passing, row))
            {
                window.set_errors(said.into());
            }
        };
        if passing {
            window.on_preview_pass_activated(activated);
        } else {
            window.on_preview_fail_activated(activated);
        }
    }
    // a list's rows clicked, and its "show in a new page"
    for passing in [true, false] {
        let clicked = {
            let preview = preview.clone();
            let weak = window.as_weak();
            move |row: i32, control: bool, shift: bool| {
                if let (Some(window), Ok(row)) = (weak.upgrade(), usize::try_from(row)) {
                    preview.clicked(&window, passing, row, control, shift);
                }
            }
        };
        let show = {
            let preview = preview.clone();
            let open_files = slots.open_files.clone();
            move || {
                if let Some(open_files) = open_files.borrow().as_ref() {
                    preview.show_selected(passing, open_files);
                }
            }
        };
        if passing {
            window.on_preview_pass_clicked(clicked);
            window.on_preview_pass_show(show);
        } else {
            window.on_preview_fail_clicked(clicked);
            window.on_preview_fail_show(show);
        }
    }
    window.on_preview_fetch_changed({
        let weak = window.as_weak();
        let preview = preview.clone();
        move || {
            if let Some(window) = weak.upgrade() {
                let limit = (!window.get_preview_fetch_all())
                    .then(|| usize::try_from(window.get_preview_fetch_limit()).ok())
                    .flatten();
                preview.set_limit(&window, limit);
            }
        }
    });
    window.on_apply({
        let weak = window.as_weak();
        let state = state.clone();
        let store = store.clone();
        let close = close.clone();
        move || {
            let Some(window) = weak.upgrade() else {
                return;
            };
            let rule = {
                let mut state = state.borrow_mut();
                read_rule(&window, &store, &mut state);
                if state.rule.action == RuleAction::Better
                    && !rule_can_determine_better(&state.rule.comparators)
                {
                    state.errors.push(CANNOT_DETERMINE_BETTER.into());
                }
                if !state.errors.is_empty() {
                    show_rule(&window, &state, false);
                    return;
                }
                state.rule.clone()
            };
            close();
            done(rule);
        }
    });
    window.on_cancel({
        let close = close.clone();
        move || close()
    });
    window.window().on_close_requested({
        let close = close.clone();
        move || {
            close();
            slint::CloseRequestResponse::HideWindow
        }
    });
    show_rule(&window, &state.borrow(), true);
    window.show().map_err(|e| e.to_string())?;
    Ok(window)
}

// the "edit rules" list -------------------------------------------------------------

struct ListState {
    list: Named<RuleEdit>,
    /// The rules as they were, by id (written again only if changed).
    original: BTreeMap<i64, Rule>,
    deleted: Vec<i64>,
    progress: BTreeMap<i64, String>,
    asking_delete: bool,
    choosing_suggested: bool,
    suggested: Vec<Rule>,
    context: TextContext,
}

fn show_list(window: &AutoResolutionRulesWindow, state: &ListState) {
    let rows: Vec<TableRow> = state
        .list
        .in_order()
        .into_iter()
        .map(|(key, edit)| {
            let progress = edit
                .id
                .and_then(|id| state.progress.get(&id).cloned())
                .unwrap_or_else(|| "no pairs".into());
            let cells: Vec<SharedString> = row(&edit.rule, &progress, &state.context)
                .into_iter()
                .map(Into::into)
                .collect();
            TableRow {
                cells: ModelRc::new(VecModel::from(cells)),
                selected: state.list.selection.is_selected(key),
            }
        })
        .collect();
    window.set_rows(ModelRc::new(VecModel::from(rows)));
    window.set_one_selected(state.list.one_selected().is_some());
    window.set_any_selected(!state.list.selection.is_empty());
    window.set_choosing_suggested(state.choosing_suggested);
    window.set_asking(state.asking_delete);
    if state.asking_delete {
        window.set_asking_title("Are you sure?".into());
        window.set_asking_message("Remove all selected?".into());
        window.set_asking_choices(strings(vec!["yes".into(), "no".into()]));
    }
}

/// Write the rules as the list holds them: the deleted deleted, the
/// changed updated (which resets their progress, as the reference's does),
/// the new added.
fn write_rules(store: &Store, state: ListState) -> hydrus_store::Result<()> {
    let ListState {
        list,
        original,
        deleted,
        ..
    } = state;
    let rules = list.into_items();
    store.write(move |ctx| {
        let conn = ctx.conn();
        for id in &deleted {
            store_rules::delete_rule(conn, *id)?;
        }
        for edit in &rules {
            match edit.id {
                Some(id) => {
                    if original.get(&id) != Some(&edit.rule) {
                        store_rules::update_rule(conn, id, &edit.rule)?;
                    }
                }
                None => {
                    store_rules::add_rule(conn, &edit.rule, None)?;
                }
            }
        }
        Ok(())
    })
}

/// Open "edit rules" on the store's rules; "apply" writes them, then
/// `applied` runs.
pub(crate) fn open(store: &Arc<Store>, slots: &Slots, applied: Rc<dyn Fn()>) -> Result<(), String> {
    if let Some(window) = slots.list.borrow().as_ref() {
        return window.show().map_err(|e| e.to_string());
    }
    let (rules, progress) = store
        .read(|conn| {
            let rules = store_rules::rules(conn)?;
            let mut progress = BTreeMap::new();
            for (id, _) in &rules {
                let counts = store_rules::counts(conn, *id)?;
                progress.insert(*id, crate::duplicates_page::rule_progress(&counts));
            }
            Ok((rules, progress))
        })
        .map_err(|e| e.to_string())?;
    let original: BTreeMap<i64, Rule> = rules.iter().cloned().collect();
    let state = Rc::new(RefCell::new(ListState {
        list: Named::new(
            rules
                .into_iter()
                .map(|(id, rule)| RuleEdit { id: Some(id), rule })
                .collect(),
        ),
        original,
        deleted: Vec::new(),
        progress,
        asking_delete: false,
        choosing_suggested: false,
        suggested: store_rules::suggested_rules(),
        context: text_context(store),
    }));
    let window =
        crate::app_title::new::<crate::AutoResolutionRulesWindow>().map_err(|e| e.to_string())?;
    window.set_warning(RULES_WARNING.into());
    window.set_note(RULES_ORDER_NOTE.into());
    window.set_suggestions(strings(
        state
            .borrow()
            .suggested
            .iter()
            .map(|r| r.name.clone())
            .collect(),
    ));
    let refresh: Rc<dyn Fn()> = Rc::new({
        let weak = window.as_weak();
        let state = state.clone();
        move || {
            if let Some(window) = weak.upgrade() {
                show_list(&window, &state.borrow());
            }
        }
    });
    let close = {
        let weak = window.as_weak();
        let slot = slots.list.clone();
        move || {
            if let Some(window) = weak.upgrade() {
                let _ = window.hide();
            }
            slot.borrow_mut().take();
        }
    };
    window.on_row_clicked({
        let state = state.clone();
        let refresh = refresh.clone();
        move |r, ctrl, shift| {
            if let Ok(r) = usize::try_from(r) {
                state.borrow_mut().list.click(r, ctrl, shift);
            }
            refresh();
        }
    });
    window.on_add_suggested({
        let state = state.clone();
        let refresh = refresh.clone();
        move || {
            state.borrow_mut().choosing_suggested = true;
            refresh();
        }
    });
    window.on_suggested_chosen({
        let state = state.clone();
        let refresh = refresh.clone();
        move |i| {
            let mut state = state.borrow_mut();
            state.choosing_suggested = false;
            if let Some(rule) = usize::try_from(i)
                .ok()
                .and_then(|i| state.suggested.get(i).cloned())
            {
                state.list.add(RuleEdit { id: None, rule });
            }
            drop(state);
            refresh();
        }
    });
    window.on_add({
        let state = state.clone();
        let refresh = refresh.clone();
        let store = store.clone();
        let slots = slots.clone();
        move || {
            if slots.rule.borrow().is_some() {
                return;
            }
            let like = state.borrow().suggested.get(4).cloned();
            let Some(like) = like else {
                return;
            };
            let done: Rc<dyn Fn(Rule)> = {
                let state = state.clone();
                let refresh = refresh.clone();
                Rc::new(move |rule| {
                    state.borrow_mut().list.add(RuleEdit { id: None, rule });
                    refresh();
                })
            };
            match open_rule(&store, new_rule(&like), "edit rule", &slots, done) {
                Ok(editor) => *slots.rule.borrow_mut() = Some(editor),
                Err(e) => eprintln!("could not open the rule editor: {e}"),
            }
        }
    });
    window.on_edit({
        let state = state.clone();
        let refresh = refresh.clone();
        let store = store.clone();
        let slots = slots.clone();
        move || {
            if slots.rule.borrow().is_some() {
                return;
            }
            let chosen = {
                let state = state.borrow();
                state
                    .list
                    .one_selected()
                    .and_then(|key| state.list.get(key).cloned().map(|e| (key, e)))
            };
            let Some((key, edit)) = chosen else {
                return;
            };
            let done: Rc<dyn Fn(Rule)> = {
                let state = state.clone();
                let refresh = refresh.clone();
                Rc::new(move |rule| {
                    state
                        .borrow_mut()
                        .list
                        .replace(key, RuleEdit { id: edit.id, rule });
                    refresh();
                })
            };
            match open_rule(
                &store,
                edit.rule.clone(),
                "edit duplicates auto-resolution rule",
                &slots,
                done,
            ) {
                Ok(editor) => *slots.rule.borrow_mut() = Some(editor),
                Err(e) => eprintln!("could not open the rule editor: {e}"),
            }
        }
    });
    window.on_delete({
        let state = state.clone();
        let refresh = refresh.clone();
        move || {
            let mut state = state.borrow_mut();
            if !state.list.selection.is_empty() {
                state.asking_delete = true;
            }
            drop(state);
            refresh();
        }
    });
    window.on_exchange({
        let state = state.clone();
        let refresh = refresh.clone();
        let store = store.clone();
        let slots = slots.clone();
        move |mode| {
            exchange(&store, &slots, &state, mode);
            refresh();
        }
    });
    window.on_chosen({
        let state = state.clone();
        let refresh = refresh.clone();
        move |i| {
            let mut state = state.borrow_mut();
            state.asking_delete = false;
            if i == 0 {
                let gone = state.list.delete_selected();
                state.deleted.extend(gone.into_iter().filter_map(|e| e.id));
            }
            drop(state);
            refresh();
        }
    });
    window.on_cancelled({
        let state = state.clone();
        let refresh = refresh.clone();
        move || {
            state.borrow_mut().asking_delete = false;
            refresh();
        }
    });
    window.on_apply({
        let state = state.clone();
        let store = store.clone();
        let close = close.clone();
        move || {
            let taken = std::mem::replace(
                &mut *state.borrow_mut(),
                ListState {
                    list: Named::new(Vec::new()),
                    original: BTreeMap::new(),
                    deleted: Vec::new(),
                    progress: BTreeMap::new(),
                    asking_delete: false,
                    choosing_suggested: false,
                    suggested: Vec::new(),
                    context: TextContext::default(),
                },
            );
            if let Err(e) = write_rules(&store, taken) {
                eprintln!("could not write the rules: {e}");
            }
            close();
            applied();
        }
    });
    window.on_cancel({
        let close = close.clone();
        move || close()
    });
    window.window().on_close_requested({
        let close = close.clone();
        move || {
            close();
            slint::CloseRequestResponse::HideWindow
        }
    });
    show_list(&window, &state.borrow());
    window.show().map_err(|e| e.to_string())?;
    *slots.list.borrow_mut() = Some(window);
    Ok(())
}

/// The numerical rating services' scales, for imported rules' rating
/// predicates.
fn scales(store: &Store) -> impl Fn(&hydrus_core::ServiceKey) -> Option<(u64, bool)> {
    let snapshot = store.snapshot();
    move |key| match snapshot.services.by_key(key).ok().map(|s| &s.kind) {
        Some(hydrus_store::services::ServiceKind::RatingNumerical(c)) => {
            Some((u64::from(c.num_stars), c.allow_zero))
        }
        _ => None,
    }
}

/// Say why an import could not be read, in the wording of the source: files
/// (`_ImportJSONs`, `_ImportPNGs`) or the clipboard.
fn import_failed(from_file: bool, error: &str) {
    let (title, message) =
        hydrus_gui_model::auto_resolution_exchange::import_failure(from_file, error);
    crate::debug_actions::message(title, &message);
}

/// The list's export (0-2), import (3-5) and duplicate (6) buttons.
fn exchange(store: &Arc<Store>, slots: &Slots, state: &Rc<RefCell<ListState>>, mode: i32) {
    use hydrus_gui_model::auto_resolution_exchange as ex;
    let selected: Vec<Rule> = {
        let state = state.borrow();
        let order = state.list.order();
        state
            .list
            .selection
            .in_order(&order)
            .into_iter()
            .filter_map(|key| state.list.get(key).map(|e| e.rule.clone()))
            .collect()
    };
    let add = |rules: Vec<Rule>, say: bool| {
        let n = rules.len();
        let mut state = state.borrow_mut();
        state.list.selection = hydrus_gui_model::list_selection::ListSelection::default();
        for rule in rules {
            state.list.add(RuleEdit { id: None, rule });
        }
        drop(state);
        if say && n > 0 {
            crate::debug_actions::message("Information", &ex::added(n));
        }
    };
    let load = |text: &str, from_file: bool| match ex::import_text(text, &scales(store)) {
        Ok(imported) => {
            if !imported.refused.is_empty() {
                crate::debug_actions::message("Warning", &ex::refused_message(&imported.refused));
            }
            add(imported.rules, true);
        }
        Err(e) => import_failed(from_file, &e),
    };
    match mode {
        0 if !selected.is_empty() => crate::copy_to_clipboard(&ex::export_text(&selected)),
        1 if !selected.is_empty() => {
            if let Some(path) = crate::pick_exchange_export()
                && let Err(e) = std::fs::write(&path, ex::export_text(&selected))
            {
                crate::debug_actions::message(ex::PROBLEM_TITLE, &e.to_string());
            }
        }
        2 if !selected.is_empty() => {
            if let Err(e) = crate::png_export_window::open(
                &slots.png,
                store,
                ex::export_text(&selected),
                Rc::new(|| {}),
            ) {
                crate::debug_actions::message(ex::PROBLEM_TITLE, &e);
            }
        }
        3 => match crate::clipboard_text() {
            Ok(Some(text)) => load(&text, false),
            Ok(None) => {}
            Err(e) => crate::debug_actions::message(
                ex::PROBLEM_TITLE,
                &format!("Problem loading from clipboard: {e}"),
            ),
        },
        4 => {
            for path in crate::pick_exchange_files("select the json files", "json") {
                match std::fs::read_to_string(&path) {
                    Ok(text) => load(&text, true),
                    Err(e) => {
                        crate::debug_actions::message(ex::PROBLEM_TITLE, &e.to_string());
                        break;
                    }
                }
            }
        }
        5 => match crate::png_export_window::import_text_with_title(ex::PNG_PICKER_TITLE) {
            Ok(Some(text)) => load(&text, true),
            Ok(None) => {}
            Err(e) => import_failed(true, &e),
        },
        6 => add(selected, false),
        _ => {}
    }
}

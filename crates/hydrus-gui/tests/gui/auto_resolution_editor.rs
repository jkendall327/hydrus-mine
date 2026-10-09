//! The auto-resolution rule editor against the reference's, replaying
//! `oracle/fixtures/auto_resolution_editor.json`
//! (`oracle/record_auto_resolution_editor.py`: the real editor panels, their
//! controls driven and their values read back): the rule's name, paused box,
//! operation mode and maximum pending queue (and its unique naming in the
//! list), the one-file hardcoded and visual duplicates comparator editors,
//! and the duplicate metadata merge options editor's note settings.

use serde_json::Value;
use slint::Model as _;

use crate::auto_resolution_review::{Opened, opened};
use crate::duplicates_lane_page::{kind_index, list_of, newest_comparator, rule_window};
use hydrus_gui::{AutoResolutionRuleWindow, Bound};

fn strings(model: &slint::ModelRc<slint::SharedString>) -> Vec<String> {
    model.iter().map(|s| s.to_string()).collect()
}

fn labels(choices: &Value) -> Vec<String> {
    choices
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c[0].as_str().unwrap().to_owned())
        .collect()
}

/// A new rule in the open list, given a comparator that can tell A from B
/// (so a "better" action is allowed), named `name`.
fn add_rule(bound: &Bound, name: &str, with_comparator: bool) -> AutoResolutionRuleWindow {
    let list = list_of(bound);
    list.invoke_add();
    let rule = rule_window(bound);
    if with_comparator {
        rule.set_comparator_kind(kind_index(&rule, "test A or B using other file info"));
        rule.invoke_comparator_add();
        let hardcoded = newest_comparator(bound);
        hardcoded.set_looking(0);
        hardcoded.set_test(0);
        hardcoded.invoke_changed();
        hardcoded.invoke_apply();
    }
    rule.set_name(name.into());
    rule.invoke_changed();
    rule
}

fn row_names(bound: &Bound) -> Vec<String> {
    let rows = list_of(bound).get_rows();
    let mut names: Vec<String> = (0..rows.row_count())
        .map(|r| {
            rows.row_data(r)
                .unwrap()
                .cells
                .row_data(0)
                .unwrap()
                .to_string()
        })
        .collect();
    names.sort();
    names
}

// leaf: audit-media-rule-identity
#[test]
fn the_rule_editors_name_pause_mode_and_pending_limit_are_the_references() {
    let _windows = hydrus_gui::headless::init();
    let recorded = hydrus_testkit::fixture_json("auto_resolution_editor.json");
    let o = opened();
    let (ui, bound) = (&o.ui, &o.bound);
    ui.invoke_duplicates_action("edit rules".into(), 0, false, false);
    // (each case below in a list of its own, cancelled after)

    // a new rule's controls
    let start = &recorded["identity_start"];
    list_of(bound).invoke_add();
    let rule = rule_window(bound);
    assert_eq!(rule.get_name(), start["name"].as_str().unwrap());
    assert_eq!(rule.get_paused(), start["paused"].as_bool().unwrap());
    assert_eq!(
        strings(&rule.get_operation_choices()),
        labels(&start["modes"])
    );
    // (the reference's choice is its mode value, 1 then 2; ours its position)
    assert_eq!(
        rule.get_operation(),
        0,
        "semi-automatic first, as the reference's {}",
        start["mode"]
    );
    assert_eq!(
        i64::from(rule.get_pending_limit()),
        start["max_pending"].as_i64().unwrap(),
        "a new rule's maximum pending pairs"
    );
    assert_eq!(
        rule.get_pending_limit_enabled(),
        start["max_pending_enabled"]
    );
    assert!(!rule.get_no_pending_limit());
    rule.invoke_cancel();
    list_of(bound).invoke_cancel();

    // each case, through the controls and back out of the written rule
    for case in recorded["identity"].as_array().unwrap() {
        ui.invoke_duplicates_action("edit rules".into(), 0, false, false);
        let with_comparator = case["name"] != "no comparator";
        let rule = add_rule(bound, "x", with_comparator);
        let mode = case["mode"].as_i64().unwrap();
        let pending = case["max_pending"].as_i64();
        rule.set_name(case["name"].as_str().unwrap().into());
        rule.set_paused(case["paused"].as_bool().unwrap());
        rule.set_operation((mode - 1) as i32);
        rule.set_no_pending_limit(pending.is_none());
        if let Some(pending) = pending {
            rule.set_pending_limit(pending as i32);
        }
        rule.invoke_changed();
        assert_eq!(
            rule.get_pending_limit_enabled(),
            case["max_pending_enabled"].as_bool().unwrap(),
            "{case}"
        );
        if let Some(error) = case.get("error") {
            {
                // refused: "Hey, you have the action set to ..." and the editor stays open
                rule.invoke_apply();
                assert!(bound.auto_resolution.rule.borrow().is_some(), "{case}");
                let veto = error
                    .as_str()
                    .unwrap()
                    .strip_prefix("VetoException: ")
                    .unwrap();
                assert_eq!(rule.get_errors(), veto);
                rule.invoke_cancel();
            }
        } else {
            {
                assert_eq!(rule.get_errors(), "", "{case}");
                rule.invoke_apply();
                assert!(bound.auto_resolution.rule.borrow().is_none(), "{case}");
                let written = &case["rule"];
                // the rule as the list now holds it, opened again
                let name = written["name"].as_str().unwrap();
                let reopened = edit_named(bound, name);
                assert_eq!(reopened.get_name(), name, "the name is kept as typed");
                assert_eq!(reopened.get_paused(), written["paused"].as_bool().unwrap());
                assert_eq!(
                    i64::from(reopened.get_operation()) + 1,
                    written["mode"].as_i64().unwrap()
                );
                assert_eq!(
                    reopened.get_no_pending_limit(),
                    written["max_pending"].is_null()
                );
                if let Some(pending) = written["max_pending"].as_i64() {
                    assert_eq!(i64::from(reopened.get_pending_limit()), pending);
                }
                reopened.invoke_cancel();
            }
        }
        list_of(bound).invoke_cancel();
    }

    // names, case-folded and unique
    for case in recorded["names"].as_array().unwrap() {
        ui.invoke_duplicates_action("edit rules".into(), 0, false, false);
        // (the database has rules of its own; theirs are set aside)
        let others = row_names(bound);
        for existing in case["existing"].as_array().unwrap() {
            let rule = add_rule(bound, existing.as_str().unwrap(), true);
            rule.invoke_apply();
        }
        // (the existing names went in as typed, unless they collide among themselves)
        let imported = case["imported"].as_str().unwrap();
        let rule = add_rule(bound, imported, true);
        rule.invoke_apply();
        let mut theirs: Vec<String> = case["names"]
            .as_array()
            .unwrap()
            .iter()
            .map(|n| n.as_str().unwrap().to_owned())
            .collect();
        theirs.sort();
        let mut mine = row_names(bound);
        mine.retain(|n| !others.contains(n));
        assert_eq!(mine, theirs, "{case}");
        list_of(bound).invoke_cancel();
    }
}

fn edit_named(bound: &Bound, name: &str) -> AutoResolutionRuleWindow {
    let list = list_of(bound);
    let rows = list.get_rows();
    let row = (0..rows.row_count())
        .position(|r| rows.row_data(r).unwrap().cells.row_data(0).unwrap() == name)
        .unwrap_or_else(|| panic!("no rule {name:?}"));
    list.invoke_row_clicked(row as i32, false, false);
    list.invoke_edit();
    rule_window(bound)
}

// leaf: audit-media-comparator-hardcoded
#[test]
fn the_one_file_hardcoded_comparator_editor_is_the_references() {
    use hydrus_gui_model::auto_resolution_rules::comparator_summary;
    use hydrus_store::duplicates::auto::{Comparator, LookingAt, OneFileTest};
    let _windows = hydrus_gui::headless::init();
    let recorded = hydrus_testkit::fixture_json("auto_resolution_editor.json");
    let hardcoded = &recorded["hardcoded"];
    let Opened {
        ui, bound, store, ..
    } = opened();
    ui.invoke_duplicates_action("edit rules".into(), 0, false, false);
    list_of(&bound).invoke_add();
    let rule = rule_window(&bound);
    let context = hydrus_search::TextContext::default();
    let mut shown = false;
    let mut chosen = Vec::new();
    for case in hardcoded["cases"].as_array().unwrap() {
        rule.set_comparator_kind(kind_index(&rule, "test A or B using other file info"));
        rule.invoke_comparator_add();
        let window = newest_comparator(&bound);
        assert_eq!(
            window.get_window_title(),
            "edit one-file hardcoded comparator"
        );
        if !shown {
            shown = true;
            assert_eq!(
                strings(&window.get_looking_choices()),
                labels(&hardcoded["target_choices"])
            );
            assert_eq!(
                strings(&window.get_test_choices()),
                labels(&hardcoded["test_choices"])
            );
            assert_eq!(
                (window.get_looking(), window.get_test()),
                (
                    hardcoded["start"][0].as_i64().unwrap() as i32,
                    hardcoded["start"][1].as_i64().unwrap() as i32
                )
            );
        }
        let target = case["target"].as_i64().unwrap();
        let test = case["test"].as_i64().unwrap();
        window.set_looking(target as i32);
        window.set_test(test as i32);
        window.invoke_changed();
        let expected = Comparator::OneFileHardcoded {
            looking_at: [LookingAt::A, LookingAt::B, LookingAt::Either][target as usize],
            test: [
                OneFileTest::JpegIsProgressive,
                OneFileTest::JpegIsNotProgressive,
            ][test as usize],
        };
        let summary = case["summary"].as_str().unwrap();
        assert_eq!(window.get_summary(), summary, "the editor's summary");
        assert_eq!(comparator_summary(&expected, &context), summary);
        chosen.push(expected);
        window.invoke_apply();
        // as the rule lists it
        let listed = strings(&rule.get_comparators());
        assert_eq!(listed.last().unwrap(), summary);
    }
    // the rule written holds each as chosen
    rule.set_name("hardcoded".into());
    rule.invoke_changed();
    rule.invoke_apply();
    list_of(&bound).invoke_apply();
    let written = o_rules(&store, "hardcoded");
    assert_eq!(written.len(), 6);
    assert_eq!(written, chosen, "each comparator as chosen, in order");
}

fn o_rules(
    store: &hydrus_store::Store,
    name: &str,
) -> Vec<hydrus_store::duplicates::auto::Comparator> {
    // the comparators of the rule of this name the list wrote
    let rules = store.read(hydrus_store::duplicates::auto::rules).unwrap();
    let Some((_, rule)) = rules.into_iter().find(|(_, r)| r.name == name) else {
        panic!("no rule {name:?} written")
    };
    rule.comparators
}

// leaf: audit-media-comparator-visual
#[test]
fn the_visual_duplicates_comparator_editor_is_the_references() {
    use hydrus_gui_model::auto_resolution_rules::comparator_summary;
    use hydrus_store::duplicates::auto::Comparator;
    let _windows = hydrus_gui::headless::init();
    let recorded = hydrus_testkit::fixture_json("auto_resolution_editor.json");
    let visual = &recorded["visual"];
    let Opened {
        ui, bound, store, ..
    } = opened();
    ui.invoke_duplicates_action("edit rules".into(), 0, false, false);
    // (a "better" action needs a comparator that can tell A from B)
    let rule = add_rule(&bound, "visual", true);
    let context = hydrus_search::TextContext::default();
    let choices = labels(&visual["choices"]);
    // the "add comparator" offer includes this one, worded as the reference's
    let offered: Vec<String> = recorded["comparator_list"]["choices"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c[0].as_str().unwrap().to_owned())
        .collect();
    for kind in strings(&rule.get_comparator_kinds()) {
        assert!(
            offered.contains(&kind),
            "{kind:?} is offered by the reference"
        );
    }
    for (i, case) in visual["cases"].as_array().unwrap().iter().enumerate() {
        rule.set_comparator_kind(kind_index(&rule, "test if A and B are visual duplicates"));
        rule.invoke_comparator_add();
        let window = newest_comparator(&bound);
        assert_eq!(
            window.get_window_title(),
            "edit visual duplicates comparator"
        );
        assert_eq!(strings(&window.get_visual_choices()), choices);
        let start = visual["choices"]
            .as_array()
            .unwrap()
            .iter()
            .position(|c| c[1] == visual["start"])
            .unwrap();
        assert_eq!(
            window.get_visual(),
            start as i32,
            "starts on the reference's default (almost certainly)"
        );
        window.set_visual(i as i32);
        window.invoke_changed();
        let confidence = case["value"].as_i64().unwrap() as u32;
        let summary = case["summary"].as_str().unwrap();
        assert_eq!(window.get_summary(), summary);
        assert_eq!(
            comparator_summary(
                &Comparator::VisualDuplicates {
                    confidence: confidence as _
                },
                &context
            ),
            summary
        );
        window.invoke_apply();
        assert_eq!(strings(&rule.get_comparators()).last().unwrap(), summary);
    }
    rule.invoke_apply();
    list_of(&bound).invoke_apply();
    let written = o_rules(&store, "visual");
    let confidences: Vec<u32> = written
        .iter()
        .filter_map(|c| match c {
            Comparator::VisualDuplicates { confidence } => Some(u32::from(*confidence)),
            _ => None,
        })
        .collect();
    assert_eq!(
        confidences,
        visual["cases"]
            .as_array()
            .unwrap()
            .iter()
            .map(|c| c["value"].as_u64().unwrap() as u32)
            .collect::<Vec<_>>()
    );
}

// leaf: audit-media-merge-note-settings
#[test]
fn the_note_merge_settings_dialog_is_the_references() {
    use hydrus_core::notes::{NoteConflict, NoteMerge};
    use hydrus_store::duplicates::merge::DuplicateMergeSettings;
    let recorded = hydrus_testkit::fixture_json("auto_resolution_editor.json");
    let crate::duplicates_lane_page::Opened { ui, store, .. } =
        crate::duplicates_lane_page::opened();
    let conflict_of = |n: i64| {
        [
            NoteConflict::Replace,
            NoteConflict::Ignore,
            NoteConflict::Append,
            NoteConflict::Rename,
        ][n as usize]
    };
    let mut first = true;
    for case in recorded["note_settings"].as_array().unwrap() {
        // the decision: 2 same quality, 4 better
        let (index, better) = match case["decision"].as_i64().unwrap() {
            2 => (1, false),
            4 => (0, true),
            other => panic!("decision {other}"),
        };
        let start = (
            case["start"][0].as_bool().unwrap(),
            conflict_of(case["start"][1].as_i64().unwrap()),
        );
        store
            .write(move |ctx| {
                let mut settings: DuplicateMergeSettings = hydrus_store::settings::get(ctx.conn())?;
                let merge = Some(NoteMerge {
                    extend_existing: start.0,
                    conflict: start.1,
                });
                if better {
                    settings.better.note_merge = merge;
                } else {
                    settings.same_quality.note_merge = merge;
                }
                hydrus_store::settings::set(ctx.conn(), &settings)
            })
            .unwrap();
        ui.invoke_duplicates_action("merge options".into(), index, false, false);
        let editor = hydrus_gui::merge_options_window::last_opened().expect("it opens");
        assert!(editor.get_note_settings_enabled());
        editor.invoke_note_settings();
        assert!(editor.get_note_settings_open());
        let dialog = &case["dialog"];
        if first {
            first = false;
            // the rows the dialog shows, and the conflict choices, as recorded
            let rows: Vec<String> = dialog["labels"]
                .as_array()
                .unwrap()
                .iter()
                .map(|l| l.as_str().unwrap().to_owned())
                .collect();
            assert_eq!(
                rows,
                [
                    editor.get_note_extend_label().to_string(),
                    editor.get_note_conflict_label().to_string()
                ]
            );
            assert_eq!(
                strings(&editor.get_note_conflicts()),
                labels(&dialog["conflict_choices"])
            );
        }
        // the starting state, as the reference's dialog shows it
        assert_eq!(
            editor.get_note_extend(),
            dialog["extend"].as_bool().unwrap()
        );
        assert_eq!(
            i64::from(editor.get_note_conflict()),
            dialog["conflict"].as_i64().unwrap()
        );
        editor.set_note_extend(case["edit"][0].as_bool().unwrap());
        editor.set_note_conflict(case["edit"][1].as_i64().unwrap() as i32);
        editor.invoke_note_settings_done(case["accept"].as_bool().unwrap());
        assert!(!editor.get_note_settings_open());
        editor.invoke_apply();
        let written: DuplicateMergeSettings = store.read(hydrus_store::settings::get).unwrap();
        let held = if better {
            written.better.note_merge
        } else {
            written.same_quality.note_merge
        }
        .unwrap();
        assert_eq!(
            (held.extend_existing, held.conflict),
            (
                case["after"][0].as_bool().unwrap(),
                conflict_of(case["after"][1].as_i64().unwrap())
            ),
            "{case}"
        );
    }
}

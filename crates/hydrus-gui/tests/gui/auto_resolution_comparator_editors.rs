//! The auto-resolution rule editor's comparators built through the nested
//! editors as a user would build them, and read as the reference words
//! them: the comparators of the reference's own rules, recorded by
//! `oracle/record_auto_resolution_summaries.py` (each rule's comparators'
//! `GetSummary`). The AND and OR groups get children added, an existing
//! child edited, one deleted and groups nested in groups; the relative
//! comparator gets each operator family with its multiplier, delta and
//! approximate range.

use slint::Model as _;

use hydrus_core::search::comparable::Comparable;
use hydrus_core::search::number::NumberOp;
use hydrus_gui_model::auto_resolution_rules::{PROPERTIES, operator_choices};

use crate::duplicates_lane_page::{
    kind_index, kind_index_sub, list_of, newest_comparator, opened, rule_window,
};

const RELATIVE: &str = "test A against B using file info";

/// The recorded summary of the reference's comparator that starts with `start`.
fn recorded(start: &str) -> String {
    let recorded = hydrus_testkit::fixture_json("auto_resolution_summaries.json");
    recorded["rules"]
        .as_array()
        .unwrap()
        .iter()
        .flat_map(|r| r["comparators"].as_array().unwrap())
        .map(|c| c[0].as_str().unwrap().to_owned())
        .find(|s| s.starts_with(start))
        .unwrap_or_else(|| panic!("no recorded comparator {start:?}"))
}

/// Set a relative comparator editor to a property and operator, then the
/// multiplier, delta and range.
fn relative(
    window: &hydrus_gui::ComparatorWindow,
    property: Comparable,
    op: NumberOp,
    multiplier: &str,
    delta: i32,
    range: i32,
) {
    let at = PROPERTIES.iter().position(|p| *p == property).unwrap();
    window.set_property(at as i32);
    window.invoke_changed();
    let op_at = operator_choices(property)
        .iter()
        .position(|o| std::mem::discriminant(o) == std::mem::discriminant(&op))
        .unwrap();
    window.set_operator(op_at as i32);
    window.invoke_changed();
    window.set_multiplier(multiplier.into());
    window.set_delta(delta);
    if window.get_approximate() {
        window.set_range(range);
    }
    window.invoke_changed();
    assert_eq!(window.get_errors(), "");
}

fn subs(window: &hydrus_gui::ComparatorWindow) -> Vec<String> {
    window.get_subs().iter().map(|s| s.to_string()).collect()
}

// leaf: audit-media-comparator-relative
#[test]
fn a_relative_comparator_of_each_operator_family_reads_as_the_references() {
    let o = opened();
    o.ui.invoke_duplicates_action("edit rules".into(), 0, false, false);
    list_of(&o.bound).invoke_add();
    let rule = rule_window(&o.bound);
    rule.set_name("relative".into());
    rule.invoke_changed();
    // (the property, the operator, the multiplier, the delta and the range,
    // for the comparators of the reference's "more comparators" rule)
    let cases: [(&str, Comparable, NumberOp, &str, i32, i32); 6] = [
        ("A has \"system:ratio\" wider than B", Comparable::Ratio, NumberOp::Greater, "1", 0, 0),
        (
            "A has \"system:ratio\" taller than or exactly 1.25x B",
            Comparable::Ratio,
            NumberOp::LessOrEqual,
            "1.25",
            0,
            0,
        ),
        (
            "A has \"system:duration\" later than B +2.5 seconds",
            Comparable::Duration,
            NumberOp::Greater,
            "1",
            2500,
            0,
        ),
        (
            "A has \"system:framerate\" ≈ 1.10x B ±5%",
            Comparable::Framerate,
            NumberOp::ApproxPercent { percent: 5 },
            "1.1",
            0,
            5,
        ),
        (
            "A has \"system:number of tags\" ≥ B +3",
            Comparable::NumTags,
            NumberOp::GreaterOrEqual,
            "1",
            3,
            0,
        ),
        (
            "A has \"system:number of pixels\" ≠ B",
            Comparable::NumPixels,
            NumberOp::NotEqual,
            "1",
            0,
            0,
        ),
    ];
    for (start, property, op, multiplier, delta, range) in cases {
        rule.set_comparator_kind(kind_index(&rule, RELATIVE));
        rule.invoke_comparator_add();
        let window = newest_comparator(&o.bound);
        relative(&window, property, op, multiplier, delta, range);
        let theirs = recorded(start);
        assert_eq!(window.get_summary(), theirs, "{start}");
        window.invoke_apply();
        assert_eq!(
            rule.get_comparators().iter().last().unwrap(),
            theirs,
            "{start}: as the list shows it"
        );
    }
}

// leaf: audit-media-comparator-and, audit-media-comparator-or
#[test]
fn and_and_or_groups_are_built_edited_and_nested_to_read_as_the_references() {
    let o = opened();
    o.ui.invoke_duplicates_action("edit rules".into(), 0, false, false);
    list_of(&o.bound).invoke_add();
    let rule = rule_window(&o.bound);
    rule.set_name("groups".into());
    rule.invoke_changed();

    // the reference's "pixel-perfect pairs" first comparator:
    // an OR of an AND of two relative tests, and a relative test
    let theirs = recorded("(A has \"system:filesize\" = B");
    rule.set_comparator_kind(kind_index(&rule, "OR Comparator"));
    rule.invoke_comparator_add();
    let or = newest_comparator(&o.bound);
    assert_eq!(or.get_window_title(), "edit OR comparator");

    // an AND group inside the OR, with a child that is edited after it is made
    or.set_sub_kind(kind_index_sub(&or, "AND Comparator"));
    or.invoke_sub_add();
    let and = newest_comparator(&o.bound);
    assert_eq!(and.get_window_title(), "edit AND comparator");
    and.set_sub_kind(kind_index_sub(&and, RELATIVE));
    and.invoke_sub_add();
    let child = newest_comparator(&o.bound);
    // (made as "less than", edited to "=" below)
    relative(&child, Comparable::Size, NumberOp::Less, "1", 0, 0);
    child.invoke_apply();
    assert_eq!(subs(&and), ["A has \"system:filesize\" < B"]);
    and.invoke_sub_clicked(0);
    and.invoke_sub_edit();
    let child = newest_comparator(&o.bound);
    assert_eq!(child.get_summary(), "A has \"system:filesize\" < B");
    relative(&child, Comparable::Size, NumberOp::Equal, "1", 0, 0);
    child.invoke_apply();
    assert_eq!(subs(&and), ["A has \"system:filesize\" = B"]);
    // a second child, then one that is deleted again
    and.set_sub_kind(kind_index_sub(&and, RELATIVE));
    and.invoke_sub_add();
    let child = newest_comparator(&o.bound);
    relative(&child, Comparable::ImportTime, NumberOp::Less, "1", 0, 0);
    child.invoke_apply();
    and.set_sub_kind(kind_index_sub(&and, "A and B have the same filetype"));
    and.invoke_sub_add();
    assert_eq!(subs(&and).len(), 3);
    and.invoke_sub_clicked(2);
    and.invoke_sub_delete();
    assert_eq!(subs(&and).len(), 2);
    and.invoke_apply();

    // the OR's second child
    or.set_sub_kind(kind_index_sub(&or, RELATIVE));
    or.invoke_sub_add();
    let child = newest_comparator(&o.bound);
    relative(&child, Comparable::Size, NumberOp::Greater, "1", 0, 0);
    child.invoke_apply();
    assert_eq!(or.get_subs().row_count(), 2);
    assert_eq!(or.get_summary(), theirs);
    or.invoke_apply();
    assert_eq!(rule.get_comparators().iter().last().unwrap(), theirs);

    // the reference's "everything" rule's last comparator: an AND with the
    // same filetype and an approximate size, OR a jpeg quality test; edited
    // from the one before it by editing the OR's first child (an AND) to be
    // made of different parts
    let theirs = recorded("(A and B have the same filetype, A has \"system:filesize\" ≈ 1.50x B");
    rule.set_comparator_kind(kind_index(&rule, "OR Comparator"));
    rule.invoke_comparator_add();
    let or = newest_comparator(&o.bound);
    or.set_sub_kind(kind_index_sub(&or, "AND Comparator"));
    or.invoke_sub_add();
    let and = newest_comparator(&o.bound);
    and.set_sub_kind(kind_index_sub(&and, "A and B have the same filetype"));
    and.invoke_sub_add();
    and.set_sub_kind(kind_index_sub(&and, RELATIVE));
    and.invoke_sub_add();
    let child = newest_comparator(&o.bound);
    relative(
        &child,
        Comparable::Size,
        NumberOp::ApproxPercent { percent: 20 },
        "1.5",
        0,
        20,
    );
    child.invoke_apply();
    and.invoke_apply();
    or.set_sub_kind(kind_index_sub(&or, "A has clearly better jpeg quality than B"));
    or.invoke_sub_add();
    assert_eq!(or.get_summary(), theirs);
    // the group, opened again from the list, shows what it was made of
    or.invoke_apply();
    rule.invoke_comparator_clicked(1);
    rule.invoke_comparator_edit();
    let or = newest_comparator(&o.bound);
    assert_eq!(or.get_summary(), theirs);
    or.invoke_sub_clicked(0);
    or.invoke_sub_edit();
    let and = newest_comparator(&o.bound);
    assert_eq!(subs(&and).len(), 2);
}

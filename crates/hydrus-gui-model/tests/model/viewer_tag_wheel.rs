//! Direction, delay and no-scrollbar gates from actual Qt hover wheel handlers.
use hydrus_gui_model::{
    options::{Editor, Kind, Row, Settings},
    viewer_tag_wheel::WheelGate,
};
use hydrus_store::{
    Store,
    settings::{self, TagWheelPropagation, ViewerTagScrollSettings},
};
const LABEL: &str = "Allow a mouse wheel scroll over the taglist to propagate to the main canvas:";
// leaf: audit-options-media-viewer-hovers-hover-windows-allow-a-mouse-wheel-scroll-over-the-taglist-to-propagate-to-the-main-canvas
#[test]
fn parent_wheel_gates_replay_actual_qt_boundaries_and_directions() {
    let fixture = hydrus_testkit::fixture_json("viewer_tag_wheel.json");
    for case in fixture["cases"].as_array().unwrap() {
        let mut gate = WheelGate {
            media_transition: case["transition"].as_f64().unwrap(),
            last_list_scroll: case["last_scroll"].as_f64().unwrap(),
            list_direction: i32::try_from(case["last_direction"].as_i64().unwrap()).unwrap(),
            last_parent_wheel: case["last_parent"].as_f64().unwrap(),
            parent_direction: i32::try_from(case["parent_direction"].as_i64().unwrap()).unwrap(),
        };
        let code = u16::try_from(case["code"].as_u64().unwrap()).unwrap();
        let dy = case["dy"].as_i64().unwrap();
        assert_eq!(
            gate.propagates(
                TagWheelPropagation::from_code(code).unwrap(),
                case["scrollbar"].as_bool().unwrap(),
                case["now"].as_f64().unwrap(),
                if dy > 0 { 1 } else { -1 }
            ),
            case["propagates"].as_bool().unwrap(),
            "{case}"
        );
    }
    let scroll = &fixture["scroll"][0];
    assert!(scroll["consumed"].as_bool().unwrap());
    assert!(!fixture["scroll"][1]["consumed"].as_bool().unwrap());
    let mut gate = WheelGate::default();
    gate.list_scrolled(scroll["now"].as_f64().unwrap(), -1);
    assert_eq!(
        gate.last_list_scroll.to_bits(),
        scroll["last_scroll"].as_f64().unwrap().to_bits()
    );
    assert_eq!(
        gate.list_direction,
        scroll["last_direction"].as_i64().unwrap() as i32
    );
    gate.parent_direction = -1;
    gate.last_parent_wheel = 1000.0;
    gate.media_changed(1100.0);
    assert_eq!(gate.parent_direction, -1);
    gate.media_changed(1250.001);
    assert_eq!(gate.parent_direction, 0, "reference 250-second reset");
}
// leaf: audit-options-media-viewer-hovers-hover-windows-allow-a-mouse-wheel-scroll-over-the-taglist-to-propagate-to-the-main-canvas
#[test]
fn four_policy_choices_stage_cancel_save_and_reopen_with_reference_labels() {
    let fixture = hydrus_testkit::fixture_json("viewer_tag_wheel.json");
    let directory = tempfile::tempdir().unwrap();
    let store = Store::open(directory.path()).unwrap();
    let before = store.read(Settings::load).unwrap();
    assert_eq!(
        before.viewer_tag_scroll.0.code(),
        fixture["initial"].as_u64().unwrap() as u16
    );
    let mut editor = Editor::new(before.clone());
    let page = editor
        .page_names()
        .iter()
        .position(|name| *name == "media viewer hovers")
        .unwrap();
    editor.show_page(page);
    let row = editor
        .rows()
        .iter()
        .position(|row| matches!(row,Row::Opt{option,..} if option.label==LABEL))
        .unwrap();
    let Row::Opt { option, .. } = &editor.rows()[row] else {
        panic!("actual option");
    };
    let Kind::Choice(choices) = &option.kind else {
        panic!("actual dropdown");
    };
    assert_eq!(serde_json::json!(choices), fixture["choices"]);
    for code in 0..4 {
        editor.choose(row, code);
        let (value, _, errors) = editor.applied();
        assert!(errors.is_empty(), "{errors:?}");
        assert_eq!(
            value.viewer_tag_scroll.0.code(),
            u16::try_from(code).unwrap()
        );
    }
    assert_eq!(
        store.read(Settings::load).unwrap(),
        before,
        "parent Cancel keeps original"
    );
    let (after, _, _) = editor.applied();
    store
        .write(move |ctx| after.save(ctx.conn(), &before))
        .unwrap();
    let reopened = Store::open(directory.path()).unwrap();
    assert_eq!(
        reopened
            .read(settings::get::<ViewerTagScrollSettings>)
            .unwrap()
            .0,
        TagWheelPropagation::Immediately
    );
}

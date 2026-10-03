//! Selecting thumbnails is the reference's (`oracle/record_thumbnail_selection.py`):
//! its grid's own selection code, run through a script of clicks (plain,
//! ctrl, shift), select all and none, focus moves and files leaving the
//! page, selects and focuses the files ours does after every step.

use hydrus_core::HashId;
use hydrus_gui_model::selection::{Move, Selection};
use serde_json::Value;

fn file(i: &Value) -> HashId {
    HashId(u32::try_from(i.as_u64().unwrap()).unwrap() + 100)
}

#[test]
fn selecting_thumbnails_is_the_reference_s() {
    let fixture = hydrus_testkit::fixture_json("thumbnail_selection.json");
    let count = u32::try_from(fixture["files"].as_u64().unwrap()).unwrap();
    let columns = fixture["columns"].as_u64().unwrap() as usize;
    let page_rows = fixture["page_rows"].as_u64().unwrap() as usize;
    let mut sorted: Vec<HashId> = (0..count).map(|i| HashId(i + 100)).collect();
    let mut selection = Selection::default();
    let steps = fixture["steps"].as_array().unwrap();
    assert!(steps.len() > 40);
    for (n, expected) in steps.iter().enumerate() {
        let step = expected["step"].as_array().unwrap();
        let flag = |i: usize| step[i].as_bool().unwrap();
        match step[0].as_str().unwrap() {
            "click" => {
                let hit = (!step[1].is_null()).then(|| file(&step[1]));
                selection.hit(&sorted, hit, flag(2), flag(3));
            }
            "all" => selection.select_all(&sorted),
            "none" => selection.select_none(&sorted),
            "move" => {
                let to = match step[1].as_str().unwrap() {
                    "left" => Move::Left,
                    "right" => Move::Right,
                    "up" => Move::Up,
                    "down" => Move::Down,
                    "page_up" => Move::PageUp,
                    "page_down" => Move::PageDown,
                    "home" => Move::Home,
                    "end" => Move::End,
                    other => panic!("{other}"),
                };
                let moved = selection.move_focus(&sorted, to, flag(2), columns, page_rows);
                assert!(moved.is_some(), "step {n}: says where it moved");
                if !flag(2) {
                    assert_eq!(moved.map(|i| sorted[i]), selection.focused(), "step {n}");
                }
            }
            "remove" => {
                let files: Vec<HashId> = step[1].as_array().unwrap().iter().map(file).collect();
                selection.remove(&sorted, &files);
                sorted.retain(|f| !files.contains(f));
            }
            other => panic!("{other}"),
        }
        let selected: Vec<HashId> = expected["selected"]
            .as_array()
            .unwrap()
            .iter()
            .map(file)
            .collect();
        assert_eq!(selection.files(&sorted), selected, "step {n}: {step:?}");
        let focused = (!expected["focused"].is_null()).then(|| file(&expected["focused"]));
        assert_eq!(selection.focused(), focused, "step {n}: {step:?}");
    }
}

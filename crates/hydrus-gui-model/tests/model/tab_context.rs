//! Replay actual Qt notebook ordering/movement from the reference recording.
use hydrus_gui_model::tab_context::{self, Move, Sort, Summary};

#[test]
fn sorts_and_moves_match_the_reference() {
    let fixture: serde_json::Value = hydrus_testkit::fixture_json("tab_context.json");
    let pages: Vec<_> = fixture["pages"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| Summary {
            name: p["name"].as_str().unwrap().into(),
            files: p["files"].as_u64().unwrap() as usize,
            progress: (
                p["progress"][0].as_u64().unwrap() as usize,
                p["progress"][1].as_u64().unwrap() as usize,
            ),
            size: p["size"].as_u64().unwrap(),
        })
        .collect();
    for step in fixture["sorts"].as_array().unwrap() {
        let by = match step["by"].as_str().unwrap() {
            "files" => Sort::Files,
            "size" => Sort::Size,
            _ => Sort::Name,
        };
        let actual = tab_context::order(&pages, by, step["ascending"].as_bool().unwrap());
        assert_eq!(serde_json::json!(actual), step["order"]);
    }
    for step in fixture["moves"].as_array().unwrap() {
        let movement = match step["movement"].as_str().unwrap() {
            "first" => Move::First,
            "left" => Move::Left,
            "right" => Move::Right,
            _ => Move::Last,
        };
        let index = step["index"].as_u64().unwrap() as usize;
        let mut order: Vec<_> = (0..pages.len()).collect();
        if let Some(target) = tab_context::destination(index, pages.len(), movement) {
            let moved = order.remove(index);
            order.insert(target, moved);
        }
        assert_eq!(serde_json::json!(order), step["order"]);
        assert_eq!(step["selected"], fixture["selected"]);
    }
}

#[test]
fn menus_expose_six_sorts_and_only_possible_moves() {
    use hydrus_gui_model::main_menu::{Command, Entry};
    let menu = tab_context::menu(2, 1, 4);
    let Entry::Menu { entries, .. } = &menu[1] else {
        panic!("sort submenu")
    };
    assert_eq!(entries.len(), 6);
    for entry in entries {
        let Entry::Item {
            command: Some(Command::SortTabs { depth, .. }),
            ..
        } = entry
        else {
            panic!("working sort")
        };
        assert_eq!(*depth, 2);
    }
    let Entry::Menu { entries, .. } = &tab_context::menu(0, 0, 4)[0] else {
        panic!("move submenu")
    };
    assert_eq!(
        entries.iter().map(Entry::label).collect::<Vec<_>>(),
        ["right", "to right end"]
    );
    assert!(tab_context::menu(0, 0, 1).is_empty());
    assert!(tab_context::menu(0, 6, 4).is_empty());
}

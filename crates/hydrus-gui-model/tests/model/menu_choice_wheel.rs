//! Real wheel protocol and retained type orders, plus staged field-scoped settings.
use hydrus_core::tag_sort::{TagGroupBy, TagSortType};
use hydrus_gui_model::{
    menu_choice_wheel,
    options::{Editor, Row, Settings, Value},
};
use hydrus_store::{Store, settings};
#[test]
fn signed_wrap_and_empty_single_choice_signals_match_actual_qt() {
    let fixture = hydrus_testkit::fixture_json("menu_choice_wheel.json");
    for case in fixture["cases"].as_array().unwrap() {
        let choices = case["choices"].as_array().unwrap();
        let current = choices
            .iter()
            .position(|choice| choice[1] == case["before"]);
        let selected = if case["enabled"] == true {
            current.and_then(|current| {
                menu_choice_wheel::next(current, choices.len(), case["dy"].as_f64().unwrap() as f32)
            })
        } else {
            None
        };
        assert_eq!(
            selected.map_or(&case["before"], |i| &choices[i][1]),
            &case["after"],
            "{case}"
        );
        assert_eq!(
            u64::from(selected.is_some()),
            case["emissions"].as_u64().unwrap()
        );
        assert_eq!(case["accepted"], case["enabled"]);
    }
    assert_eq!(menu_choice_wheel::next(7, 3, 120.0), None);
}
#[test]
fn simple_options_sort_retains_the_recorded_text_and_count_orders() {
    let fixture = hydrus_testkit::fixture_json("menu_choice_wheel.json");
    let mut editor = Editor::new(Settings::default());
    let page = editor
        .page_names()
        .iter()
        .position(|name| *name == "tag sort")
        .unwrap();
    editor.show_page(page);
    let row=editor.rows().iter().position(|row|matches!(row,Row::Opt{option,..} if option.label=="Default tag sort in search pages: ")).unwrap();
    for case in fixture["tags"].as_array().unwrap() {
        let (part, count) = match case["field"].as_str().unwrap() {
            "_sort_type" => (0, 3),
            "_sort_order_count" | "_sort_order_text" => (1, 2),
            "_group_by" => (2, 3),
            "_use_siblings" => continue,
            _ => panic!("field"),
        };
        let before = editor.applied().0.tag_presentation.search_page_sort;
        let current = match part {
            0 => match before.sort_type {
                TagSortType::Tag => 0,
                TagSortType::Subtag => 1,
                TagSortType::Count => 2,
            },
            1 => {
                if before.sort_type == TagSortType::Count {
                    usize::from(before.ascending)
                } else {
                    usize::from(!before.ascending)
                }
            }
            _ => match before.group_by {
                TagGroupBy::Nothing => 0,
                TagGroupBy::NamespaceAz => 1,
                TagGroupBy::NamespaceUser => 2,
            },
        };
        if case["enabled"] == true {
            editor.tag_sort(
                row,
                part,
                menu_choice_wheel::next(current, count, case["dy"].as_f64().unwrap() as f32)
                    .unwrap(),
            );
        }
        let actual = editor.applied().0.tag_presentation.search_page_sort;
        assert_eq!(
            actual.sort_type,
            match case["after"]["type"].as_u64().unwrap() {
                0 => TagSortType::Tag,
                1 => TagSortType::Subtag,
                2 => TagSortType::Count,
                _ => panic!("type"),
            }
        );
        assert_eq!(actual.ascending, case["after"]["order"] == 0);
        assert_eq!(
            actual.group_by,
            match case["after"]["group"].as_u64().unwrap() {
                0 => TagGroupBy::Nothing,
                1 => TagGroupBy::NamespaceAz,
                2 => TagGroupBy::NamespaceUser,
                _ => panic!("group"),
            }
        );
    }
    assert!(
        matches!(
            editor.rows()[row],
            Row::Opt {
                value: Value::TagSort(_),
                ..
            }
        ),
        "saved/query type stays TagSort"
    );
}
#[test]
fn staged_setting_saves_only_its_key_and_unchanged_draft_preserves_live_value() {
    let directory = tempfile::tempdir().unwrap();
    let store = Store::open(directory.path()).unwrap();
    let before = store.read(Settings::load).unwrap();
    let mut editor = Editor::new(before.clone());
    let page = editor
        .page_names()
        .iter()
        .position(|name| *name == "gui")
        .unwrap();
    editor.show_page(page);
    let row=editor.rows().iter().position(|row|matches!(row,Row::Opt{option,..} if option.label=="Mouse wheel can \"scroll\" through menu buttons: ")).unwrap();
    editor.check(row, false);
    assert!(
        store
            .read(hydrus_store::menu_choice_wheel::load)
            .unwrap()
            .enabled
    );
    store
        .write(|writer| {
            let mut live: settings::GuiSettings = settings::get(writer.conn())?;
            live.application_display_name = "concurrent wheel owner".into();
            settings::set(writer.conn(), &live)
        })
        .unwrap();
    let (after, original, problems) = editor.applied();
    assert!(problems.is_empty());
    let original = original.clone();
    store
        .write(move |writer| after.save(writer.conn(), &original))
        .unwrap();
    assert!(
        !store
            .read(hydrus_store::menu_choice_wheel::load)
            .unwrap()
            .enabled
    );
    assert_eq!(
        store
            .read(settings::get::<settings::GuiSettings>)
            .unwrap()
            .application_display_name,
        "concurrent wheel owner"
    );
    let unchanged = before.clone();
    store
        .write(move |writer| unchanged.save(writer.conn(), &before))
        .unwrap();
    assert!(
        !store
            .read(hydrus_store::menu_choice_wheel::load)
            .unwrap()
            .enabled
    );
    assert!(
        !Store::open(directory.path())
            .unwrap()
            .read(hydrus_store::menu_choice_wheel::load)
            .unwrap()
            .enabled
    );
}

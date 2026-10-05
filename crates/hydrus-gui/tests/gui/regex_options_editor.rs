//! Actual Qt Options RegexPanel replay and the real saved matcher input consumer.
use hydrus_gui::{
    MainWindow, OptionsWindow, Pages, RegexFavouritesWindow, bind, headless,
    regex_favourites_window, string_processor_window,
};
use hydrus_gui_model::regex_favourites::RegexFavourites;
use hydrus_store::{regex_favourites, settings};
use serde_json::{Value, json};
use slint::{ComponentHandle as _, Model as _};
use std::{cell::RefCell, rc::Rc};

fn open(ui: &MainWindow, bound: &hydrus_gui::Bound) -> OptionsWindow {
    ui.invoke_menu_title_pressed(0, 20.0, 22.0);
    let lines = ui.get_menu_panes().row_data(0).unwrap().lines;
    let at = lines
        .iter()
        .position(|r| r.label == "options\u{2026}")
        .unwrap();
    ui.invoke_menu_line_clicked(0, at as i32, 0.0, 0.0, 0.0);
    let w = bound.options.borrow().as_ref().unwrap().clone_strong();
    let p = w
        .get_pages()
        .iter()
        .position(|p| p.text == "regex favourites")
        .unwrap() as i32;
    w.set_page(p);
    w.invoke_page_chosen(p);
    w
}
fn rows(w: &RegexFavouritesWindow) -> Value {
    json!(
        w.get_rows()
            .iter()
            .map(|r| r.cells.iter().map(|s| s.to_string()).collect::<Vec<_>>())
            .collect::<Vec<_>>()
    )
}
fn choose_row(w: &RegexFavouritesWindow, pair: &Value, ctrl: bool) {
    let i = rows(w)
        .as_array()
        .unwrap()
        .iter()
        .position(|r| r == pair)
        .unwrap();
    w.invoke_row_clicked(i as i32, ctrl, false);
}
fn menu(w: &RegexFavouritesWindow) -> Value {
    let pane = w.get_favourite_panes().row_data(0).unwrap();
    json!(
        pane.lines
            .iter()
            .map(|l| if l.kind == 2 {
                json!({"separator":true})
            } else {
                json!({"label":l.label.to_string(),"enabled":l.usable})
            })
            .collect::<Vec<_>>()
    )
}
fn replay(w: &RegexFavouritesWindow, f: &Value, copied: &Rc<RefCell<Vec<String>>>) {
    for step in f["steps"].as_array().unwrap() {
        let op = &step["operation"];
        let action = op["action"].as_str().unwrap();
        match action {
            "add" => w.invoke_action("add".into()),
            "edit" => {
                choose_row(w, &op["selected"], false);
                w.invoke_action("edit".into());
            }
            "delete" => {
                for (i, pair) in op["selected"].as_array().unwrap().iter().enumerate() {
                    choose_row(w, pair, i > 0);
                }
                w.invoke_action("delete".into());
                w.invoke_action(
                    if op["answer"] == true {
                        "confirm-delete"
                    } else {
                        "cancel-delete"
                    }
                    .into(),
                );
            }
            _ => unreachable!(),
        }
        if action != "delete" {
            w.set_phrase(op["phrase"].as_str().unwrap().into());
            w.invoke_changed();
            assert_eq!(w.get_valid(), step["opened"][0]["valid"].as_bool().unwrap());
            if op["menu"] == true {
                let before = copied.borrow().len();
                w.invoke_favourite_menu(20.0, 20.0);
                assert_eq!(menu(w), step["menus"][0]["rows"]);
                // Instruction is enabled but performs no copy; favourites only copy.
                let labels = w
                    .get_favourite_panes()
                    .row_data(0)
                    .unwrap()
                    .lines
                    .iter()
                    .filter(|l| l.kind != 2)
                    .map(|l| l.label.to_string())
                    .collect::<Vec<_>>();
                for label in labels {
                    w.invoke_favourite_menu(20.0, 20.0);
                    let pane = w.get_favourite_panes().row_data(0).unwrap();
                    let i = pane.lines.iter().position(|l| l.label == label).unwrap();
                    w.invoke_favourite_line_clicked(0, i as i32, 0.0, 0.0, 0.0);
                }
                assert_eq!(json!(&copied.borrow()[before..]), step["copied"]);
                assert_eq!(w.get_phrase(), op["phrase"].as_str().unwrap());
            }
            if op["phrase_accept"] == false || op["description"].is_null() {
                w.invoke_action("cancel-row".into());
            } else {
                w.set_description(op["description"].as_str().unwrap().into());
                w.invoke_action("save-row".into());
                if let Some(warning) = step["said"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .find_map(|s| s["warning"].as_str())
                {
                    assert_eq!(w.get_error(), warning);
                    w.invoke_action("cancel-row".into());
                }
            }
        }
        assert_eq!(rows(w), step["after"]["draft"]);
    }
}

#[test]
fn real_options_saved_input_chooser_crud_cancel_apply_and_retired_owners_match_qt() {
    use hydrus_core::url::strings::{MatchKind, PyRegex, StringMatch};

    let (_dirs, store) = crate::subscriptions::store();
    let f = hydrus_testkit::fixture_json("regex_options_editor.json");
    let initial = RegexFavourites(serde_json::from_value(f["initial"].clone()).unwrap());
    let seed = initial.clone();
    store
        .write(move |ctx| settings::set(ctx.conn(), &seed))
        .unwrap();
    let windows = headless::init();
    let ui = MainWindow::new().unwrap();
    ui.show().unwrap();
    let bound = bind(&ui, Pages::open(store.clone()).unwrap());
    let copied = Rc::new(RefCell::new(Vec::new()));
    hydrus_gui::set_clipper({
        let copied = copied.clone();
        move |c| {
            if let hydrus_gui::Clip::Text(t) = c {
                copied.borrow_mut().push(t.clone());
            }
        }
    });
    for accept in [false, true] {
        let options = open(&ui, &bound);
        options.invoke_regex_favourites_clicked();
        let child = regex_favourites_window::last_opened().unwrap();
        assert!(options.get_regex_child_open());
        let regex_page = options.get_page();
        let routing_page = options
            .get_pages()
            .iter()
            .position(|p| p.text == "open externally")
            .unwrap() as i32;
        options.set_page(routing_page);
        options.invoke_page_chosen(routing_page);
        assert_eq!(
            options.get_page(),
            regex_page,
            "regex child owns page navigation"
        );
        options.invoke_search_edited("open externally".into());
        let found = options
            .get_matches()
            .iter()
            .position(|m| m.contains("open externally"))
            .unwrap() as i32;
        options.invoke_search_chosen(found);
        assert_eq!(
            options.get_page(),
            regex_page,
            "search cannot bypass the regex child"
        );
        options.invoke_routing_url_action("add".into());
        options.invoke_shortcuts_clicked();
        assert!(!bound.options_open_externally.has_open());
        assert!(!options.get_shortcuts_child_open());
        options.invoke_apply();
        assert!(bound.options.borrow().is_some());
        assert_eq!(store.read(regex_favourites::load).unwrap(), initial);
        options.hide().unwrap();
        child.invoke_action("apply".into());
        assert!(child.window().is_visible());
        assert_eq!(store.read(regex_favourites::load).unwrap(), initial);
        options.show().unwrap();
        let before_blank = rows(&child);
        child.invoke_action("add".into());
        child.set_phrase("".into());
        child.set_description("".into());
        child.invoke_changed();
        assert!(
            child.get_valid(),
            "an empty regex is valid, unlike an empty description"
        );
        child.invoke_action("save-row".into());
        assert_eq!(
            child.get_error(),
            f["descriptions"][0]["error"].as_str().unwrap()
        );
        assert_eq!(rows(&child), before_blank);
        child.invoke_action("cancel-row".into());
        replay(&child, &f, &copied);
        if !accept {
            let pixels = headless::render(&windows.get(2).unwrap(), 780, 500);
            headless::save_png(
                &std::path::Path::new(env!("CARGO_TARGET_TMPDIR"))
                    .join("regex_options_editor_native.png"),
                &pixels,
                780,
                500,
            )
            .unwrap();
        }
        child.invoke_action("apply".into());
        assert!(!options.get_regex_child_open());
        assert_eq!(store.read(regex_favourites::load).unwrap(), initial);
        if accept {
            options.invoke_apply();
        } else {
            options.invoke_cancel();
        }
        let expected = if accept { &f["saved"] } else { &f["initial"] };
        assert_eq!(
            json!(store.read(regex_favourites::load).unwrap().0),
            *expected
        );
        // Retained owner senders cannot reopen children or commit into successors.
        options.invoke_regex_favourites_clicked();
        child.invoke_action("apply".into());
        assert!(regex_favourites_window::last_opened().is_none());
        let fresh = open(&ui, &bound);
        options.invoke_regex_favourites_clicked();
        assert!(regex_favourites_window::last_opened().is_none());
        fresh.invoke_regex_favourites_clicked();
        let pending = regex_favourites_window::last_opened().unwrap();
        pending.invoke_action("add".into());
        pending.set_phrase("discarded".into());
        pending.invoke_favourite_menu(20.0, 20.0);
        let count = copied.borrow().len();
        fresh.invoke_cancel();
        pending.invoke_favourite_line_clicked(0, 2, 0.0, 0.0, 0.0);
        pending.invoke_action("save-row".into());
        pending.invoke_action("apply".into());
        assert_eq!(copied.borrow().len(), count);
        assert!(!pending.window().is_visible());
        assert_eq!(
            json!(store.read(regex_favourites::load).unwrap().0),
            *expected
        );
    }
    // Reciprocal composition: an existing shortcut child blocks regex input.
    let options = open(&ui, &bound);
    options.invoke_shortcuts_clicked();
    let shortcuts = hydrus_gui::shortcut_windows::last_set().unwrap();
    assert!(options.get_shortcuts_child_open());
    options.invoke_regex_favourites_clicked();
    assert!(regex_favourites_window::last_opened().is_none());
    assert!(!options.get_regex_child_open());
    shortcuts.invoke_cancel();
    options.invoke_regex_favourites_clicked();
    let child = regex_favourites_window::last_opened().unwrap();
    assert!(options.get_regex_child_open());
    options.invoke_cancel();
    assert!(!child.window().is_visible());
    assert_eq!(
        json!(store.read(regex_favourites::load).unwrap().0),
        f["saved"]
    );
    let slots = string_processor_window::Slots::default();
    let input = StringMatch {
        kind: MatchKind::Regex(PyRegex::new("original+")),
        example: "original".into(),
        ..StringMatch::any()
    };
    string_processor_window::open_match(
        &store,
        &input,
        &slots,
        Rc::new(|_| panic!("copied input is cancelled")),
    );
    let matcher = slots.step.borrow().as_ref().unwrap().clone_strong();
    matcher.hide().unwrap();
    matcher.invoke_manage_favourites();
    assert!(
        slots.favourites.borrow().is_none(),
        "hidden matcher cannot open a child"
    );
    matcher.show().unwrap();
    matcher.invoke_favourite_menu(20.0, 20.0);
    let pane = matcher.get_favourite_panes().row_data(0).unwrap();
    let actual = json!(
        pane.lines
            .iter()
            .map(|l| if l.kind == 2 {
                json!({"separator":true})
            } else {
                json!({"label":l.label.to_string(),"enabled":l.usable})
            })
            .collect::<Vec<_>>()
    );
    assert_eq!(actual, f["consumer"]["menu"]["rows"]);
    let before = copied.borrow().len();
    for label in ["letters only", "letters"] {
        matcher.invoke_favourite_menu(20.0, 20.0);
        let pane = matcher.get_favourite_panes().row_data(0).unwrap();
        let i = pane.lines.iter().position(|l| l.label == label).unwrap();
        matcher.invoke_favourite_line_clicked(0, i as i32, 0.0, 0.0, 0.0);
    }
    assert_eq!(json!(&copied.borrow()[before..]), f["consumer"]["copied"]);
    assert_eq!(
        matcher.get_match_regex(),
        f["consumer"]["input"].as_str().unwrap()
    );
    matcher.invoke_cancel();
    let reopened = hydrus_store::Store::open(store.dir()).unwrap();
    assert_eq!(
        json!(reopened.read(regex_favourites::load).unwrap().0),
        f["reopened"]
    );
}

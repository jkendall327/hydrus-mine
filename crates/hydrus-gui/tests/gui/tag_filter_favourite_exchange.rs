//! The tag filter editor's favourite export and import, compared with
//! `oracle/fixtures/tag_filter_favourite_exchange.json`
//! (`record_tag_filter_favourite_exchange.py`): the text each entry of the
//! export menu puts on the clipboard and what the editor says of text that is
//! not a Tag Filter.
use hydrus_core::tag_filter::{FilterRule, TagFilter};
use hydrus_gui::{Clip, TagFilterWindow, headless, set_clipper, set_paster, tag_filter_window};
use hydrus_gui_model::tag_filter_editor::FavouriteTagFilters;
use hydrus_store::Store;
use serde_json::Value;
use slint::{ComponentHandle as _, Model as _};
use std::{cell::RefCell, rc::Rc, sync::Arc};

fn rules(value: &Value) -> TagFilter {
    let mut filter = TagFilter::new();
    for rule in value.as_array().unwrap() {
        filter = filter.with_rule(
            rule[0].as_str().unwrap(),
            if rule[1] == 1 {
                FilterRule::Blacklist
            } else {
                FilterRule::Whitelist
            },
        );
    }
    filter
}

/// A Tag Filter object's kind, version and rules, the rules sorted.
fn rules_of(text: &str) -> Value {
    let mut value: Value = serde_json::from_str(text).unwrap();
    value[2]
        .as_array_mut()
        .unwrap()
        .sort_by_key(ToString::to_string);
    value
}

fn choices(w: &TagFilterWindow) -> Vec<String> {
    let names = w.get_asking_choices();
    (0..names.row_count())
        .map(|i| names.row_data(i).unwrap().to_string())
        .collect()
}

/// A favourites store and an editor open on the recording's tag filter.
fn editor(fixture: &Value) -> (tempfile::TempDir, Arc<Store>, TagFilterWindow) {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path()).unwrap();
    // (the favourites in the order the recording made them)
    let mut favourites: Vec<_> = fixture["favourites"].as_object().unwrap().iter().collect();
    favourites.sort_by_key(|(name, _)| match name.as_str() {
        "zebra" => 0,
        _ => 1,
    });
    for (name, filter) in favourites {
        FavouriteTagFilters::save(&store, name.clone(), rules(filter), true).unwrap();
    }
    let slot = Rc::new(RefCell::new(None));
    let window = tag_filter_window::open(
        &store,
        &rules(&fixture["current"]),
        false,
        "test filter",
        "",
        &slot,
        Rc::new(|_| {}),
    )
    .unwrap();
    *slot.borrow_mut() = Some(window.clone_strong());
    (dir, store, window)
}

// leaf: audit-shared-tag-favourite-export
#[test]
fn the_export_menu_copies_what_the_reference_copies() {
    let _windows = headless::init();
    let fixture = hydrus_testkit::fixture_json("tag_filter_favourite_exchange.json");
    let (dir, _store, w) = editor(&fixture);
    let clip = Rc::new(RefCell::new(Vec::<Clip>::new()));
    set_clipper({
        let clip = clip.clone();
        move |c| clip.borrow_mut().push(c.clone())
    });
    w.invoke_favourite("export".into());
    let want: Vec<&str> = fixture["exports"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| e["label"].as_str().unwrap())
        .collect();
    assert_eq!(choices(&w), want, "the export menu's entries");
    for (n, export) in fixture["exports"].as_array().unwrap().iter().enumerate() {
        w.invoke_favourite("export".into());
        w.invoke_chosen(n as i32);
        let Clip::Text(text) = clip.borrow().last().cloned().unwrap() else {
            panic!("text export");
        };
        // the reference lists the rules in the order its set holds them (the
        // editor's blacklist then whitelist, or as a favourite was made); ours
        // are sorted by tag slice. So: the reference's text is exactly its
        // rules written as Python's `json.dumps` writes them, and ours is
        // exactly the same rules, sorted, written the same way.
        let reference = export["text"].as_str().unwrap();
        let written = |value: &Value| {
            let rules: Vec<String> = value[2]
                .as_array()
                .unwrap()
                .iter()
                .map(|r| format!("[{}, {}]", r[0], r[1]))
                .collect();
            format!("[44, 1, [{}]]", rules.join(", "))
        };
        let theirs: Value = serde_json::from_str(reference).unwrap();
        assert_eq!(
            written(&theirs),
            reference,
            "{}: the format",
            export["label"]
        );
        assert_eq!(
            text,
            written(&rules_of(reference)),
            "{}: our text",
            export["label"]
        );
        // and "export to a file" writes the same text
        let path = dir.path().join(format!("export {n}.json"));
        w.set_exchange_path(path.to_string_lossy().as_ref().into());
        w.invoke_exchange_action("save".into());
        assert_eq!(
            std::fs::read_to_string(&path).unwrap(),
            text,
            "{}: the file",
            export["label"]
        );
        w.invoke_cancelled();
    }
}

// leaf: audit-shared-tag-favourite-import
#[test]
fn importing_what_is_not_a_tag_filter_says_what_the_reference_says() {
    let _windows = headless::init();
    let fixture = hydrus_testkit::fixture_json("tag_filter_favourite_exchange.json");
    for import in fixture["imports"].as_array().unwrap() {
        let text = import["text"].as_str().unwrap();
        let (_dir, store, w) = editor(&fixture);
        let before = FavouriteTagFilters::load(&store).unwrap();
        set_paster({
            let text = text.to_owned();
            move || text.clone()
        });
        w.invoke_favourite("import".into());
        w.set_exchange_text(text.into());
        w.invoke_exchange_action("import".into());
        let shown = w.get_exchange_error().to_string();
        let reference = import["errors"][0]["message"].as_str().unwrap();
        assert!(!w.get_favourite_naming(), "{text:?} asks no name");
        assert_eq!(shown, reference, "{text:?}");
        assert_eq!(
            FavouriteTagFilters::load(&store).unwrap(),
            before,
            "{text:?}"
        );
    }
}

fn recorded_rules(value: &Value) -> Vec<(String, bool)> {
    let mut rules: Vec<_> = value
        .as_array()
        .unwrap()
        .iter()
        .map(|r| (r[0].as_str().unwrap().to_owned(), r[1] == 1))
        .collect();
    rules.sort();
    rules
}

fn rules_in(filter: &TagFilter) -> Vec<(String, bool)> {
    let mut rules: Vec<_> = filter
        .rules()
        .map(|(slice, rule)| (slice.to_owned(), rule == FilterRule::Blacklist))
        .collect();
    rules.sort();
    rules
}

// leaf: audit-shared-tag-favourite-import
#[test]
fn importing_a_tag_filter_asks_a_name_and_an_overwrite_as_the_reference_does() {
    let _windows = headless::init();
    let fixture = hydrus_testkit::fixture_json("tag_filter_favourites.json");
    let events: Vec<&Value> = fixture["events"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|e| e["action"].as_str().unwrap().starts_with("import"))
        .take(4)
        .collect();
    assert_eq!(events.len(), 4);
    // the steps of the recording: cancel at the name, "no" at the overwrite, a
    // new name, "yes" at the overwrite
    let steps: [(&str, Option<&str>, Option<bool>); 4] = [
        ("import_cancel", None, None),
        ("import_no", Some("zebra"), Some(false)),
        ("import", Some("apple"), None),
        ("import_yes", Some("zebra"), Some(true)),
    ];
    for (n, (action, name, overwrite)) in steps.into_iter().enumerate() {
        let event = events[n];
        assert_eq!(event["action"], action);
        // where the step started: the favourites and the draft the step before left
        // (the first started on "zebra", which was also the draft)
        let before = if n == 0 { events[0] } else { events[n - 1] };
        let dir = tempfile::tempdir().unwrap();
        let store = Store::open(dir.path()).unwrap();
        for (favourite, filter) in before["favourites"].as_object().unwrap() {
            FavouriteTagFilters::save(&store, favourite.clone(), rules(filter), true).unwrap();
        }
        let slot = Rc::new(RefCell::new(None));
        let applied = Rc::new(RefCell::new(Vec::<TagFilter>::new()));
        let w = tag_filter_window::open(
            &store,
            &rules(&before["draft"]),
            false,
            "test filter",
            "",
            &slot,
            {
                let applied = applied.clone();
                Rc::new(move |f| applied.borrow_mut().push(f))
            },
        )
        .unwrap();
        *slot.borrow_mut() = Some(w.clone_strong());

        // the clipboard holds a filter with untidy rules, which the reference cleans
        w.invoke_favourite("import".into());
        w.set_exchange_text(fixture["dirty_payload"].as_str().unwrap().into());
        w.invoke_exchange_action("import".into());
        let questions = event["questions"].as_array().unwrap();
        assert_eq!(
            w.get_asking_message(),
            questions[0].as_str().unwrap(),
            "{action}"
        );
        match name {
            None => w.invoke_cancelled(),
            Some(name) => {
                w.invoke_favourite_named(name.into());
                match overwrite {
                    Some(yes) => {
                        assert_eq!(
                            w.get_asking_message(),
                            questions[1].as_str().unwrap(),
                            "{action}"
                        );
                        w.invoke_chosen(i32::from(!yes));
                    }
                    None => assert_eq!(questions.len(), 1, "{action}"),
                }
            }
        }
        let mut got = serde_json::Map::new();
        for (favourite, filter) in FavouriteTagFilters::load(&store).unwrap().0 {
            got.insert(favourite, serde_json::json!(rules_in(&filter)));
        }
        let mut want = serde_json::Map::new();
        for (favourite, rules) in event["favourites"].as_object().unwrap() {
            want.insert(favourite.clone(), serde_json::json!(recorded_rules(rules)));
        }
        assert_eq!(got, want, "{action}: the favourites");
        // and the draft: the imported filter once one is taken in
        w.invoke_apply();
        assert_eq!(
            rules_in(applied.borrow().last().unwrap()),
            recorded_rules(&event["draft"]),
            "{action}: the draft"
        );
    }
}

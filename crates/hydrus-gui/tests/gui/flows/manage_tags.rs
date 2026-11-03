//! Flow (`oracle/record_flow_manage_tags.py`): manage tags across many
//! files with siblings and parents in play. `kitty` → `cat` and `cat` →
//! `animal` set on "my tags"; `cat` and `fluffy` added to three files and
//! `kitty` to a fourth; all four managed together (`fluffy`, which three
//! have, asks what to do: "delete"; `striped` added to all); `striped`
//! removed from one through the remove button (which asks); one file's
//! tags copied and pasted onto another; two files sent to the inbox, undone
//! and redone from the undo menu; and searches by the changed tags. After
//! each step what the user sees must be what the reference showed.

use std::cell::RefCell;
use std::collections::BTreeSet;
use std::rc::Rc;

use serde_json::{Value as Json, json};
use slint::{ComponentHandle as _, Model as _};

use hydrus_core::Sha256;
use hydrus_gui::{Bound, MainWindow, ManageTagsWindow, Pages, SearchPage, bind, headless};
use hydrus_store::Store;

use super::{Known, Normaliser, State, compare};
use crate::common::{main_menu, widgets};

/// What this flow shows that the reference does not, each explained in
/// `docs/rust/DIFFERENCES.md`.
const KNOWN: &[Known] = &[
    // hydrus-rs offers to undo manage tags' edits; the reference's manage
    // tags commits past its undo manager, so its undo menu has nothing
    Known {
        path: "(undo)/dialog/after tagging",
        why: "Content undo",
    },
    Known {
        path: "(undo)/dialog/after undo",
        why: "Content undo",
    },
];

fn sha(hex_text: &str) -> Sha256 {
    Sha256::from_slice(&hex::decode(hex_text).unwrap()).unwrap()
}

/// The manage tags window's tag list, as its rows read, each tag's parent
/// rows (indented, under it) sorted (the reference lists them in a set's
/// order, which varies run to run).
fn rows(manage: &ManageTagsWindow) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut parents: Vec<String> = Vec::new();
    for text in manage.get_tags().iter().map(|r| r.text.to_string()) {
        if text.starts_with("    ") {
            parents.push(text);
        } else {
            parents.sort();
            out.append(&mut parents);
            out.push(text);
        }
    }
    parents.sort();
    out.append(&mut parents);
    out
}

/// `tag` entered as typed: the suggestion for the tag itself if there is
/// one (a sibling's old tag shows as "kitty → cat", and Enter would take
/// the ideal), else the text as typed.
fn enter(manage: &ManageTagsWindow, tag: &str) {
    manage.invoke_text_edited(tag.into());
    widgets::settle();
    let exact = manage
        .get_suggestions()
        .iter()
        .position(|s| s.text == tag || s.text.starts_with(&format!("{tag} \u{2192} ")));
    match exact {
        Some(at) => manage.invoke_suggestion_chosen(i32::try_from(at).unwrap()),
        None => manage.invoke_entered(),
    }
    widgets::settle();
}

/// The undo menu's content undo and redo items, as the reference's
/// `GetUndoRedoStrings` gives them (null where there is none).
fn undo_strings(ui: &MainWindow) -> Json {
    let lines = main_menu::open(ui, &["undo"]);
    ui.invoke_menu_dismissed();
    let find = |prefix: &str| {
        lines
            .iter()
            .find(|l| l.0.starts_with(prefix))
            .map(|l| l.0.clone())
    };
    json!([find("undo "), find("redo ")])
}

#[test]
fn tags_managed_across_many_files_reach_search_as_the_reference_does() {
    let recording = hydrus_testkit::fixture_json("flow_manage_tags.json");
    let (_dirs, store) = super::basic_store();
    let _windows = headless::init();
    let clipboard = Rc::new(RefCell::new(String::new()));
    hydrus_gui::set_clipper({
        let clipboard = clipboard.clone();
        move |clip| {
            if let hydrus_gui::Clip::Text(text) = clip {
                clipboard.borrow_mut().clone_from(text);
            }
        }
    });
    hydrus_gui::set_paster({
        let clipboard = clipboard.clone();
        move || clipboard.borrow().clone()
    });
    let ui = MainWindow::new().unwrap();
    ui.show().unwrap();
    let bound = bind(&ui, Pages::single(SearchPage::new(store.clone())));
    let n = Normaliser::local();
    let state = State {
        store: &store,
        ui: &ui,
        site: None,
        n: &n,
    };
    let files: Vec<String> = recording["files"]
        .as_array()
        .unwrap()
        .iter()
        .map(|h| h.as_str().unwrap().to_owned())
        .collect();
    let [file_a, file_b, file_c, file_d] = [&files[0], &files[1], &files[2], &files[3]];
    let mine = store.snapshot().services.by_name("my tags").unwrap().id;

    // a search page of every file
    let search = |predicates: &[&str]| {
        while ui.get_predicates().row_count() > 0 {
            ui.invoke_remove_predicate(0);
            widgets::settle();
        }
        for p in predicates {
            ui.invoke_search_edited((*p).into());
            ui.invoke_search_accepted();
            widgets::settle();
        }
    };
    search(&["system:everything"]);

    let hash_id = |hash: &str| {
        let sha = sha(hash);
        store
            .read(move |c| hydrus_store::master::hash_id(c, &sha))
            .unwrap()
            .unwrap()
    };
    let select = |hashes: &[&String]| {
        ui.invoke_select_none();
        for (i, hash) in hashes.iter().enumerate() {
            let id = hash_id(hash);
            let at = {
                let page = bound.current.borrow();
                let page = page.borrow();
                page.results().iter().position(|&r| r == id).unwrap()
            };
            ui.invoke_thumbnail_clicked(i32::try_from(at).unwrap(), i > 0, false);
        }
        widgets::settle();
    };
    // a file's tags in my tags, as stored and as displayed
    let tags_of = |hash: &str| -> Json {
        let id = hash_id(hash);
        let snapshot = store.snapshot();
        store
            .read(|conn| {
                let batch = hydrus_store::media::load(conn, &snapshot.services, None, &[id])?;
                let tags = batch.results[0]
                    .tags
                    .get(&mine)
                    .and_then(|t| t.by_status.get(&hydrus_core::ContentStatus::Current))
                    .cloned()
                    .unwrap_or_default();
                let graph = snapshot.display.get(mine);
                let mut stored = BTreeSet::new();
                let mut displayed = BTreeSet::new();
                for t in tags {
                    stored.insert(batch.tags[&t].as_str().to_owned());
                    for shown in graph.display_tags(t) {
                        displayed.insert(
                            hydrus_store::master::tag(conn, shown)?
                                .unwrap()
                                .as_str()
                                .to_owned(),
                        );
                    }
                }
                Ok(json!({"stored": stored, "displayed": displayed}))
            })
            .unwrap()
    };
    let all_tags = || -> Json {
        files
            .iter()
            .map(|h| (h.clone(), tags_of(h)))
            .collect::<serde_json::Map<_, _>>()
            .into()
    };
    let inbox = || -> Json {
        let shown = state.files(files.iter().cloned().collect());
        files
            .iter()
            .map(|h| (h.clone(), shown[h]["inbox"].clone()))
            .collect::<serde_json::Map<_, _>>()
            .into()
    };
    let step =
        |name: &str, dialog: Json| json!({"step": name, "dialog": dialog, "files": all_tags()});
    // manage tags on `hashes`, on my tags: `script` acts on it, then apply
    // (or cancel); what it showed
    let manage = |hashes: &[&String],
                  script: &dyn Fn(&ManageTagsWindow, &mut serde_json::Map<String, Json>),
                  accept: bool|
     -> Json {
        select(hashes);
        ui.invoke_manage_tags_selected();
        let manage = bound
            .manage_tags
            .borrow()
            .as_ref()
            .expect("the manage tags dialog")
            .clone_strong();
        let at = manage
            .get_service_names()
            .iter()
            .position(|s| s == "my tags")
            .unwrap();
        manage.invoke_service_chosen(i32::try_from(at).unwrap());
        widgets::settle();
        let mut seen = serde_json::Map::new();
        seen.insert("title".into(), json!(manage.get_window_title().as_str()));
        seen.insert("rows".into(), json!(rows(&manage)));
        script(&manage, &mut seen);
        seen.insert("rows after".into(), json!(rows(&manage)));
        if accept {
            manage.invoke_apply();
        } else {
            manage.invoke_cancel();
        }
        widgets::settle();
        assert!(bound.manage_tags.borrow().is_none(), "the dialog closed");
        Json::Object(seen)
    };
    let entering = |tags: &'static [&'static str]| {
        move |m: &ManageTagsWindow, _: &mut serde_json::Map<String, Json>| {
            for tag in tags {
                enter(m, tag);
            }
        }
    };
    let mut steps = Vec::new();

    // 1. relationships: kitty → cat, cat → animal, on my tags
    let mut seen = serde_json::Map::new();
    for (kind, left, right) in [("siblings", "kitty", "cat"), ("parents", "cat", "animal")] {
        let w = super::tag_relationships::open(&ui, &bound, kind);
        let services = w.get_service_names();
        let at = (0..services.row_count())
            .find(|&i| services.row_data(i).unwrap() == "my tags")
            .unwrap();
        w.invoke_service_chosen(i32::try_from(at).unwrap());
        widgets::settle();
        w.invoke_enter_tags(false, left.into());
        w.invoke_enter_tags(true, right.into());
        w.invoke_add();
        widgets::settle();
        let pairs: Vec<Vec<String>> = w
            .get_rows()
            .iter()
            .map(|r| r.cells.iter().map(|c| c.to_string()).collect())
            .collect();
        seen.insert(kind.into(), json!({"rows": pairs}));
        w.invoke_apply();
        widgets::settle();
    }
    steps.push(step("relationships", Json::Object(seen)));

    // 2. add: cat and fluffy on A, B and C; kitty on D
    let seen = json!({
        "A B C": manage(&[file_a, file_b, file_c], &entering(&["cat", "fluffy"]), true),
        "D": manage(&[file_d], &entering(&["kitty"]), true),
    });
    steps.push(step("add", seen));

    // 3. mixed: all four; fluffy (three have it) activated in the list asks,
    // "delete"; striped typed
    let mixed = |m: &ManageTagsWindow, seen: &mut serde_json::Map<String, Json>| {
        // fluffy activated in the tag list (typed, it would only add)
        let at = rows(m)
            .iter()
            .position(|r| r.starts_with("fluffy ("))
            .unwrap();
        m.invoke_tag_clicked(i32::try_from(at).unwrap(), false, false);
        m.invoke_tag_activated(i32::try_from(at).unwrap(), false, false);
        widgets::settle();
        let asked = json!([{
            "title": m.get_tag_menu_question_title().as_str(),
            "message": m.get_tag_menu_question().as_str(),
            "choices": [m.get_tag_menu_yes_label().as_str(), m.get_tag_menu_no_label().as_str()],
        }]);
        assert!(
            m.get_tag_menu_no_label().starts_with("delete"),
            "the second choice deletes: {:?} / {:?} / {:?} / rows {:?}",
            m.get_tag_menu_question().as_str(),
            m.get_tag_menu_yes_label().as_str(),
            m.get_tag_menu_no_label().as_str(),
            rows(m)
        );
        m.invoke_tag_menu_answered(false);
        widgets::settle();
        enter(m, "striped");
        seen.insert("asked".into(), asked);
    };
    let seen = manage(&[file_a, file_b, file_c, file_d], &mixed, true);
    steps.push(step("mixed", seen));

    // 4. remove: striped selected on A, the remove button, yes
    let remove = |m: &ManageTagsWindow, seen: &mut serde_json::Map<String, Json>| {
        let at = rows(m)
            .iter()
            .position(|r| r.starts_with("striped ("))
            .unwrap();
        m.invoke_tag_clicked(i32::try_from(at).unwrap(), false, false);
        m.invoke_remove_pressed();
        widgets::settle();
        seen.insert(
            "asked".into(),
            json!([{"title": "yes/no", "message": m.get_tag_menu_question().as_str()}]),
        );
        m.invoke_tag_menu_answered(true);
        widgets::settle();
    };
    let seen = manage(&[file_a], &remove, true);
    steps.push(step("remove", seen));

    // 5. copy: all of A's tags copied (nothing selected), cancel; pasted
    // onto B with the paste button
    let copy = |m: &ManageTagsWindow, seen: &mut serde_json::Map<String, Json>| {
        m.invoke_copy_pressed();
        seen.insert("copied".into(), json!(clipboard.borrow().clone()));
    };
    let paste = |m: &ManageTagsWindow, _: &mut serde_json::Map<String, Json>| {
        m.invoke_paste_requested(true);
        widgets::settle();
    };
    let seen = json!({
        "A": manage(&[file_a], &copy, false),
        "B": manage(&[file_b], &paste, true),
        "asked": [],
    });
    steps.push(step("copy", seen));

    // 6. undo: what the undo menu offers; A and B sent to the inbox (asked
    // first); undo, then redo
    let mut seen = serde_json::Map::new();
    seen.insert("after tagging".into(), undo_strings(&ui));
    select(&[file_a, file_b]);
    ui.invoke_inbox_selected();
    let question = ui.get_question().to_string();
    ui.invoke_answer(true);
    widgets::settle();
    seen.insert(
        "asked".into(),
        json!([{"title": "yes/no", "message": question}]),
    );
    seen.insert("inboxed".into(), inbox());
    let undo = undo_strings(&ui);
    seen.insert("after inbox".into(), undo.clone());
    main_menu::choose(&ui, &["undo"], undo[0].as_str().expect("an undo item"));
    widgets::settle();
    seen.insert("undone".into(), inbox());
    let redo = undo_strings(&ui);
    seen.insert("after undo".into(), redo.clone());
    main_menu::choose(&ui, &["undo"], redo[1].as_str().expect("a redo item"));
    widgets::settle();
    seen.insert("redone".into(), inbox());
    seen.insert("after redo".into(), undo_strings(&ui));
    steps.push(step("undo", Json::Object(seen)));

    // 7. search: each tag on a page of its own (the four files among them)
    let mut searches = serde_json::Map::new();
    for text in ["cat", "kitty", "animal", "striped", "fluffy", "-fluffy"] {
        if text.starts_with('-') {
            search(&["system:everything", text]);
        } else {
            search(&[text]);
        }
        let ids = bound.current.borrow().borrow().results().to_vec();
        let found: BTreeSet<String> = store
            .read(|c| hydrus_store::master::hashes(c, &ids))
            .unwrap()
            .values()
            .map(|h| hex::encode(h.as_bytes()))
            .collect();
        let ours: Vec<&String> = files.iter().filter(|h| found.contains(*h)).collect();
        let mut ours: Vec<String> = ours.into_iter().cloned().collect();
        ours.sort();
        searches.insert(text.to_owned(), json!(ours));
    }
    let mut last = step("search", json!({}));
    last["searches"] = Json::Object(searches);
    steps.push(last);

    compare(&steps, &recording, KNOWN);
    let _: (&Store, &Bound) = (&store, &bound);
}

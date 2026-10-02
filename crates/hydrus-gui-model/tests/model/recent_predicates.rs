//! Recent system predicates against the reference's
//! (`oracle/record_recent_predicates.py`): kept as the reference keeps
//! them (each editor's additions, newest first, five of each type, one
//! added again moved to the front, one forgotten gone), shown in each
//! editor's pages as the reference shows them (its types', less those its
//! buttons add), and migrated from the reference's option. (The editor
//! window keeping and showing them: hydrus-gui's tests.)

use std::sync::Arc;

use serde_json::{Value as Json, json};

use hydrus_core::search::recent::RecentPredicates;
use hydrus_gui_model::predicate_editors::{Blank, Context, Editor, button_label};
use hydrus_search::{
    CivilDateTime, Predicate, SystemPredicate, TextContext, parse_system_predicate, predicate_text,
};
use hydrus_store::Store;
use hydrus_store::import::import_legacy;

fn store() -> ([tempfile::TempDir; 2], Arc<Store>) {
    let legacy = hydrus_testkit::legacy_fixture("basic");
    let native = tempfile::tempdir().unwrap();
    import_legacy(
        legacy.path(),
        &native.path().join(hydrus_store::store::DB_FILE_NAME),
    )
    .unwrap();
    let store = Store::open(native.path()).unwrap();
    ([legacy, native], store)
}

fn context(store: &Store) -> Context {
    Context::new(
        &store.snapshot().services,
        Vec::new(),
        CivilDateTime::new(2026, 1, 1, 0, 0).unwrap(),
    )
}

fn text_context(store: &Store) -> TextContext {
    let viewing = store.read(hydrus_store::settings::get).unwrap_or_default();
    TextContext::from_store(&store.snapshot().services, &viewing)
}

fn written(predicate: &SystemPredicate, context: &TextContext) -> String {
    predicate_text(&Predicate::System(predicate.clone()), context)
}

fn texts(predicates: &[SystemPredicate], context: &TextContext) -> Vec<String> {
    predicates.iter().map(|p| written(p, context)).collect()
}

/// The recent predicates as the recording has them: each type's (by its
/// number) that has any, as written.
fn shown(recent: &RecentPredicates, context: &TextContext) -> Json {
    recent
        .by_type
        .iter()
        .filter(|(_, kept)| !kept.is_empty())
        .map(|(kind, kept)| (kind.to_string(), json!(texts(kept, context))))
        .collect::<serde_json::Map<_, _>>()
        .into()
}

fn parse(text: &str) -> SystemPredicate {
    parse_system_predicate(text).unwrap_or_else(|e| panic!("{text}: {e:?}"))
}

/// What the editor for `blank` adds from the button labelled `label`.
fn button(editor: &Editor, label: &str, context: &TextContext) -> Vec<SystemPredicate> {
    editor
        .pages
        .iter()
        .flat_map(|page| &page.buttons)
        .find(|b| button_label(b, context) == label)
        .unwrap_or_else(|| panic!("no button {label}"))
        .predicates
        .clone()
}

/// What the editor adds from its page `page`'s panel `panel`, at its
/// defaults.
fn ok(editor: &Editor, page: &str, panel: usize, context: &Context) -> Vec<SystemPredicate> {
    editor
        .pages
        .iter()
        .find(|p| p.name == page)
        .unwrap_or_else(|| panic!("no page {page}"))
        .panels[panel]
        .predicates(context)
        .unwrap()
        .into_iter()
        .map(|p| match p {
            Predicate::System(s) => s,
            other => panic!("{other:?}"),
        })
        .collect()
}

/// Replay the recorded steps, checking what each editor adds and the
/// recent predicates after each step; the recent predicates after them.
fn replay(store: &Store, recorded: &Json) -> RecentPredicates {
    let context = context(store);
    let text = text_context(store);
    let mut recent = RecentPredicates::default();
    for step in recorded["steps"].as_array().unwrap() {
        let s = step["step"].as_array().unwrap();
        let what = s[0].as_str().unwrap();
        match what {
            "button" | "ok" => {
                let blank = Blank::from_text(s[1].as_str().unwrap()).unwrap();
                let editor = Editor::new(blank, &context);
                let added = if what == "button" {
                    button(&editor, s[2].as_str().unwrap(), &text)
                } else {
                    let panel = usize::try_from(s[3].as_u64().unwrap()).unwrap();
                    ok(&editor, s[2].as_str().unwrap(), panel, &context)
                };
                assert_eq!(json!(texts(&added, &text)), step["added"], "{s:?}");
                recent.push(&added);
            }
            "push" => {
                let pushed: Vec<SystemPredicate> = s[1]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|t| parse(t.as_str().unwrap()))
                    .collect();
                recent.push(&pushed);
            }
            _ => recent.remove(&parse(s[1].as_str().unwrap())),
        }
        assert_eq!(shown(&recent, &text), step["recent"], "after {s:?}");
    }
    recent
}

#[test]
fn recent_predicates_are_kept_as_the_reference_keeps_them() {
    let (_dirs, store) = store();
    let recorded = hydrus_testkit::fixture_json("recent_predicates.json");
    // a new client keeps none
    assert_eq!(recorded["new_client"], json!({}));
    let recent = replay(&store, &recorded);
    // (the recording keeps more than five widths once, and moves one to
    // the front)
    assert_eq!(recent.by_type[&13].len(), 4);
}

#[test]
fn each_editor_shows_its_pages_recent_predicates() {
    let (_dirs, store) = store();
    let recorded = hydrus_testkit::fixture_json("recent_predicates.json");
    let mut recent = replay(&store, &recorded);
    let context = context(&store);
    let text = text_context(&store);
    let mut forgot = 0;
    for shown_editor in recorded["editors"].as_array().unwrap() {
        let name = shown_editor["editor"].as_str().unwrap();
        let blank = Blank::from_text(name).unwrap();
        let editor = Editor::new(blank, &context);
        let ours: Vec<Json> = editor
            .pages
            .iter()
            .map(|p| json!({ "name": p.name, "recent": texts(&p.recent(&recent), &text) }))
            .collect();
        assert_eq!(Json::from(ours), shown_editor["pages"], "{name}");
        // the first editor showing any forgets its last, as the next show
        let Some(forgotten) = shown_editor["forgot"].as_object() else {
            continue;
        };
        let page = editor
            .pages
            .iter()
            .find(|p| p.name == forgotten["page"].as_str().unwrap())
            .unwrap();
        let predicate = page
            .recent(&recent)
            .into_iter()
            .find(|p| written(p, &text) == forgotten["predicate"].as_str().unwrap())
            .unwrap();
        recent.remove(&predicate);
        assert_eq!(
            json!(texts(&page.recent(&recent), &text)),
            forgotten["shown"],
            "{name}"
        );
        assert_eq!(shown(&recent, &text), forgotten["recent"], "{name}");
        forgot += 1;
    }
    assert!(forgot >= 5, "{forgot}");
}

/// The reference's option comes across: each predicate under its type,
/// newest first.
#[test]
fn the_reference_s_recent_predicates_come_across() {
    let (_dirs, store) = store();
    let recorded = hydrus_testkit::fixture_json("recent_predicates.json");
    let object = hydrus_legacy::serialisable::SerialisableObject::from_tuple_str(
        &recorded["stored"].to_string(),
    )
    .unwrap();
    let migrated = hydrus_legacy::objects::recent_predicates(&object, &|_| None).unwrap();
    let steps = recorded["steps"].as_array().unwrap();
    assert_eq!(
        shown(&migrated, &text_context(&store)),
        steps.last().unwrap()["recent"]
    );
    for (kind, kept) in &migrated.by_type {
        for predicate in kept {
            assert_eq!(predicate.reference_type(), *kind, "{predicate:?}");
        }
    }
}

/// Each recorded predicate's type is the reference's.
#[test]
fn predicates_have_the_reference_s_type_numbers() {
    let (_dirs, store) = store();
    let recorded = hydrus_testkit::fixture_json("recent_predicates.json");
    let text = text_context(&store);
    let mut checked = 0;
    for (as_written, kind) in recorded["types"].as_object().unwrap() {
        // (as written with thousands separators, which the parser takes
        // without)
        let Ok(predicate) = parse_system_predicate(&as_written.replace(',', "")) else {
            continue;
        };
        assert_eq!(
            u64::from(predicate.reference_type()),
            kind.as_u64().unwrap(),
            "{as_written} ({})",
            written(&predicate, &text)
        );
        checked += 1;
    }
    assert!(checked >= 20, "{checked}");
}

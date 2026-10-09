//! The tag siblings and parents windows, opened from the tags menu and
//! driven as a user would, replayed against the reference's
//! (`oracle/fixtures/tag_relationships.json`, from
//! `oracle/record_tag_relationships.py`), for a local tag service and a
//! repository: pairs entered and added, or a pair's row selected and
//! "delete" pressed, the questions (petition reasons among them) answered
//! with their buttons, and the rows (status, pair, reason) compared after
//! each step; then the workspace wiped to one tag and the "show all
//! pairs", "show pending and petitioned groups" and "show whole chains"
//! boxes clicked, the rows compared with the reference's for each.

use std::sync::Arc;

use serde_json::{Value, json};
use slint::{ComponentHandle as _, Model as _};

use hydrus_core::{ServiceKey, Tag};
use hydrus_gui::{MainWindow, Pages, SearchPage, TagRelationshipsWindow, bind, headless};
use hydrus_gui_model::tag_relationships::RelationKind;
use hydrus_store::Store;
use hydrus_store::content::tag_relations::{self, RelationAction, RelationUpdate};

use crate::common::{main_menu, widgets};

/// The window's rows: (status, left, right, note), in the window's order.
fn rows(w: &TagRelationshipsWindow) -> Vec<Value> {
    w.get_rows()
        .iter()
        .map(|r| {
            let cells: Vec<String> = r.cells.iter().map(|c| c.to_string()).collect();
            json!(cells)
        })
        .collect()
}

/// The rows' pairs, sorted.
fn pairs(w: &TagRelationshipsWindow) -> Vec<Value> {
    let mut pairs: Vec<Value> = rows(w).iter().map(|r| json!([r[1], r[2]])).collect();
    pairs.sort_by_key(Value::to_string);
    pairs
}

fn open(ui: &MainWindow, bound: &hydrus_gui::Bound, kind: &str) -> TagRelationshipsWindow {
    let lines = main_menu::open(ui, &["tags"]);
    let label = lines
        .iter()
        .find(|l| l.0.starts_with(kind))
        .unwrap_or_else(|| panic!("tags > {kind}"))
        .0
        .clone();
    main_menu::choose(ui, &["tags"], &label);
    bound
        .tag_relationships
        .borrow()
        .as_ref()
        .unwrap()
        .clone_strong()
}

/// Type a tag into a side's entry and, once its suggestions settle,
/// choose the tag itself among them (a tag with siblings shows as "a → c":
/// pressing Enter would take the highlighted ideal), else press Enter.
fn enter(w: &TagRelationshipsWindow, right: bool, tag: &str) {
    w.invoke_autocomplete_edited(right, tag.into());
    widgets::settle();
    let suggestions = if right {
        w.get_right_suggestions()
    } else {
        w.get_left_suggestions()
    };
    let exact = suggestions
        .iter()
        .position(|s| s.text == tag || s.text.starts_with(&format!("{tag} \u{2192} ")));
    match exact {
        Some(i) => w.invoke_autocomplete_chosen(right, i32::try_from(i).unwrap()),
        None => w.invoke_enter_tags(right, tag.into()),
    }
    let side = if right {
        w.get_right_tags()
    } else {
        w.get_left_tags()
    };
    assert!(
        side.iter().any(|t| t.text == tag),
        "{tag} entered: {:?}",
        side.iter().map(|t| t.text.to_string()).collect::<Vec<_>>()
    );
}

/// Answer what the window asks, in turn, with its yes button (a reason
/// typed first where it asks for one); what it asked.
fn answer(w: &TagRelationshipsWindow) -> Vec<Value> {
    let mut asked = Vec::new();
    while !w.get_question().is_empty() {
        asked.push(json!({
            "message": w.get_question().as_str(),
            "yes": w.get_yes_label().as_str(),
            "no": w.get_no_label().as_str(),
        }));
        if w.get_ask_reason() {
            w.set_reason("oracle reason".into());
        }
        let yes = w.get_yes_label().to_string();
        widgets::click(w.window(), &yes);
    }
    asked
}

/// Click a check box so that it shows `on`.
fn tick(w: &TagRelationshipsWindow, label: &str, on: bool) {
    if widgets::checked(w.window(), label) != on {
        widgets::click(w.window(), label);
    }
    assert_eq!(widgets::checked(w.window(), label), on, "{label}");
}

// leaf: audit-media-siblings-workspace
// leaf: audit-media-parents-workspace
// leaf: audit-media-parents-reasons
#[test]
fn the_relationship_windows_edit_and_filter_pairs_as_the_reference_s_do() {
    let recorded = hydrus_testkit::fixture_json("tag_relationships.json");
    let _windows = headless::init();
    for case in recorded.as_array().unwrap() {
        let siblings = case["kind"] == "siblings";
        let local = case["local"].as_bool().unwrap();
        let name = format!(
            "{} {}",
            case["kind"],
            if local { "local" } else { "repository" }
        );
        let dir = tempfile::tempdir().unwrap();
        let store: Arc<Store> = Store::open(dir.path()).unwrap();
        if !local {
            store
                .write_and_refresh(|ctx| {
                    hydrus_store::services::insert(
                        ctx.conn(),
                        &ServiceKey::new(vec![24; 16]),
                        "a repository",
                        &hydrus_store::services::ServiceKind::TagRepository(
                            hydrus_store::services::RepositoryConfig::default(),
                        ),
                    )?;
                    Ok(())
                })
                .unwrap();
        }
        let service_name = if local { "my tags" } else { "a repository" };
        let service = store.snapshot().services.by_name(service_name).unwrap().id;
        let kind = if siblings {
            RelationKind::Siblings
        } else {
            RelationKind::Parents
        };
        let initial: Vec<RelationUpdate> = case["initial"]
            .as_array()
            .unwrap()
            .iter()
            .map(|p| RelationUpdate {
                service,
                left: Tag::new(p[0].as_str().unwrap()).unwrap(),
                right: Tag::new(p[1].as_str().unwrap()).unwrap(),
                action: RelationAction::Add,
            })
            .collect();
        tag_relations::apply(&store, kind, initial).unwrap();

        let ui = MainWindow::new().unwrap();
        let bound = bind(&ui, Pages::single(SearchPage::new(store.clone())));
        let w = open(&ui, &bound, if siblings { "siblings" } else { "parents" });
        widgets::lay_out(w.window(), 1000.0, 800.0);
        let services: Vec<String> = w
            .get_service_names()
            .iter()
            .map(|s| s.to_string())
            .collect();
        let at = services.iter().position(|s| s == service_name).unwrap();
        w.invoke_service_chosen(i32::try_from(at).unwrap());
        tick(&w, "show all pairs", true);

        for step in case["steps"].as_array().unwrap() {
            let context = format!("{name} {}", step["step"]);
            let mut asked = Vec::new();
            if let Some(action) = step["step"].as_array() {
                let (left, right) = (
                    action[1][0].as_str().unwrap(),
                    action[1][1].as_str().unwrap(),
                );
                let row = rows(&w).iter().position(|r| r[1] == left && r[2] == right);
                // a pair shown: its row selected, "delete" pressed; else a
                // pair to add (or one not shown, which entering it adds,
                // as the reference's does)
                if let (true, Some(row)) = (action[0] == "delete", row) {
                    w.invoke_row_clicked(i32::try_from(row).unwrap(), false, false);
                    widgets::click(w.window(), "delete");
                } else {
                    enter(&w, false, left);
                    enter(&w, true, right);
                    widgets::click(w.window(), "add");
                }
                asked = answer(&w);
            }
            assert_eq!(json!(asked), step["asked"], "{context}");
            let mut ours = rows(&w);
            ours.sort_by_key(|r| (r[1].to_string(), r[2].to_string()));
            assert_eq!(json!(ours), step["rows"], "{context}");
        }

        // the workspace wiped to one tag, then the filters
        enter(&w, false, "cousin");
        widgets::click(w.window(), "wipe workspace");
        tick(&w, "show all pairs", false);
        let whole = if siblings { "true" } else { "false" };
        assert_eq!(json!(pairs(&w)), case["filtered"][whole], "{name}");
        tick(&w, "show pending and petitioned groups", true);
        assert_eq!(
            json!(pairs(&w)),
            case["filtered_pending"][whole],
            "{name} pending"
        );
        tick(&w, "show pending and petitioned groups", false);
        if siblings {
            // (siblings always show whole chains: there is no box)
            assert!(!widgets::shows(w.window(), "show whole chains"));
        } else {
            tick(&w, "show whole chains", true);
            assert_eq!(json!(pairs(&w)), case["filtered"]["true"], "{name} whole");
            tick(&w, "show pending and petitioned groups", true);
            assert_eq!(
                json!(pairs(&w)),
                case["filtered_pending"]["true"],
                "{name} whole, pending"
            );
        }
        w.invoke_cancel();
    }
}

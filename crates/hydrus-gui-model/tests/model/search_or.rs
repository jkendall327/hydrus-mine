use hydrus_gui_model::search_or::Construction;
use hydrus_search::{TextContext, enter_predicates, parse_api_search, predicate_text};
use serde_json::{Value, json};

#[test]
fn staged_or_replays_actual_read_broadcasts_rewind_cancel_and_single_unwrap() {
    let fixture: Value = hydrus_testkit::fixture_json("read_or.json");
    let text = TextContext::default();
    let mut draft = Construction::default();
    let mut search = Vec::new();
    for event in fixture["events"].as_array().unwrap() {
        match event["action"].as_str().unwrap() {
            "initial" => {}
            "broadcast" => {
                let chosen = parse_api_search(&json!([event["tag"]])).unwrap();
                let committed = draft.broadcast(chosen, event["shift"].as_bool().unwrap(), &text);
                enter_predicates(&mut search, &committed, &text);
            }
            "rewind" | "escape" => draft.rewind(),
            "cancel" => draft.cancel(),
            "commit_draft" => {
                let chosen = draft.predicate().unwrap();
                let committed = draft.broadcast(vec![chosen], false, &text);
                enter_predicates(&mut search, &committed, &text);
            }
            unknown => panic!("unknown event {unknown}"),
        }
        let terms = draft.terms().map(|terms| {
            terms
                .iter()
                .map(|p| predicate_text(p, &text))
                .collect::<Vec<_>>()
        });
        assert_eq!(json!(terms), event["draft"], "{event}");
        let label = draft.predicate().map(|p| predicate_text(&p, &text));
        assert_eq!(json!(label), event["draft_label"], "{event}");
        let mut shown: Vec<_> = search.iter().map(|p| predicate_text(p, &text)).collect();
        shown.sort();
        assert_eq!(json!(shown), event["predicates"], "{event}");
    }
}

#[test]
fn advanced_boolean_preview_replays_all_operators_escapes_and_real_validation() {
    let fixture: Value = hydrus_testkit::fixture_json("read_or_editors.json");
    let text = TextContext::default();
    for case in fixture["cases"].as_array().unwrap() {
        let input = case["input"].as_str().unwrap();
        let (preview, valid) = hydrus_gui_model::search_or::advanced::preview(input, &text);
        assert_eq!(preview, case["preview"].as_str().unwrap(), "{case}");
        assert_eq!(valid, case["valid"] == "HydrusValid", "{case}");
        if valid {
            let predicates =
                hydrus_gui_model::search_or::advanced::predicates(input, &text).unwrap();
            let mut shown: Vec<_> = predicates
                .iter()
                .map(|p| predicate_text(p, &text))
                .collect();
            shown.sort();
            assert_eq!(json!(shown), case["predicates"], "{case}");
        }
    }
}

//! The "filename tagging" dialog's rows and value against the reference's,
//! recorded by `oracle/record_filename_tagging.py`: for each case of
//! settings on the "my tags" tab, each path's tags and what the import
//! gets.

use serde_json::{Value as Json, json};

use hydrus_gui_model::filename_tagging::{ServiceTagging, row, value};

fn recorded() -> Json {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../oracle/fixtures/filename_tagging.json"
    );
    serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap()
}

fn strings(v: &Json) -> Vec<String> {
    v.as_array()
        .into_iter()
        .flatten()
        .map(|s| s.as_str().unwrap().to_owned())
        .collect()
}

fn tagging(case: &Json) -> ServiceTagging {
    let mut t = ServiceTagging::default();
    t.options.tags_for_all = strings(&case["all"]).into_iter().collect();
    t.options.add_filename = case.get("filename").map(|n| n.as_str().unwrap().to_owned());
    // (the reference's boxes in order: first, second, third, third last,
    // second last, last)
    let mut directories: Vec<(i64, String)> = case["directories"]
        .as_object()
        .into_iter()
        .flatten()
        .map(|(i, n)| (i.parse().unwrap(), n.as_str().unwrap().to_owned()))
        .collect();
    let order = [0, 1, 2, -3, -2, -1];
    directories.sort_by_key(|(i, _)| order.iter().position(|o| o == i));
    t.options.directories = directories;
    t.options.quick_namespaces = case["quick"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|q| {
            (
                q[0].as_str().unwrap().to_owned(),
                q[1].as_str().unwrap().to_owned(),
            )
        })
        .collect();
    t.options.regexes = strings(&case["regexes"]);
    if let Some(n) = case.get("number") {
        t.number_base = n[0].as_i64().unwrap();
        t.number_step = n[1].as_i64().unwrap();
        n[2].as_str().unwrap().clone_into(&mut t.number_namespace);
    }
    for (i, tags) in case["single"].as_object().into_iter().flatten() {
        t.add_single(&[i.parse().unwrap()], &strings(tags));
    }
    t
}

/// A recorded (POSIX) path as this platform writes it: the reference
/// splits directories on its own separator, so on Windows it would be
/// given these paths with backslashes.
fn native(path: &str) -> String {
    if cfg!(windows) {
        path.replace('/', "\\")
    } else {
        path.to_owned()
    }
}

/// A recorded value, its paths as this platform writes them.
fn native_json(value: &Json) -> Json {
    match value {
        Json::Object(map) => {
            Json::Object(map.iter().map(|(k, v)| (native(k), v.clone())).collect())
        }
        other => other.clone(),
    }
}

#[test]
fn rows_and_values_are_the_references() {
    let recorded = recorded();
    let paths: Vec<String> = strings(&recorded["paths"])
        .iter()
        .map(|p| native(p))
        .collect();
    for case in recorded["cases"].as_array().unwrap() {
        let settings = &case["case"];
        let mine = tagging(settings);
        let rows: Vec<[String; 3]> = paths
            .iter()
            .enumerate()
            .map(|(i, p)| row(i, p, &mine.tags(i, p)))
            .collect();
        let expected: Vec<Json> = case["rows"]
            .as_array()
            .unwrap()
            .iter()
            .map(|r| json!([r[0], native(r[1].as_str().unwrap()), r[2]]))
            .collect();
        assert_eq!(json!(rows), Json::Array(expected), "{settings}");
        // (the other services' tabs, untouched, give nothing)
        let services = vec![
            ("downloader tags".to_owned(), ServiceTagging::default()),
            ("my tags".to_owned(), mine),
        ];
        let ours: serde_json::Map<String, Json> = value(&services, &paths)
            .into_iter()
            .map(|(path, tags)| {
                let by_service: serde_json::Map<String, Json> = tags
                    .into_iter()
                    .map(|(service, tags)| (service.to_owned(), json!(tags)))
                    .collect();
                (path.to_owned(), Json::Object(by_service))
            })
            .collect();
        assert_eq!(
            Json::Object(ours),
            native_json(&case["value"]),
            "{settings}"
        );
    }
}

#[test]
fn single_tags_are_shown_and_removed_for_the_selection() {
    let mut t = ServiceTagging::default();
    t.add_single(&[0, 1], &["a".into(), "B".into()]);
    t.add_single(&[1], &["c".into()]);
    assert_eq!(t.selected_single(&[0]), ["a", "b"]);
    assert_eq!(t.selected_single(&[0, 1]), ["a", "b", "c"]);
    t.remove_single(&[0, 1], &["b".into()]);
    assert_eq!(t.selected_single(&[0, 1]), ["a", "c"]);
}

#[test]
fn typed_regexes_and_quick_namespaces_are_checked() {
    use hydrus_gui_model::filename_tagging::{
        parse_quick_namespaces, parse_regexes, quick_namespaces_text,
    };
    let (good, errors) = parse_regexes("(\\d+)\n\n[unclosed\n");
    assert_eq!(good, ["(\\d+)"]);
    assert_eq!(errors.len(), 1);
    assert!(errors[0].starts_with("That regex would not compile!\n\n"));
    let (good, errors) = parse_quick_namespaces("page:(?<=page )\\d+\n:x\nbad:(\n");
    assert_eq!(good, [("page".to_owned(), "(?<=page )\\d+".to_owned())]);
    assert_eq!(errors[0], "Please enter something for the namespace.");
    assert!(errors[1].starts_with("That regex would not compile!"));
    assert_eq!(quick_namespaces_text(&good), "page:(?<=page )\\d+");
}

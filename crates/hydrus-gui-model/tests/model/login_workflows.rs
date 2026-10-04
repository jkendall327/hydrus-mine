//! Reference Qt credential rows, advisory prompts, script identities and storage.
use hydrus_gui_model::login_workflows::{CredentialsEditor, ScriptsEditor};
use hydrus_legacy::{objects::logins as legacy, serialisable::SerialisableObject};
use hydrus_parse::login::CredentialKind;
use serde_json::{Value, json};
fn manager(fixture: &Value) -> hydrus_parse::login::LoginManager {
    legacy::manager(&SerialisableObject::from_tuple_str(&fixture["manager"].to_string()).unwrap())
        .unwrap()
}
fn rows(editor: &CredentialsEditor) -> Value {
    json!(editor.rows().into_iter().map(|row| json!({"name":row.definition.name,"value":row.value,"hidden":row.definition.kind==CredentialKind::Hidden,"label":row.label,"valid":row.valid})).collect::<Vec<_>>())
}
#[test]
fn credentials_match_reference_display_order_status_and_confirmation() {
    let fixture = hydrus_testkit::fixture_json("login_editors.json");
    let script = manager(&fixture).scripts.remove(0);
    let states = fixture["credentials"].as_array().unwrap();
    let initial = states[0]["state"]
        .as_array()
        .unwrap()
        .iter()
        .map(|row| {
            (
                row["name"].as_str().unwrap().to_owned(),
                row["value"].as_str().unwrap().to_owned(),
            )
        })
        .collect();
    let mut editor = CredentialsEditor::new(
        &script.credentials.iter().rev().cloned().collect::<Vec<_>>(),
        &initial,
    );
    assert_eq!(rows(&editor), states[0]["state"]);
    for state in states.iter().skip(1) {
        let names = editor
            .rows()
            .into_iter()
            .map(|row| row.definition.name)
            .collect::<Vec<_>>();
        for (i, name) in names.iter().enumerate() {
            editor.set(i, state["do"][name].as_str().unwrap().to_owned());
        }
        assert_eq!(rows(&editor), state["state"]);
        assert_eq!(json!(editor.value()), state["value"]);
        assert_eq!(
            json!(editor.warning().into_iter().collect::<Vec<_>>()),
            state["questions"]
        );
        assert_eq!(
            editor.warning().is_none() || state["answer"] == true,
            state["accepted"] == true
        );
    }
}
#[test]
fn scripts_import_rename_and_delete_keep_reference_name_and_key_rules() {
    let fixture = hydrus_testkit::fixture_json("login_editors.json");
    let initial = manager(&fixture);
    let original_key = initial.scripts[0].key.clone();
    let mut editor = ScriptsEditor::new(initial);
    editor.import(&fixture["script"].to_string()).unwrap();
    let names = editor
        .order()
        .into_iter()
        .map(|i| editor.draft.scripts[i].name.clone())
        .collect::<Vec<_>>();
    assert_eq!(json!(names), fixture["script_list"][1]["state"]["names"]);
    assert_ne!(editor.draft.scripts[1].key, original_key);
    let added_key = editor.draft.scripts[1].key.clone();
    let mut renamed = editor.draft.scripts[1].clone();
    renamed.name = "renamed script".into();
    editor.put(Some(1), renamed);
    let names = editor
        .order()
        .into_iter()
        .map(|i| editor.draft.scripts[i].name.clone())
        .collect::<Vec<_>>();
    assert_eq!(json!(names), fixture["script_list"][2]["state"]["names"]);
    assert_eq!(editor.draft.scripts[1].key, added_key);
    let before = editor.draft.clone();
    assert!(editor.import("invalid").is_err());
    assert_eq!(editor.draft, before);
    editor.click(0, false, false);
    editor.click(1, true, false);
    editor.delete();
    assert!(editor.draft.scripts.is_empty());
    assert_eq!(editor.draft.domains, before.domains);
}
#[test]
fn preserved_credentials_load_then_native_save_controls_domain_warnings() {
    let fixture = hydrus_testkit::fixture_json("login_editors.json");
    let conn = rusqlite::Connection::open_in_memory().unwrap();
    conn.execute_batch("CREATE TABLE settings(key TEXT PRIMARY KEY, value TEXT); CREATE TABLE legacy_objects(source TEXT, type_id INTEGER, version INTEGER, dump TEXT);").unwrap();
    conn.execute(
        "INSERT INTO legacy_objects VALUES ('json_dumps',48,1,?)",
        [fixture["manager"][2].to_string()],
    )
    .unwrap();
    let mut value = hydrus_store::logins::load(&conn).unwrap();
    assert_eq!(value, manager(&fixture));
    assert_eq!(
        value.domains["login.example"].credentials["password"],
        "dummy-pass"
    );
    value.domains.get_mut("login.example").unwrap().active = false;
    hydrus_store::logins::save(&conn, &value).unwrap();
    assert_eq!(hydrus_store::logins::load(&conn).unwrap(), value);
    assert!(
        hydrus_store::settings::get::<hydrus_store::network::LoginDomains>(&conn)
            .unwrap()
            .0
            .is_empty()
    );
}

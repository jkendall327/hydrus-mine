//! Reference Qt credential rows, advisory prompts, script identities and storage.
use hydrus_gui_model::login_workflows::{
    ArgumentKind, CookiesEditor, CredentialsEditor, ScriptsEditor, StepEditor,
};
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

#[test]
fn login_step_content_replays_reference_unique_import_cancel_sort_and_request_cleanup() {
    use hydrus_downloader_exchange::{Definition, Native};
    use hydrus_parse::content::ContentKind;
    let fixture = hydrus_testkit::fixture_json("login_editors.json");
    let states = fixture["step_states"].as_array().unwrap();
    let decode = |i: usize| {
        legacy::login_step(
            &SerialisableObject::from_tuple_str(&states[i]["state"]["value"].to_string()).unwrap(),
        )
        .unwrap()
    };
    let mut editor = StepEditor::new(&decode(0));
    assert_eq!(
        hydrus_downloader_exchange::logins::step_tuple(&editor.value()).unwrap(),
        states[0]["state"]["value"]
    );
    let mut parser = editor.step.content_parsers[0].clone();
    parser.name = "renamed response".into();
    parser.kind = ContentKind::Variable {
        name: "token".into(),
    };
    editor.put(Some(0), parser.clone()).unwrap();
    assert_eq!(
        hydrus_downloader_exchange::logins::step_tuple(&editor.value()).unwrap(),
        states[1]["state"]["value"]
    );
    editor
        .import(vec![Definition::new(Native::Content(parser))])
        .unwrap();
    assert_eq!(
        hydrus_downloader_exchange::logins::step_tuple(&editor.value()).unwrap(),
        states[2]["state"]["value"]
    );
    assert_eq!(states[2]["state"], states[3]["state"]);
    let before = editor.value();
    let invalid = hydrus_gui_model::parser_editors::new_content();
    let allowed = editor.step.content_parsers[0].clone();
    assert!(
        editor
            .import(vec![
                Definition::new(Native::Content(allowed)),
                Definition::new(Native::Content(invalid))
            ])
            .is_err()
    );
    assert_eq!(editor.value(), before, "mixed imports must remain atomic");
    let veto = decode(4).content_parsers.remove(0);
    editor.put(None, veto).unwrap();
    assert_eq!(
        hydrus_downloader_exchange::logins::step_tuple(&editor.value()).unwrap(),
        states[4]["state"]["value"]
    );
    editor.step.name = "edited request".into();
    editor.step.scheme = "https".into();
    editor.step.method = "POST".into();
    editor.step.subdomain = None;
    editor.step.path = "signin".into();
    assert_eq!(
        hydrus_downloader_exchange::logins::step_tuple(&editor.value()).unwrap(),
        states[5]["state"]["value"]
    );
    let order = editor.order();
    editor.selection.click(&order, 0, false, false);
    editor.selection.click(&order, 2, true, false);
    editor.delete();
    assert_eq!(editor.value().content_parsers.len(), 1);
    assert_eq!(editor.value().content_parsers[0].name, "renamed response");
}

#[test]
fn domain_credentials_replay_reference_validity_activation_cancel_and_delay_reset() {
    use hydrus_gui_model::login_workflows::DomainsEditor;
    let fixture = hydrus_testkit::fixture_json("login_editors.json");
    let mut editor = DomainsEditor::new(manager(&fixture));
    let states = fixture["domain_states"].as_array().unwrap();
    for state in states.iter().skip(1) {
        if state["accepted"] == true {
            let values = serde_json::from_value(state["do"].clone()).unwrap();
            let activation = editor.replace_credentials("login.example", values).unwrap();
            if activation {
                assert_eq!(
                    state["questions"],
                    json!(["Activate this login script for this domain?"])
                );
                editor
                    .draft
                    .domains
                    .get_mut("login.example")
                    .unwrap()
                    .active = true;
            }
        }
        let mut expected = fixture.clone();
        expected["manager"][2][1] = state["state"]["value"].clone();
        assert_eq!(editor.draft, manager(&expected));
    }
    let login = editor.draft.domains.get_mut("login.example").unwrap();
    login.no_work_until = 4_000_000_000;
    login.delay_reason = "synthetic wait".into();
    editor
        .replace_credentials(
            "login.example",
            [
                ("username".into(), "alice".into()),
                ("password".into(), "dummy-pass".into()),
            ]
            .into(),
        )
        .unwrap();
    assert_eq!(editor.draft.domains["login.example"].no_work_until, 0);
    assert!(
        editor.draft.domains["login.example"]
            .delay_reason
            .is_empty()
    );
}

#[test]
fn domain_apply_preserves_concurrent_script_edits_and_rejects_domain_conflicts() {
    use hydrus_gui_model::login_workflows::DomainsEditor;
    let fixture = hydrus_testkit::fixture_json("login_editors.json");
    let initial = manager(&fixture);
    let dir = tempfile::tempdir().unwrap();
    let store = hydrus_store::Store::open(dir.path()).unwrap();
    let saved = initial.clone();
    store
        .write_and_refresh(move |ctx| hydrus_store::logins::save(ctx.conn(), &saved))
        .unwrap();
    let mut editor = DomainsEditor::new(initial.clone());
    editor
        .draft
        .domains
        .get_mut("login.example")
        .unwrap()
        .active = false;
    store
        .write_and_refresh(|ctx| {
            let mut current = hydrus_store::logins::load(ctx.conn())?;
            current.scripts[0].name = "concurrent script".into();
            hydrus_store::logins::save(ctx.conn(), &current)
        })
        .unwrap();
    editor.save(&store).unwrap();
    let saved = store.read(hydrus_store::logins::load).unwrap();
    assert_eq!(saved.scripts[0].name, "concurrent script");
    assert!(!saved.domains["login.example"].active);
    assert!(
        editor.save(&store).is_err(),
        "an old domain draft must not overwrite a newer save"
    );
    assert_eq!(store.read(hydrus_store::logins::load).unwrap(), saved);
}

#[test]
fn request_arguments_replay_real_qt_rename_duplicates_blank_values_and_cancel() {
    let fixture = hydrus_testkit::fixture_json("login_editors.json");
    let states = fixture["argument_states"].as_array().unwrap();
    let step = legacy::login_step(
        &SerialisableObject::from_tuple_str(&states[0]["state"].to_string()).unwrap(),
    )
    .unwrap();
    let mut editor = StepEditor::new(&step);
    for state in &states[1..] {
        let kind = match state["kind"].as_str().unwrap() {
            "credential" => ArgumentKind::Credential,
            "temporary" => ArgumentKind::Temporary,
            _ => ArgumentKind::Static,
        };
        let values = state["answers"].as_array().unwrap();
        if values.len() == 1 {
            assert_eq!(
                editor
                    .set_argument(
                        kind,
                        None,
                        values[0].as_str().unwrap().to_owned(),
                        String::new()
                    )
                    .unwrap_err(),
                state["prompts"][1]["warning"].as_str().unwrap()
            );
        } else if let Some(value) = values[1].as_str() {
            let old = if state["action"] == "edit" {
                editor.arguments(kind).keys().next().cloned()
            } else {
                None
            };
            editor
                .set_argument(
                    kind,
                    old.as_deref(),
                    values[0].as_str().unwrap().to_owned(),
                    value.to_owned(),
                )
                .unwrap();
        }
        assert_eq!(
            hydrus_downloader_exchange::logins::step_tuple(&editor.value()).unwrap(),
            state["state"]
        );
    }
    let before = editor.value();
    assert!(
        editor
            .set_argument(
                ArgumentKind::Static,
                Some("lang"),
                String::new(),
                "bad".into()
            )
            .is_err()
    );
    assert_eq!(editor.value(), before);
    editor.remove_argument(ArgumentKind::Static, "empty");
    assert!(!editor.step.static_args.contains_key("empty"));
    assert_eq!(editor.step.credentials["account"], "account_param");
    assert_eq!(editor.step.temp_args["csrf"], "token");
}

#[test]
fn cookie_requirements_match_real_qt_pair_edits_cancel_and_duplicate_looking_keys() {
    let fixture = hydrus_testkit::fixture_json("login_editors.json");
    let mut script = manager(&fixture).scripts.remove(0);
    let mut editor = CookiesEditor::new(&script.required_cookies);
    for state in &fixture["cookie_states"].as_array().unwrap()[1..] {
        let values = state["answers"].as_array().unwrap();
        if let Some(value) = values[1].as_str() {
            let index = if state["action"] == "edit" {
                editor.rows.iter().position(|row| {
                    row.name == hydrus_core::url::strings::StringMatch::fixed("token")
                })
            } else {
                None
            };
            editor.put(
                index,
                hydrus_parse::login::CookieRequirement {
                    name: hydrus_core::url::strings::StringMatch::fixed(
                        values[0].as_str().unwrap(),
                    ),
                    value: hydrus_core::url::strings::StringMatch::fixed(value),
                    reference_auxiliary: None,
                },
            );
        }
        script.required_cookies = editor.value();
        assert_eq!(
            hydrus_downloader_exchange::logins::script_tuple(&script).unwrap()[3][1],
            state["state"]["value"]
        );
        assert_eq!(
            json!(
                editor
                    .value()
                    .iter()
                    .map(|row| vec![
                        row.name.describe(false, false),
                        row.value.describe(false, false)
                    ])
                    .collect::<Vec<_>>()
            ),
            state["state"]["rows"]
        );
    }
    assert_eq!(
        editor.rows.len(),
        3,
        "independent Python matcher objects may look identical"
    );
    let order = editor.order();
    editor.selection.select_many(&order[..2]);
    editor.delete();
    assert_eq!(editor.rows.len(), 1);
    assert!(editor.selection.is_empty());
}

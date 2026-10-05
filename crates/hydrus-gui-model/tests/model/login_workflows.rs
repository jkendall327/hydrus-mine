//! Reference Qt credential rows, advisory prompts, script identities and storage.
use hydrus_gui_model::login_workflows::{
    ArgumentKind, CookiesEditor, CredentialsEditor, ExampleDraft, ScriptsEditor, StepEditor,
};
use hydrus_legacy::{objects::logins as legacy, serialisable::SerialisableObject};
use hydrus_parse::login::CredentialKind;
use serde_json::{Value, json};
use std::collections::BTreeMap;
fn fixed_editor_value(
    original: &hydrus_core::url::strings::StringMatch,
    text: &str,
) -> hydrus_core::url::strings::StringMatch {
    let mut child = hydrus_gui_model::string_editors::MatchEditor::new(original);
    child.set_type(1);
    text.clone_into(&mut child.fixed);
    child.value().expect("the recorded fixed matcher is valid")
}
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
        let old = if state["action"] == "edit" {
            editor.arguments(kind).keys().next().cloned()
        } else {
            None
        };
        let warning = state["prompts"]
            .as_array()
            .unwrap()
            .iter()
            .find_map(|prompt| prompt["warning"].as_str());
        if let Some(warning) = warning {
            assert_eq!(
                editor
                    .set_argument(
                        kind,
                        old.as_deref(),
                        values[0].as_str().unwrap().to_owned(),
                        String::new()
                    )
                    .unwrap_err(),
                warning
            );
        } else if let (Some(key), Some(value)) = (
            values[0].as_str().filter(|key| !key.is_empty()),
            values.get(1).and_then(serde_json::Value::as_str),
        ) {
            editor
                .set_argument(kind, old.as_deref(), key.to_owned(), value.to_owned())
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
        if let Some(value) = values.get(1).and_then(serde_json::Value::as_str) {
            let index = if state["action"] == "edit" {
                editor.rows.iter().position(|row| {
                    row.name.kind == hydrus_core::url::strings::MatchKind::Fixed("token".into())
                })
            } else {
                None
            };
            let mut cookie = index.map_or_else(
                || hydrus_parse::login::CookieRequirement {
                    name: hydrus_core::url::strings::StringMatch::any(),
                    value: hydrus_core::url::strings::StringMatch::any(),
                    reference_auxiliary: None,
                },
                |i| editor.rows[i].clone(),
            );
            cookie.name = fixed_editor_value(&cookie.name, values[0].as_str().unwrap());
            cookie.value = fixed_editor_value(&cookie.value, value);
            cookie.reference_auxiliary = None;
            editor.put(index, cookie);
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

#[test]
fn embedded_step_cookie_list_replays_sorted_pair_actions_and_confirmed_bulk_delete() {
    let fixture = hydrus_testkit::fixture_json("login_step_cookies.json");
    let original = legacy::login_step(
        &SerialisableObject::from_tuple_str(&fixture["original"].to_string()).unwrap(),
    )
    .unwrap();
    let mut editor = StepEditor::new(&original);
    let states = fixture["states"].as_array().unwrap();
    for state in &states[1..] {
        let action = state["action"].as_str().unwrap();
        if action == "delete" {
            if !state["answer"].as_bool().unwrap() {
                let order = editor.cookies.order();
                editor.cookies.selection.click(&order, 0, false, false);
                editor.cookies.selection.click(&order, 1, true, false);
            } else {
                editor.cookies.delete();
            }
            assert_eq!(state["questions"][0], "Remove all selected?");
        } else {
            let index = if action == "edit" {
                editor.cookies.order().last().copied()
            } else {
                None
            };
            if let Some(index) = index {
                editor.cookies.selection.select_only(Some(index));
            }
            let answers = state["answers"].as_array().unwrap();
            if let Some(value) = answers.get(1).and_then(Value::as_str) {
                let mut cookie = index.map_or_else(
                    || hydrus_parse::login::CookieRequirement {
                        name: hydrus_core::url::strings::StringMatch::any(),
                        value: hydrus_core::url::strings::StringMatch::any(),
                        reference_auxiliary: None,
                    },
                    |i| editor.cookies.rows[i].clone(),
                );
                // Replay the real matcher child: fixed text becomes its example
                // and clears length limits, as the recorded Qt GetValue does.
                cookie.name = fixed_editor_value(&cookie.name, answers[0].as_str().unwrap());
                cookie.value = fixed_editor_value(&cookie.value, value);
                cookie.reference_auxiliary = None;
                editor.cookies.put(index, cookie);
            }
        }
        assert_eq!(
            hydrus_downloader_exchange::logins::step_tuple(&editor.value()).unwrap(),
            state["state"]["value"]
        );
        assert_eq!(
            json!(
                editor
                    .cookies
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
        assert_eq!(
            hydrus_downloader_exchange::logins::step_tuple(&original).unwrap(),
            fixture["original"]
        );
    }
    assert_eq!(editor.cookies.rows.len(), 1);
    assert_eq!(original.required_cookies.len(), 1);
}

#[test]
fn three_argument_lists_keep_independent_extended_selection_and_bulk_deletion() {
    let fixture = hydrus_testkit::fixture_json("login_editors.json");
    let state = fixture["argument_states"]
        .as_array()
        .unwrap()
        .last()
        .unwrap();
    let step = legacy::login_step(
        &SerialisableObject::from_tuple_str(&state["state"].to_string()).unwrap(),
    )
    .unwrap();
    let mut editor = StepEditor::new(&step);
    editor.select_arguments(ArgumentKind::Credential, 0, false, false);
    editor.select_arguments(ArgumentKind::Static, 0, false, false);
    editor.select_arguments(ArgumentKind::Static, 1, true, false);
    editor.select_arguments(ArgumentKind::Temporary, 0, false, false);
    assert_eq!(
        editor.selected_arguments(ArgumentKind::Credential),
        ["account"]
    );
    assert_eq!(
        editor.selected_arguments(ArgumentKind::Static),
        ["empty", "lang"]
    );
    assert_eq!(editor.selected_arguments(ArgumentKind::Temporary), ["csrf"]);
    editor.delete_arguments(ArgumentKind::Static);
    assert!(editor.step.static_args.is_empty());
    assert_eq!(
        editor.selected_arguments(ArgumentKind::Credential),
        ["account"]
    );
    assert_eq!(editor.selected_arguments(ArgumentKind::Temporary), ["csrf"]);
    assert_eq!(editor.step.credentials, step.credentials);
    assert_eq!(editor.step.temp_args, step.temp_args);
}

#[test]
fn example_domains_replay_reference_defaults_duplicate_errors_and_final_cancel_acceptance() {
    let fixture = hydrus_testkit::fixture_json("login_editors.json");
    let mut rows = manager(&fixture).scripts.remove(0).examples;
    for state in &fixture["example_states"].as_array().unwrap()[1..] {
        let texts = state["texts"].as_array().unwrap();
        if let Some(domain) = texts[0].as_str() {
            let index = if state["action"] == "edit" {
                rows.iter().position(|row| {
                    row.domain == "another.example" || row.domain == "renamed.example"
                })
            } else {
                None
            };
            let mut draft = ExampleDraft::new(index.map(|i| &rows[i]));
            draft.domain = domain.to_owned();
            match draft.validate_domain(&rows, index) {
                Err(error) => assert_eq!(error, state["prompts"][1]["warning"].as_str().unwrap()),
                Ok(()) => {
                    if let Some(access) = state["access"].as_i64() {
                        draft
                            .select_access(hydrus_parse::login::Access::from_code(access).unwrap());
                        let row = draft.value(texts[1].as_str()).unwrap();
                        if let Some(index) = index {
                            rows[index] = row;
                        } else {
                            rows.push(row);
                        }
                    }
                }
            }
        }
        rows.sort_by_cached_key(|row| {
            (
                row.domain.clone(),
                row.access.label(),
                row.description.clone(),
            )
        });
        assert_eq!(
            json!(
                rows.iter()
                    .map(|row| json!([row.domain, row.access.code(), row.description]))
                    .collect::<Vec<_>>()
            ),
            state["state"]["value"]
        );
        assert_eq!(
            json!(
                rows.iter()
                    .map(|row| json!([row.domain, row.access.label(), row.description]))
                    .collect::<Vec<_>>()
            ),
            state["state"]["rows"]
        );
    }
    let mut draft = ExampleDraft::new(None);
    assert_eq!(draft.domain, "example.com");
    assert_eq!(draft.access, hydrus_parse::login::Access::Nsfw);
    draft.domain.clear();
    assert!(draft.validate_domain(&rows, None).is_err());
    draft.select_access(hydrus_parse::login::Access::UserPreferences);
    assert_eq!(
        draft.value(None).unwrap().description,
        fixture["access_types"][3][2].as_str().unwrap()
    );
    assert!(draft.value(Some("")).is_err());
}

#[test]
fn login_domain_rows_replay_cookie_expiry_and_pretty_delay_reference() {
    use hydrus_gui_model::login_workflows::domain_cells;
    use hydrus_parse::login::{Access, DomainLogin, Validity};
    let fixture = hydrus_testkit::fixture_json("login_sessions.json");
    let script = legacy::login_script(
        &SerialisableObject::from_tuple_str(&fixture["script"].to_string()).unwrap(),
    )
    .unwrap();
    let now = fixture["now"].as_i64().unwrap();
    let login = DomainLogin {
        script_key: script.key.clone(),
        script_name: script.name.clone(),
        credentials: BTreeMap::new(),
        access: Access::Everything,
        description: "synthetic fixture".into(),
        active: true,
        validity: Validity::Untested,
        validity_error: String::new(),
        no_work_until: now + 3600,
        delay_reason: "synthetic delay".into(),
    };
    for step in fixture["states"].as_array().unwrap() {
        let state = &step["state"];
        assert_eq!(
            json!(domain_cells(
                "login.example",
                &login,
                Some(&script),
                state["logged_in"].as_bool().unwrap(),
                state["expiry"].as_i64(),
                now
            )),
            state["cells"]
        );
    }
    let mut inactive = login.clone();
    inactive.active = false;
    inactive.validity = Validity::Invalid;
    inactive.validity_error = "synthetic error".into();
    let cells = domain_cells("login.example", &inactive, Some(&script), true, None, now);
    assert_eq!(cells[3], "no");
    assert_eq!(cells[4], "yes - session");
    assert!(cells[5].is_empty());
    let cells = domain_cells("login.example", &login, None, false, None, now + 3601);
    assert_eq!(cells[1], "login script not found");
    assert!(cells[6].is_empty());
}

fn domain_reference_manager(fixture: &Value, domains: &Value) -> hydrus_parse::login::LoginManager {
    use hydrus_parse::login::{Access, DomainLogin, LoginManager, Validity};
    LoginManager {
        scripts: fixture["scripts"]
            .as_array()
            .unwrap()
            .iter()
            .map(|row| {
                legacy::login_script(&SerialisableObject::from_tuple_str(&row.to_string()).unwrap())
                    .unwrap()
            })
            .collect(),
        domains: domains
            .as_object()
            .unwrap()
            .iter()
            .map(|(name, row)| {
                (
                    name.clone(),
                    DomainLogin {
                        script_key: row[0][0].as_str().unwrap().into(),
                        script_name: row[0][1].as_str().unwrap().into(),
                        credentials: serde_json::from_value(row[1].clone()).unwrap(),
                        access: Access::from_code(row[2].as_i64().unwrap()).unwrap(),
                        description: row[3].as_str().unwrap().into(),
                        active: row[4].as_bool().unwrap(),
                        validity: Validity::from_code(row[5].as_i64().unwrap()).unwrap(),
                        validity_error: row[6].as_str().unwrap().into(),
                        no_work_until: row[7].as_i64().unwrap(),
                        delay_reason: row[8].as_str().unwrap().into(),
                    },
                )
            })
            .collect(),
    }
}
#[test]
fn domain_add_change_and_delete_replay_all_recorded_prompt_chains() {
    use hydrus_gui_model::login_workflows::{
        DomainEntry, DomainEntryStage as Stage, DomainsEditor,
    };
    let fixture = hydrus_testkit::fixture_json("login_domains.json");
    for case in fixture["cases"].as_array().unwrap() {
        let mut manager = domain_reference_manager(&fixture, &case["before"]);
        let expected = domain_reference_manager(&fixture, &case["after"]);
        if case["name"] == "no-scripts" {
            manager.scripts.clear();
        }
        let mut editor = DomainsEditor::new(manager.clone());
        let action = case["action"].as_str().unwrap();
        if action == "delete" {
            editor.selection.select_only(Some(0));
            if case["answers"][0] == true {
                editor.delete();
            }
        } else {
            let mut errors = Vec::<String>::new();
            match DomainEntry::new(
                &manager,
                action.starts_with("change").then_some("login.example"),
            ) {
                Err(error) => errors.push(error),
                Ok(mut entry) => {
                    for prompt in case["prompts"].as_array().unwrap() {
                        let answer = &prompt["answer"];
                        if answer.is_null() {
                            entry.cancel();
                            continue;
                        }
                        match prompt["kind"].as_str().unwrap() {
                            "select" => {
                                assert_eq!(
                                    json!(
                                        entry
                                            .choices()
                                            .into_iter()
                                            .map(|choice| choice.label)
                                            .collect::<Vec<_>>()
                                    ),
                                    prompt["choices"],
                                    "{}",
                                    case["name"]
                                );
                                entry.choose(usize::try_from(answer.as_u64().unwrap()).unwrap());
                            }
                            "text" => {
                                if entry.stage == Stage::Description {
                                    assert_eq!(
                                        entry.description,
                                        prompt["default"].as_str().unwrap()
                                    );
                                }
                                if let Err(error) = entry.enter_text(answer.as_str().unwrap()) {
                                    errors.push(error);
                                }
                            }
                            "question" => {
                                assert_eq!(entry.stage, Stage::Activate);
                                entry.activate(answer.as_bool().unwrap());
                            }
                            "credentials" => entry
                                .set_credentials(serde_json::from_value(answer.clone()).unwrap()),
                            _ => panic!("unknown reference prompt"),
                        }
                    }
                    if let Some((domain, login)) = entry.value() {
                        editor.put(domain, login);
                    }
                }
            }
            assert_eq!(json!(errors), case["warnings"], "{}", case["name"]);
        }
        assert_eq!(editor.draft.domains, expected.domains, "{}", case["name"]);
    }
}

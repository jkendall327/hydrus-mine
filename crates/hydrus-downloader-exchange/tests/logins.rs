//! Real Qt-authored login tuples, validations, upgrades and bounded packages.
use hydrus_downloader_exchange::{Error, logins};
use hydrus_legacy::{objects::logins as legacy, serialisable::SerialisableObject};
use hydrus_parse::login::{Access, CredentialKind};
use serde_json::{Value, json};

#[test]
fn login_scripts_preserve_reference_fields_and_upgrade_cookie_names() {
    let fixture = hydrus_testkit::fixture_json("login_editors.json");
    let scripts = logins::decode_text(&fixture["script"].to_string()).unwrap();
    assert_eq!(scripts.len(), 1);
    assert_eq!(
        logins::script_tuple(&scripts[0]).unwrap(),
        fixture["script"]
    );
    let old = logins::decode_text(&fixture["legacy_script"].to_string()).unwrap();
    assert_eq!(
        logins::script_tuple(&old[0]).unwrap(),
        fixture["upgraded_script"]
    );
    for vector in [
        &fixture["definition"]["before"],
        &fixture["definition"]["after"],
    ] {
        let definition = legacy::credential_definition(
            &SerialisableObject::from_tuple_str(&vector.to_string()).unwrap(),
        )
        .unwrap();
        assert_eq!(logins::credential_tuple(&definition), *vector);
    }
    let bundle = logins::decode_text(&fixture["bundle"].to_string()).unwrap();
    assert_eq!(bundle.len(), 2);
    assert_eq!(
        serde_json::from_str::<Value>(&logins::encode_text(&bundle).unwrap()).unwrap(),
        fixture["bundle"]
    );
    assert_eq!(
        logins::decode_png(&logins::encode_png(&bundle).unwrap()).unwrap(),
        bundle
    );
}

#[test]
fn login_validation_matches_python_and_rejects_mixed_packages_atomically() {
    let fixture = hydrus_testkit::fixture_json("login_editors.json");
    let script = logins::decode_text(&fixture["script"].to_string())
        .unwrap()
        .remove(0);
    for vector in fixture["checks"].as_array().unwrap() {
        let credentials = serde_json::from_value(vector["given"].clone()).unwrap();
        assert_eq!(
            json!(script.check_credentials(&credentials).err()),
            vector["error"]
        );
    }
    let mut bad = script.clone();
    bad.credentials.clear();
    assert_eq!(
        json!(bad.check_valid().unwrap_err()),
        fixture["missing_definitions"]
    );
    let mut bad = script.clone();
    bad.steps.reverse();
    assert_eq!(
        json!(bad.check_valid().unwrap_err()),
        fixture["missing_variables"]
    );
    let mut mixed = fixture["bundle"].clone();
    mixed[2][1] = json!([0, "unsupported"]);
    assert!(matches!(
        logins::decode_text(&mixed.to_string()),
        Err(Error::Unsupported(_))
    ));
    let mut future = fixture["script"].clone();
    future[2] = json!(999);
    assert!(logins::decode_text(&future.to_string()).is_err());
    assert_eq!(
        json!(
            [CredentialKind::Normal, CredentialKind::Hidden]
                .map(|kind| (kind.code(), kind.label()))
        ),
        fixture["credential_types"]
    );
    let access = [
        Access::Everything,
        Access::Nsfw,
        Access::Special,
        Access::UserPreferences,
    ];
    assert_eq!(
        json!(access.map(|access| (access.code(), access.label(), access.description()))),
        fixture["access_types"]
    );
}

#[test]
fn native_veto_default_matcher_uses_python_constructor_empty_match_value() {
    let fixture = hydrus_testkit::fixture_json("login_editors.json");
    let step = legacy::login_step(
        &SerialisableObject::from_tuple_str(
            &fixture["step_states"][4]["state"]["value"].to_string(),
        )
        .unwrap(),
    )
    .unwrap();
    let encoded = logins::step_tuple(&step).unwrap();
    assert_eq!(encoded, fixture["step_states"][4]["state"]["value"]);
    let veto_matcher = &encoded[3][8][2][0][1][2][3][1];
    // Reference StringMatch() stores the unused ANY match value as empty text.
    assert_eq!(veto_matcher[2][0], 3);
    assert_eq!(veto_matcher[2][1], "");
}

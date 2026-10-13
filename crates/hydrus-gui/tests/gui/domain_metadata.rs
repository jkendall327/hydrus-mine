//! Domain metadata packages (headers and bandwidth rules) through the real
//! package window, replaying `oracle/fixtures/domain_metadata_packages.json`.
use hydrus_core::bandwidth::Rules;
use hydrus_core::network::NetworkContext;
use hydrus_gui::{downloader_interchange_window as windows, headless};
use hydrus_gui_model::downloader_interchange::{self as exchange, Draft, Native};
use hydrus_store::bandwidth::BandwidthSettings;
use hydrus_store::network::{self, Approval};
use hydrus_store::{Store, settings};
use serde_json::{Value, json};
use std::sync::Arc;

fn setup() -> (tempfile::TempDir, Arc<Store>) {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path()).unwrap();
    (dir, store)
}

fn header(store: &Store, domain: &str, name: &str, value: &str, approval: Approval, reason: &str) {
    let (domain, name, value, reason) = (
        domain.to_owned(),
        name.to_owned(),
        value.to_owned(),
        reason.to_owned(),
    );
    store
        .write(move |ctx| {
            network::set_header(
                ctx.conn(),
                &NetworkContext::domain(domain),
                &name,
                Some(&value),
                Some(approval),
                Some(&reason),
            )
        })
        .unwrap();
}

fn rules(store: &Store, domain: &str, recorded: &Value) {
    let domain = domain.to_owned();
    let rules = recorded
        .as_array()
        .unwrap()
        .iter()
        .map(|r| {
            hydrus_core::bandwidth::Rule::new(
                hydrus_core::bandwidth::BandwidthType::from_code(r[0].as_i64().unwrap()).unwrap(),
                r[1].as_u64(),
                r[2].as_u64().unwrap(),
            )
        })
        .collect::<Vec<_>>();
    store
        .write(move |ctx| {
            let mut current: BandwidthSettings = settings::get(ctx.conn())?;
            current
                .rules
                .retain(|(c, _)| c != &NetworkContext::domain(domain.clone()));
            current
                .rules
                .push((NetworkContext::domain(domain), Rules::new(rules)));
            settings::set(ctx.conn(), &current)
        })
        .unwrap();
}

/// The recorder's `domain_state`: domain headers and rules, empties dropped.
fn state(store: &Store) -> Value {
    let (headers, bandwidth) = store
        .read(|conn| {
            let mut headers = serde_json::Map::new();
            for context in network::header_contexts(conn)? {
                if context.kind != hydrus_core::network::CONTEXT_DOMAIN {
                    continue;
                }
                let mut set = serde_json::Map::new();
                for h in network::headers(conn, &context)? {
                    set.insert(h.name, json!([h.value, h.approval as i64, h.reason]));
                }
                if !set.is_empty() {
                    headers.insert(context.data, Value::Object(set));
                }
            }
            Ok((headers, settings::get::<BandwidthSettings>(conn)?.rules))
        })
        .unwrap();
    let mut rule_map = serde_json::Map::new();
    for (context, rules) in bandwidth {
        if context.kind == hydrus_core::network::CONTEXT_DOMAIN && !context.data.is_empty() {
            let mut rows: Vec<Value> = rules
                .rules()
                .iter()
                .map(|r| json!([r.kind as i64, r.time_delta, r.max_allowed]))
                .collect();
            rows.sort_by_key(|r| (r[0].as_i64(), r[1].as_u64().unwrap_or(0), r[2].as_u64()));
            rule_map.insert(context.data, Value::Array(rows));
        }
    }
    // `serde_json::Map` is ordered, so sorted keys compare equal.
    json!({"headers": Value::Object(headers), "rules": Value::Object(rule_map)})
}

fn recorded_state(value: &Value) -> Value {
    let headers: serde_json::Map<String, Value> = value["headers"]
        .as_object()
        .unwrap()
        .iter()
        .filter(|(_, v)| !v.as_object().unwrap().is_empty())
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect();
    json!({"headers": Value::Object(headers), "rules": value["rules"]})
}

fn seed_initial(store: &Store, fixture: &Value) {
    for (domain, set) in fixture["initial"]["headers"].as_object().unwrap() {
        for (name, row) in set.as_object().unwrap() {
            let approval = Approval::from_code(row[1].as_i64().unwrap()).unwrap();
            header(
                store,
                domain,
                name,
                row[0].as_str().unwrap(),
                approval,
                row[2].as_str().unwrap(),
            );
        }
    }
    // (rules in the order the reference held them, as its package lists them)
    for definition in exchange::decode_text(&fixture["reference"].to_string()).unwrap() {
        if let Native::Domain(m) = &definition.native
            && let Some(recorded) = &m.rules
        {
            let rows = recorded
                .iter()
                .map(|r| json!([r.kind as i64, r.time_delta, r.max_allowed]));
            rules(store, &m.domain, &Value::Array(rows.collect()));
        }
    }
    // The recorded downloader, its URL class and parser.
    let definitions: Vec<_> = exchange::decode_text(&fixture["reference"].to_string())
        .unwrap()
        .into_iter()
        .filter(|d| !matches!(d.native, Native::Domain(_)))
        .collect();
    let mut draft = Draft::load(store).unwrap();
    draft.import(definitions).unwrap();
    draft.save(store).unwrap();
}

fn add_domain(window: &hydrus_gui::DownloaderExchangeWindow, text: Option<&str>) {
    window.invoke_action("add-domain".into());
    assert!(window.get_domain_prompt());
    // the label the window shows is the one the reference's EnterText showed
    let recorded = hydrus_testkit::fixture_json("domain_metadata_packages.json");
    for prompt in recorded["prompts"].as_array().unwrap() {
        assert_eq!(
            window.get_domain_prompt_text(),
            prompt["message"].as_str().unwrap()
        );
        assert_eq!(prompt["title"], "Enter Text");
    }
    match text {
        Some(text) => {
            window.set_domain_text(text.into());
            window.invoke_action("domain-ok".into());
        }
        None => window.invoke_action("domain-cancel".into()),
    }
    assert!(!window.get_domain_prompt());
}

/// Take every domain's headers and rules away (the recorder's
/// `clear_domains`).
fn clear_domains(store: &Store) {
    store
        .write(|ctx| {
            let conn = ctx.conn();
            for context in network::header_contexts(conn)? {
                if context.kind == hydrus_core::network::CONTEXT_DOMAIN {
                    for h in network::headers(conn, &context)? {
                        network::delete_header(conn, &context, &h.name)?;
                    }
                }
            }
            let mut current: BandwidthSettings = settings::get(conn)?;
            current.rules.retain(|(c, _)| {
                c.kind != hydrus_core::network::CONTEXT_DOMAIN || c.data.is_empty()
            });
            settings::set(conn, &current)
        })
        .unwrap();
}

/// Import what the window has reviewed as the recording's stage went: its
/// notices one at a time, its question answered `yes`, then (accepted) its
/// last notice.
fn replay_import(import: &hydrus_gui::DownloaderExchangeWindow, stage: &Value, yes: bool) {
    let notices: Vec<&str> = stage["notices"]
        .as_array()
        .unwrap()
        .iter()
        .map(|n| n.as_str().unwrap())
        .collect();
    let questions = stage["questions"].as_array().unwrap();
    assert_eq!(questions.len(), 1);
    let (details, done) = if yes {
        notices.split_at(notices.len() - 1)
    } else {
        (&notices[..], &[][..])
    };
    for detail in details {
        assert_eq!(shown(import), ((*detail).to_owned(), false));
        import.invoke_action("ok".into());
    }
    assert_eq!(
        shown(import),
        (questions[0].as_str().unwrap().to_owned(), true)
    );
    import.invoke_action(if yes { "yes" } else { "no" }.into());
    if let [done] = done {
        assert_eq!(shown(import), ((*done).to_owned(), false));
        import.invoke_action("ok".into());
    }
}

use crate::downloader_package::{
    export_state, recorded_state as recorded_list, replay_chooser, shown,
};

// leaf: exchange-domain
// leaf: audit-network-exchange-unsupported
#[test]
fn domain_metadata_is_prompted_exported_to_png_and_imported_as_the_reference_does() {
    let _rendered = headless::init();
    let fixture = hydrus_testkit::fixture_json("domain_metadata_packages.json");
    let choosers = fixture["chooser_states"].as_array().unwrap();
    let (dir, store) = setup();
    seed_initial(&store, &fixture);
    let slots = windows::Slots::default();
    let export = windows::package(&store, &slots, false).unwrap();
    assert!(export_state(&export).is_empty(), "the list starts empty");
    // The recorded steps: the domain prompt cancelled, finding nothing,
    // pending-only, a subdomain, repeated; then a downloader chosen.
    let steps = fixture["export_steps"].as_array().unwrap();
    let prompts = [
        "",
        "nothing.example",
        "pendingonly.example",
        "www.api.packages.example",
        "packages.example",
    ];
    for (step, prompt) in steps.iter().zip(prompts) {
        if prompt.is_empty() {
            add_domain(&export, None);
        } else {
            add_domain(&export, Some(prompt));
        }
        let notices = step["notices"].as_array().unwrap();
        if notices.len() == 1 && notices[0] == "No headers/bandwidth rules found!" {
            assert_eq!(
                export.get_error().as_str(),
                "No headers/bandwidth rules found!"
            );
        } else {
            assert!(export.get_error().is_empty(), "{}", export.get_error());
            for notice in notices {
                assert!(
                    export.get_review().contains(notice.as_str().unwrap()),
                    "{notice} missing from {}",
                    export.get_review()
                );
            }
        }
        assert_eq!(
            export_state(&export),
            recorded_list(&step["state"]),
            "{}",
            step["step"]
        );
    }
    // the downloader brings its example domain's rules, its URL class and
    // its parser
    let gug = &steps[prompts.len()];
    assert_eq!(gug["step"], "gug");
    export.invoke_action("add-gug".into());
    replay_chooser(&export, &choosers[0]);
    assert_eq!(export_state(&export), recorded_list(&gug["state"]));
    // the list reproduces the recorded package, tuple for tuple
    let exported = exchange::decode_text(export.get_text().as_str()).unwrap();
    let recorded = exchange::decode_text(&fixture["reference"].to_string()).unwrap();
    assert_eq!(exported.len(), recorded.len());
    for definition in &recorded {
        let same = exported
            .iter()
            .find(|d| d.name() == definition.name())
            .unwrap();
        if matches!(definition.native, Native::Domain(_)) {
            assert_eq!(
                same.tuple().unwrap(),
                definition.tuple().unwrap(),
                "{}",
                definition.name()
            );
        }
    }
    // The PNG the recorder wrote decodes to the same domain packages.
    let png = exchange::decode_png(
        &std::fs::read(hydrus_testkit::fixture_path("domain_metadata_packages.png")).unwrap(),
    )
    .unwrap();
    assert_eq!(png.len(), recorded.len());
    // "export to png" asks with the summary first
    let path = dir.path().join("domains.png");
    export.set_path(path.to_string_lossy().as_ref().into());
    export.invoke_action("save".into());
    assert_eq!(
        shown(&export),
        (
            fixture["export_questions"][0].as_str().unwrap().to_owned(),
            true
        )
    );
    export.invoke_action("yes".into());
    assert!(export.get_error().is_empty(), "{}", export.get_error());
    let written = exchange::decode_png(&std::fs::read(&path).unwrap()).unwrap();
    assert_eq!(written, exported);
    export.invoke_action("cancel".into());

    // Import into a client set up as the recorder's was.
    let (_other_dir, other) = setup();
    for (domain, set) in fixture["before"]["headers"].as_object().unwrap() {
        for (name, row) in set.as_object().unwrap() {
            let approval = Approval::from_code(row[1].as_i64().unwrap()).unwrap();
            header(
                &other,
                domain,
                name,
                row[0].as_str().unwrap(),
                approval,
                row[2].as_str().unwrap(),
            );
        }
    }
    for (domain, recorded) in fixture["before"]["rules"].as_object().unwrap() {
        rules(&other, domain, recorded);
    }
    assert_eq!(state(&other), recorded_state(&fixture["before"]));
    let open = |source: &str| {
        let import = windows::package(&other, &slots, true).unwrap();
        import.set_path(source.into());
        import.invoke_action("open".into());
        assert!(import.get_ready(), "{}", import.get_error());
        import
    };
    let recorded_png = hydrus_testkit::fixture_path("domain_metadata_packages.png");
    let recorded_png = recorded_png.to_string_lossy();
    // "select objects from list", the chooser cancelled: nothing
    let import = open(&recorded_png);
    import.set_select_from_list(true);
    import.invoke_action("accept".into());
    replay_chooser(&import, &choosers[1]);
    assert!(import.get_question().is_empty());
    assert_eq!(
        state(&other),
        recorded_state(&fixture["chooser_cancelled"]["state"])
    );
    // without it: each new package's detail, then the question, declined
    import.set_select_from_list(false);
    import.invoke_action("accept".into());
    replay_import(&import, &fixture["declined"], false);
    assert_eq!(state(&other), recorded_state(&fixture["declined"]["state"]));
    // accepted
    import.invoke_action("accept".into());
    replay_import(&import, &fixture["accepted"], true);
    assert!(slots.0.borrow().is_none());
    assert_eq!(state(&other), recorded_state(&fixture["accepted"]["state"]));
    let saved: hydrus_parse::Downloaders = other.read(settings::get).unwrap();
    assert!(
        saved
            .gugs
            .gugs
            .iter()
            .any(|g| g.name() == "rules only gallery")
    );
    // again: only what is still new, then the rules are set
    let import = open(&recorded_png);
    import.invoke_action("accept".into());
    replay_import(&import, &fixture["again"], true);
    assert_eq!(state(&other), recorded_state(&fixture["again"]["state"]));
    // chosen from the list: the reference's early return in the bandwidth
    // manager, a headers-only package first stopping later rules
    for (stage, chooser) in [("stopped", Some(&choosers[2])), ("nine", None)] {
        clear_domains(&other);
        let stage = &fixture[stage];
        let source = dir.path().join("source.json");
        std::fs::write(&source, stage["source"].to_string()).unwrap();
        let import = open(&source.to_string_lossy());
        import.set_select_from_list(chooser.is_some());
        import.invoke_action("accept".into());
        if let Some(chooser) = chooser {
            replay_chooser(&import, chooser);
        }
        // (nine packages: the first eight shown)
        replay_import(&import, stage, true);
        assert_eq!(state(&other), recorded_state(&stage["state"]));
    }
}

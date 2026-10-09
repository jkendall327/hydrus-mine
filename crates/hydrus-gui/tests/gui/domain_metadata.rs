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
use slint::Model as _;
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

fn choice_names(window: &hydrus_gui::DownloaderExchangeWindow) -> Vec<String> {
    window
        .get_package_choices()
        .iter()
        .map(|c| c.label.to_string())
        .collect()
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

// leaf: exchange-domain
#[test]
fn domain_metadata_is_prompted_exported_to_png_and_imported_as_the_reference_does() {
    let _rendered = headless::init();
    let fixture = hydrus_testkit::fixture_json("domain_metadata_packages.json");
    let (dir, store) = setup();
    seed_initial(&store, &fixture);
    let slots = windows::Slots::default();
    let export = windows::package(&store, &slots, false).unwrap();
    assert!(
        !choice_names(&export)
            .iter()
            .any(|n| n.starts_with("Domain Metadata")),
        "domain metadata is listed only once added"
    );
    // The recorded prompt steps: cancel, nothing, pending-only, subdomain, repeat.
    let steps = fixture["export_steps"].as_array().unwrap();
    let domain_names = |window: &hydrus_gui::DownloaderExchangeWindow| -> Vec<String> {
        choice_names(window)
            .into_iter()
            .filter_map(|n| n.strip_prefix("Domain Metadata: ").map(str::to_owned))
            .collect()
    };
    let recorded_domains = |step: &Value| -> Vec<String> {
        step["state"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|s| s["kind"] == 71)
            .map(|s| s["name"].as_str().unwrap().to_owned())
            .collect()
    };
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
        let mut expected = recorded_domains(step);
        expected.sort();
        let mut actual = domain_names(&export);
        actual.sort();
        assert_eq!(actual, expected, "{}", step["step"]);
    }
    // Choosing the recorded downloader packages its example domain's rules,
    // its URL class and its parser.
    let gug = choice_names(&export)
        .iter()
        .position(|n| n == "GUG: rules only gallery")
        .unwrap();
    export.invoke_package_chosen(-1, false);
    export.invoke_package_chosen(i32::try_from(gug).unwrap(), true);
    let exported = exchange::decode_text(export.get_text().as_str()).unwrap();
    let mut names: Vec<_> = exported.iter().map(|d| d.name().to_owned()).collect();
    names.sort();
    assert_eq!(
        names,
        [
            "rules only gallery",
            "rules only gallery class",
            "rules only parser",
            "rulesonly.example"
        ]
    );
    // Everything chosen reproduces the recorded package, tuple for tuple.
    export.invoke_package_chosen(-1, true);
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
    let path = dir.path().join("domains.png");
    export.set_path(path.to_string_lossy().as_ref().into());
    export.invoke_action("save".into());
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
    let slots = windows::Slots::default();
    let import = windows::package(&other, &slots, true).unwrap();
    import.set_path(
        hydrus_testkit::fixture_path("domain_metadata_packages.png")
            .to_string_lossy()
            .as_ref()
            .into(),
    );
    import.invoke_action("open".into());
    assert!(import.get_error().is_empty(), "{}", import.get_error());
    assert!(import.get_ready());
    for notice in fixture["accepted"]["notices"].as_array().unwrap() {
        let notice = notice.as_str().unwrap();
        if notice.starts_with("For domain") {
            assert!(
                import.get_review().contains(notice),
                "{notice} missing from {}",
                import.get_review()
            );
        }
    }
    for line in [
        "Domain Metadata: api.packages.example",
        "Domain Metadata: packages.example",
        "Domain Metadata: rulesonly.example",
    ] {
        assert!(
            import.get_review().contains(line),
            "{}",
            import.get_review()
        );
    }
    // Reviewing and cancelling changes nothing.
    import.invoke_action("cancel".into());
    assert_eq!(state(&other), recorded_state(&fixture["declined"]["state"]));
    let import = windows::package(&other, &slots, true).unwrap();
    import.set_path(
        hydrus_testkit::fixture_path("domain_metadata_packages.png")
            .to_string_lossy()
            .as_ref()
            .into(),
    );
    import.invoke_action("open".into());
    import.invoke_action("accept".into());
    assert!(import.get_error().is_empty(), "{}", import.get_error());
    assert_eq!(state(&other), recorded_state(&fixture["accepted"]["state"]));
    let saved: hydrus_parse::Downloaders = other.read(settings::get).unwrap();
    assert!(
        saved
            .gugs
            .gugs
            .iter()
            .any(|g| g.name() == "rules only gallery")
    );
    // The second import keeps only what is still new, then sets the rules.
    let import = windows::package(&other, &slots, true).unwrap();
    import.set_path(
        hydrus_testkit::fixture_path("domain_metadata_packages.png")
            .to_string_lossy()
            .as_ref()
            .into(),
    );
    import.invoke_action("open".into());
    assert!(
        import
            .get_review()
            .contains("Bandwidth rules: \n1 rqs per 2 seconds")
    );
    import.invoke_action("accept".into());
    assert_eq!(state(&other), recorded_state(&fixture["again"]["state"]));
}

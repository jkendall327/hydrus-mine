//! An importer's file log window against the reference's, recorded by
//! `oracle/record_file_log.py`: each file's row, the whole log's menu for
//! logs of many counts, and the right-click menu on selected rows.

use serde_json::{Value as Json, json};

use hydrus_gui_model::file_log::{Entry, LogFacts, log_menu, row, row_menu};
use hydrus_store::queues::{
    FileSeed, FileSeedMeta, SeedStatus, SeedType, StatusCounts, file_log_status,
};

fn status(code: i64) -> SeedStatus {
    SeedStatus::from_code(code).unwrap()
}

/// A recorded seed, as the store would hold it.
fn seed(recorded: &Json, now: i64) -> FileSeed {
    let data = recorded[1].as_str().unwrap().to_owned();
    let mut meta = FileSeedMeta::default();
    if recorded[7].as_bool().unwrap() {
        use sha2::Digest as _;
        meta.set_hash("sha256", hex::encode(sha2::Sha256::digest(data.as_bytes())));
    }
    FileSeed {
        id: 0,
        queue_id: 0,
        seed_type: if recorded[0].as_bool().unwrap() {
            SeedType::Url
        } else {
            SeedType::Path
        },
        data_for_comparison: data.clone(),
        data,
        created: now - recorded[3].as_i64().unwrap(),
        modified: now - recorded[4].as_i64().unwrap(),
        source_time: recorded[5].as_i64().map(|ago| now - ago),
        status: status(recorded[2].as_i64().unwrap()),
        note: recorded[6].as_str().unwrap().into(),
        referral_url: None,
        meta,
    }
}

/// A menu as the recording writes it.
fn tree(entries: &[Entry]) -> Json {
    Json::Array(
        entries
            .iter()
            .map(|e| match e {
                Entry::Item(label, _) | Entry::Label(label) => json!(label),
                Entry::Separator => json!("---"),
                Entry::Menu(title, entries) => json!({"menu": title, "entries": tree(entries)}),
            })
            .collect(),
    )
}

fn facts(seeds: &[FileSeed]) -> LogFacts {
    let mut counts = StatusCounts::new();
    for s in seeds {
        *counts.entry(s.status).or_default() += 1;
    }
    LogFacts {
        counts,
        len: seeds.len(),
        urls: seeds.first().is_none_or(|s| s.seed_type == SeedType::Url),
    }
}

#[test]
fn the_file_log_is_the_references() {
    let recorded = hydrus_testkit::fixture_json("file_log.json");
    let now = recorded["now"].as_i64().unwrap();
    let seeds: Vec<FileSeed> = recorded["seeds"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| seed(s, now))
        .collect();
    for (i, (s, want)) in seeds
        .iter()
        .zip(recorded["rows"].as_array().unwrap())
        .enumerate()
    {
        assert_eq!(json!(row(s, i, now)), *want, "row {i}");
    }
    let log = facts(&seeds);
    assert_eq!(file_log_status(&log.counts), recorded["header"]);

    for menu in recorded["menus"].as_array().unwrap() {
        let mut counts = StatusCounts::new();
        for (code, n) in menu["counts"].as_object().unwrap() {
            counts.insert(status(code.parse().unwrap()), n.as_u64().unwrap() as usize);
        }
        let log = LogFacts {
            len: counts.values().sum(),
            counts,
            urls: menu["urls"].as_bool().unwrap(),
        };
        assert_eq!(
            tree(&log_menu(&log, false)),
            menu["menu"],
            "{}",
            menu["counts"]
        );
    }

    let row_menus = recorded["row_menus"].as_array().unwrap();
    for menu in &row_menus[..2] {
        let selected: Vec<&FileSeed> = menu["selected"]
            .as_array()
            .unwrap()
            .iter()
            .map(|i| &seeds[i.as_u64().unwrap() as usize])
            .collect();
        assert_eq!(
            tree(&row_menu(&selected, &log)),
            menu["menu"],
            "{}",
            menu["selected"]
        );
    }
    // a log of paths
    let path = seed(
        &json!([false, "/home/me/a.jpg", 1, 3600, 3600, null, "", true]),
        now,
    );
    let paths = facts(std::slice::from_ref(&path));
    assert_eq!(tree(&row_menu(&[&path], &paths)), row_menus[2]["menu"]);
}

#[test]
fn clipboard_source_batches_match_actual_reference_imports() {
    let fixture = hydrus_testkit::fixture_json("file_log_exchange.json");
    let classes = hydrus_core::url::UrlClasses::new(hydrus_core::url::UrlClassSettings::default());
    for case in fixture["imports"].as_array().unwrap() {
        let parsed =
            hydrus_gui_model::file_log::pasted_sources(case["raw"].as_str().unwrap(), &classes);
        if case["name"] == "empty" {
            assert!(parsed.unwrap_err().contains("Lines of URLs or file paths"));
            continue;
        }
        let mut seen = std::collections::HashSet::new();
        let values: Vec<Json> = parsed
            .unwrap()
            .into_iter()
            .filter(|s| seen.insert((s.seed_type as i64, s.data_for_comparison.clone())))
            .map(|s| json!([s.seed_type as i64, s.data, s.data_for_comparison, 0]))
            .collect();
        assert_eq!(json!(values), case["seeds"], "{}", case["name"]);
    }
}

#[test]
fn selected_urls_are_one_exact_match_or_container() {
    use hydrus_core::search::predicate::{Predicate, SystemPredicate, UrlRule};
    let urls = vec![
        "https://clipboard.example/a".into(),
        "/synthetic/a.jpg".into(),
        "https://clipboard.example/b".into(),
    ];
    assert_eq!(
        hydrus_gui_model::file_log::url_search(&urls),
        vec![Predicate::Or(vec![
            Predicate::System(SystemPredicate::KnownUrl {
                has: true,
                rule: UrlRule::ExactMatch(urls[0].clone())
            }),
            Predicate::System(SystemPredicate::KnownUrl {
                has: true,
                rule: UrlRule::ExactMatch(urls[2].clone())
            }),
        ])]
    );
}

#[test]
fn source_png_carriers_import_actual_qt_exports_and_render_custom_headers() {
    use hydrus_gui_model::png_export;
    let fixture = hydrus_testkit::fixture_json("file_log_png.json");
    let payload = fixture["export"]["payload"].as_str().unwrap();
    let bytes = std::fs::read(hydrus_testkit::fixture_path("file_log_sources.png")).unwrap();
    assert_eq!(
        hydrus_downloader_exchange::text_png::decode(&bytes).unwrap(),
        payload
    );
    assert_eq!(
        png_export::payload_description(payload),
        fixture["export"]["initial"]["summary"]
    );
    assert_eq!(
        png_export::validate("", "title", 512).unwrap_err(),
        fixture["export"]["initial"]["button"]
    );
    let dir = tempfile::tempdir().unwrap();
    assert!(
        png_export::validate(dir.path().join("sources").to_str().unwrap(), "title", 100).is_ok()
    );
    assert!(
        png_export::validate("", "", 99)
            .unwrap_err()
            .contains("set a title")
    );
    assert!(png_export::encode(payload, 4097, "title", "").is_err());
    let image = png_export::encode(
        payload,
        256,
        "Synthetic source list <title>",
        "Shared source lines",
    )
    .unwrap();
    assert_eq!(
        hydrus_downloader_exchange::text_png::decode(&image).unwrap(),
        payload
    );
    let raster = hydrus_media::decode_image(&image).unwrap();
    assert_eq!(raster.width(), 256);
    let channels = usize::from(raster.channels());
    let grey: Vec<u8> = raster.data().chunks_exact(channels).map(|p| p[0]).collect();
    let height = usize::from(u16::from_be_bytes([grey[0], grey[1]]));
    assert!(height > 50 && height < raster.height() as usize);
    assert!(
        grey[2..height * 256].iter().filter(|p| **p < 128).count() > 50,
        "header contains readable text pixels"
    );
    assert!(hydrus_downloader_exchange::text_png::encode("text", 2, &[255]).is_err());
    assert!(hydrus_downloader_exchange::text_png::decode(b"invalid PNG").is_err());
}

fn advanced_seed(index: usize) -> FileSeed {
    use std::collections::BTreeSet;
    let urls = [
        "https://renormalise.example/post?id=1&token=a",
        "https://renormalise.example/post?id=1&token=b",
        "https://renormalise.example/post?id=2&token=c",
    ];
    let n = i64::try_from(index).unwrap();
    FileSeed {
        id: n + 1,
        queue_id: 1,
        seed_type: SeedType::Url,
        data: urls[index].into(),
        data_for_comparison: urls[index].into(),
        created: 1_700_000_000 + n,
        modified: 1_700_000_100 + n,
        source_time: Some(1_699_990_000 + n),
        status: if index == 0 {
            SeedStatus::Error
        } else {
            SeedStatus::SuccessfulAndNew
        },
        note: format!("entry {index} 日本"),
        referral_url: Some("https://renormalise.example/gallery".into()),
        meta: FileSeedMeta {
            request_headers: vec![("X-Synthetic".into(), "header".into())],
            external_filterable_tags: BTreeSet::from(["filter:tag".into()]),
            external_additional_tags: vec![(
                hex::encode([17; 32]),
                BTreeSet::from(["extra:tag".into()]),
            )],
            primary_urls: BTreeSet::from(["https://renormalise.example/primary".into()]),
            source_urls: BTreeSet::from(["https://source.example/a".into()]),
            tags: BTreeSet::from(["tag:one".into()]),
            notes: vec![("note".into(), "metadata note".into())],
            hashes: vec![(
                "sha256".into(),
                hex::encode([u8::try_from(index + 1).unwrap(); 32]),
            )],
            ..FileSeedMeta::default()
        },
    }
}

#[test]
fn selected_import_objects_match_full_reference_clipboard_bytes() {
    let fixture = hydrus_testkit::fixture_json("file_log_advanced.json");
    let seeds = [advanced_seed(0), advanced_seed(1)];
    let output =
        hydrus_gui_model::file_log::export_objects(&seeds.iter().collect::<Vec<_>>()).unwrap();
    assert_eq!(output, fixture["clipboard"][0].as_str().unwrap());
    assert_eq!(
        hydrus_gui_model::file_log::RENORMALISE_QUESTION,
        fixture["questions"][0]["text"]
    );
}

#[test]
fn renormalisation_collapses_later_duplicates_and_preserves_first_progress_and_metadata() {
    use hydrus_core::url::strings::{StringMatch, StringProcessor};
    use hydrus_core::url::{DomainMask, UrlClass, UrlClassSettings, UrlClasses, UrlParameter};
    use hydrus_store::{Store, queues};
    let directory = tempfile::tempdir().unwrap();
    let store = Store::open(directory.path()).unwrap();
    let originals = vec![advanced_seed(0), advanced_seed(1), advanced_seed(2)];
    let queue = store
        .write({
            let originals = originals.clone();
            move |ctx| {
                let queue = queues::create_queue(
                    ctx.conn(),
                    queues::QueueKind::Urls,
                    "synthetic",
                    None,
                    &hydrus_core::import_options::ImportOptionsSlice::default(),
                    0,
                )?;
                queues::restore_file_seeds(ctx.conn(), queue, &originals)?;
                Ok(queue)
            }
        })
        .unwrap();
    let before = store.read(|c| queues::file_seeds(c, queue)).unwrap();
    let classes = UrlClasses::new(UrlClassSettings {
        url_classes: vec![UrlClass {
            name: "synthetic changed class".into(),
            domain_mask: DomainMask::new(vec!["renormalise.example".into()], vec![], false, false),
            path_components: vec![(StringMatch::fixed("post"), None)],
            parameters: vec![UrlParameter {
                name: "id".into(),
                value: StringMatch::any(),
                ephemeral: false,
                default: None,
                default_processor: StringProcessor::default(),
            }],
            keep_extra_parameters_for_server: false,
            ..UrlClass::default()
        }],
        ..UrlClassSettings::default()
    });
    assert_eq!(
        store
            .write(move |ctx| queues::renormalise_file_seeds(ctx.conn(), queue, &classes))
            .unwrap(),
        1
    );
    let after = store.read(|c| queues::file_seeds(c, queue)).unwrap();
    let fixture = hydrus_testkit::fixture_json("file_log_advanced.json");
    let summary: Vec<Json> = after
        .iter()
        .map(|s| {
            json!([
                s.data,
                s.data_for_comparison,
                s.status.code(),
                s.created,
                s.modified,
                s.note
            ])
        })
        .collect();
    assert_eq!(json!(summary), fixture["states"][2]);
    assert_eq!(after[0].id, before[0].id);
    assert_eq!(after[0].meta, before[0].meta);
    assert_eq!(
        after[0].status,
        SeedStatus::Error,
        "first error survives later successful duplicate"
    );
    assert_eq!(after[1].meta, before[2].meta);
    assert_eq!(after[0].source_time, before[0].source_time);
}

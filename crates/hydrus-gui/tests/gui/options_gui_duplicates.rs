//! The options window's duplicates page against the reference's
//! `DuplicatesPanel`: each control's label, box, range and default, and
//! that the saved value reaches what reads it: the duplicate filter's
//! comparison statements and batches, and the thumbnail menu's "open in a
//! new duplicate filter page".

use std::sync::Arc;

use hydrus_core::HashId;
use hydrus_duplicates::potentials::PotentialsQuery;
use hydrus_duplicates::selector::FileContent;
use hydrus_duplicates::statements;
use hydrus_gui::duplicate_filter::{Decision, DuplicateFilter, Step};
use hydrus_media::jpeg::{JpegQuality, Subsampling};
use hydrus_search::FileSearchContext;
use hydrus_search::media::FileFacts;
use hydrus_store::Store;
use hydrus_store::duplicates::{
    ComparisonScores, DuplicateFilterSettings, FileScope, PairOrder, PairSearchKind,
    PixelDuplicates,
};

use crate::options_gui_support::{Client, box_of, row, show_page};

const PAGE: &str = "duplicates";

fn query() -> PotentialsQuery {
    PotentialsQuery {
        scope: FileScope::AllKnownFiles,
        kind: PairSearchKind::OneFileMatchesOneSearch,
        pixel_duplicates: PixelDuplicates::Allowed,
        max_hamming_distance: 4,
        search_1: FileSearchContext::default(),
        search_2: FileSearchContext::default(),
    }
}

fn filter(store: &Arc<Store>) -> DuplicateFilter {
    DuplicateFilter::new(store.clone(), query(), PairOrder::MaxFilesize, false, false).unwrap()
}

fn facts(mime: hydrus_core::Mime, size: u64, width: u64, height: u64) -> FileFacts {
    FileFacts {
        mime: Some(mime),
        size: Some(size),
        width: Some(width),
        height: Some(height),
        ..FileFacts::default()
    }
}

/// jpegs the filter would read the quality of.
struct Jpegs(f64, f64);

impl FileContent for Jpegs {
    fn jpeg_quality(&mut self, file: HashId) -> JpegQuality {
        JpegQuality {
            subsampling: Subsampling::S444,
            quality: Some(if u32::from(file) == 1 { self.0 } else { self.1 }),
            progressive: false,
        }
    }

    fn visual_comparison(
        &mut self,
        _: HashId,
        _: HashId,
    ) -> Option<(
        hydrus_media::visual::Verdict,
        Option<hydrus_media::visual::Verdict>,
    )> {
        None
    }
}

fn score_of(statements: &[statements::Statement], key: &str) -> i32 {
    statements
        .iter()
        .find(|s| s.key == key)
        .unwrap_or_else(|| panic!("no {key:?} statement in {statements:?}"))
        .score
}

// leaf: audit-options-duplicates-duplicate-filter-comparison-score-weights-score-for-jpeg-with-non-trivially-higher-jpeg-quality
// leaf: audit-options-duplicates-duplicate-filter-comparison-score-weights-score-for-jpeg-with-significantly-higher-jpeg-quality
// leaf: audit-options-duplicates-duplicate-filter-comparison-score-weights-score-for-file-with-non-trivially-higher-filesize
// leaf: audit-options-duplicates-duplicate-filter-comparison-score-weights-score-for-file-with-significantly-higher-filesize
// leaf: audit-options-duplicates-duplicate-filter-comparison-score-weights-score-for-file-with-higher-resolution-as-num-pixels
// leaf: audit-options-duplicates-duplicate-filter-comparison-score-weights-score-for-file-with-significantly-higher-resolution-as-num-pixels
// leaf: audit-options-duplicates-duplicate-filter-comparison-score-weights-score-for-file-with-more-tags
// leaf: audit-options-duplicates-duplicate-filter-comparison-score-weights-score-for-file-with-non-trivially-earlier-import-time
// leaf: audit-options-duplicates-duplicate-filter-comparison-score-weights-score-for-file-with-nicer-resolution-ratio
// leaf: audit-options-duplicates-duplicate-filter-comparison-score-weights-score-for-file-with-audio
#[test]
fn the_ten_comparison_score_weights_are_the_references_and_drive_the_comparison() {
    use hydrus_core::Mime::{ImageJpeg, ImagePng};
    let client = Client::basic();
    let options = client.open_options();
    show_page(&options, PAGE);
    // (label, the reference's default, the score a pair's statement then carries)
    let labels = [
        ("Score for jpeg with non-trivially higher jpeg quality:", 10),
        ("Score for jpeg with significantly higher jpeg quality:", 20),
        ("Score for file with non-trivially higher filesize:", 10),
        ("Score for file with significantly higher filesize:", 20),
        ("Score for file with higher resolution (as num pixels):", 20),
        (
            "Score for file with significantly higher resolution (as num pixels):",
            50,
        ),
        ("Score for file with more tags:", 8),
        ("Score for file with non-trivially earlier import time:", 4),
        ("Score for file with 'nicer' resolution ratio:", 10),
        ("Score for file with audio:", 20),
    ];
    let chosen: Vec<i32> = (0..labels.len() as i32).map(|i| -95 + 17 * i).collect();
    for ((label, default), value) in labels.iter().zip(&chosen) {
        let (i, r) = row(&options, label);
        // a spin box from -100 to 100 in the "duplicate filter comparison score weights" box
        assert_eq!((r.kind, r.minimum, r.maximum), (2, -100, 100), "{label}");
        assert_eq!(r.number, *default, "{label}: the reference's default");
        assert_eq!(
            box_of(&options, label),
            "duplicate filter comparison score weights"
        );
        options.invoke_number_edited(i, *value);
    }
    // (nothing is saved before apply)
    assert_eq!(
        client.setting::<DuplicateFilterSettings>().scores,
        ComparisonScores::default()
    );
    options.invoke_apply();
    let saved = client.setting::<DuplicateFilterSettings>().scores;
    let expected = ComparisonScores {
        higher_jpeg_quality: chosen[0],
        much_higher_jpeg_quality: chosen[1],
        higher_filesize: chosen[2],
        much_higher_filesize: chosen[3],
        higher_resolution: chosen[4],
        much_higher_resolution: chosen[5],
        more_tags: chosen[6],
        older: chosen[7],
        nicer_ratio: chosen[8],
        has_audio: chosen[9],
    };
    assert_eq!(saved, expected);

    // a filter opened afterwards judges pairs with them
    let scores = *filter(&client.store).scores();
    assert_eq!(scores, expected);
    let now = 1_700_000_000;
    let fast = |shown: &FileFacts, other: &FileFacts| statements::fast(shown, other, &scores, now);

    // filesize: 1.5x is non-trivially higher, 3x significantly
    let small = facts(ImagePng, 1000, 100, 100);
    let medium = facts(ImagePng, 1500, 100, 100);
    let large = facts(ImagePng, 3000, 100, 100);
    assert_eq!(
        score_of(&fast(&medium, &small).statements, "filesize"),
        scores.higher_filesize
    );
    assert_eq!(
        score_of(&fast(&large, &small).statements, "filesize"),
        scores.much_higher_filesize
    );
    assert_eq!(
        score_of(&fast(&small, &medium).statements, "filesize"),
        -scores.higher_filesize
    );
    // resolution (as pixels): 1.44x higher, 2.67x significantly higher
    let r100 = facts(ImagePng, 1000, 100, 100);
    let r120 = facts(ImagePng, 1000, 120, 120);
    let r200 = facts(ImagePng, 1000, 200, 150);
    assert_eq!(
        score_of(&fast(&r120, &r100).statements, "resolution"),
        scores.higher_resolution
    );
    assert_eq!(
        score_of(&fast(&r200, &r100).statements, "resolution"),
        scores.much_higher_resolution
    );
    // the nicer ratio: 16:9 against an unusual one
    let wide = facts(ImagePng, 1000, 160, 90);
    let odd = facts(ImagePng, 1000, 160, 100);
    assert_eq!(
        score_of(&fast(&wide, &odd).statements, "ratio"),
        scores.nicer_ratio
    );
    // audio
    let mut audio = small.clone();
    audio.has_audio = true;
    assert_eq!(
        score_of(&fast(&audio, &small).statements, "has_audio"),
        scores.has_audio
    );
    // more tags
    let mut tagged = small.clone();
    tagged.tags = ["a", "b"].map(String::from).into();
    let mut one = small.clone();
    one.tags = ["a"].map(String::from).into();
    assert_eq!(
        score_of(&fast(&tagged, &one).statements, "num_tags"),
        scores.more_tags
    );
    // an import time more than a month earlier
    let mut older = small.clone();
    older.imported_ms = Some(1_000_000_000_000);
    let mut newer = small.clone();
    newer.imported_ms = Some(1_000_000_000_000 + 90 * 86_400 * 1000);
    assert_eq!(
        score_of(&fast(&older, &newer).statements, "time_imported"),
        scores.older
    );
    // jpeg quality: the lower number is the better; 2x is the boundary of significant
    let jpeg = facts(ImageJpeg, 1000, 100, 100);
    let (a, b) = (HashId::from(1), HashId::from(2));
    let slow = |good: f64, bad: f64| {
        statements::slow(
            (a, &jpeg),
            (b, &jpeg),
            false,
            &scores,
            &mut Jpegs(good, bad),
        )
    };
    assert_eq!(
        score_of(&slow(300.0, 450.0), "jpeg_quality"),
        scores.higher_jpeg_quality
    );
    assert_eq!(
        score_of(&slow(300.0, 900.0), "jpeg_quality"),
        scores.much_higher_jpeg_quality
    );
}

/// Near-duplicate families imported and searched, so that pairs wait.
fn store_with_pairs() -> (Vec<tempfile::TempDir>, Arc<Store>) {
    use hydrus_import::{FileImportOptions, FileImporter};
    use hydrus_store::similar::{self, SimilarFilesSettings};
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path()).unwrap();
    let importer = FileImporter::new(Arc::clone(&store), hydrus_media::MediaTools::new());
    let mut names: Vec<_> = std::fs::read_dir(hydrus_testkit::fixture_path("auto_resolution"))
        .unwrap()
        .map(|e| e.unwrap().path())
        .collect();
    names.sort();
    for path in names {
        importer
            .import_path(&path, &FileImportOptions::default())
            .unwrap();
    }
    store
        .write(|ctx| {
            hydrus_store::settings::set(
                ctx.conn(),
                &SimilarFilesSettings {
                    search_distance: 4,
                    ..SimilarFilesSettings::default()
                },
            )
        })
        .unwrap();
    while similar::run_search(&store, 100).unwrap() > 0 {}
    (vec![dir], store)
}

/// A filter over the files of "my files" alone, with the saved settings.
fn my_files_filter(store: &Arc<Store>) -> DuplicateFilter {
    let (id, key) = {
        let snapshot = store.snapshot();
        let service = snapshot
            .services
            .builtin(hydrus_core::service::builtin_keys::MY_FILES)
            .unwrap();
        (service.id, service.key.clone())
    };
    let search = FileSearchContext {
        location: hydrus_search::LocationContext::single(key),
        ..FileSearchContext::default()
    };
    let query = PotentialsQuery {
        scope: FileScope::Domains {
            current: vec![id],
            deleted: Vec::new(),
        },
        search_1: search.clone(),
        search_2: search,
        ..query()
    };
    DuplicateFilter::new(store.clone(), query, PairOrder::MaxFilesize, false, false).unwrap()
}

#[test]
fn batch_size_and_auto_commit_set_how_the_filter_batches_and_confirms() {
    let (dirs, store) = store_with_pairs();
    let client = Client::with(dirs, store.clone());
    let options = client.open_options();
    show_page(&options, PAGE);

    let size = "Max size of duplicate filter pair batches (in mixed mode):";
    let auto = "Auto-commit completed batches of this size or smaller:";
    let (size_row, r) = row(&options, size);
    assert_eq!((r.kind, r.minimum, r.maximum, r.number), (2, 5, 1024, 100));
    assert_eq!(box_of(&options, size), "duplicate filter batches");
    let (auto_row, r) = row(&options, auto);
    assert_eq!((r.kind, r.minimum, r.maximum), (3, 1, 50));
    assert_eq!(r.none_phrase, "no, always confirm");
    assert!(!r.is_none && r.number == 1, "the reference's default is 1");
    assert_eq!(box_of(&options, auto), "duplicate filter batches");

    // batches of 5, always confirmed
    options.invoke_number_edited(size_row, 5);
    options.invoke_none_toggled(auto_row, true);
    assert_eq!(
        client.setting::<DuplicateFilterSettings>().max_batch_size,
        100
    );
    options.invoke_apply();
    let saved = client.setting::<DuplicateFilterSettings>();
    assert_eq!(
        (saved.max_batch_size, saved.auto_commit_batch_size),
        (5, None)
    );

    let mut filter = my_files_filter(&store);
    assert_eq!(filter.load_batch().unwrap(), Step::Showing);
    assert!(
        filter.index_text().ends_with("1/5 - no decisions yet"),
        "{}",
        filter.index_text()
    );
    let mut step = Step::Showing;
    let mut decided = 0;
    while step == Step::Showing {
        step = filter.decide(Decision::FALSE_POSITIVE).unwrap();
        decided += 1;
        assert!(decided <= 5);
    }
    assert!(matches!(step, Step::Confirm { .. }), "{step:?}");
    assert!(!filter.done_work(), "waiting for the confirmation");

    // and a batch of 5 or fewer is committed without asking
    let options = client.open_options();
    show_page(&options, PAGE);
    let (auto_row, r) = row(&options, auto);
    assert!(r.is_none);
    options.invoke_none_toggled(auto_row, false);
    options.invoke_number_edited(auto_row, 5);
    options.invoke_apply();
    assert_eq!(
        client
            .setting::<DuplicateFilterSettings>()
            .auto_commit_batch_size,
        Some(5)
    );
    let mut filter = my_files_filter(&store);
    assert_eq!(filter.load_batch().unwrap(), Step::Showing);
    for _ in 0..4 {
        assert_eq!(
            filter.decide(Decision::FALSE_POSITIVE).unwrap(),
            Step::Showing
        );
    }
    assert!(!filter.done_work());
    assert_eq!(
        filter.decide(Decision::FALSE_POSITIVE).unwrap(),
        Step::Showing
    );
    assert!(filter.done_work(), "the batch committed itself");
    assert!(filter.index_text().ends_with("1/5 - no decisions yet"));
}

/// The domain of the newest duplicates page.
fn newest_duplicates_location(client: &Client) -> hydrus_search::LocationContext {
    use hydrus_core::pages::PageContent;
    let pages = client.bound.pages.borrow();
    let page = pages
        .session()
        .all_pages()
        .into_iter()
        .rev()
        .find(|p| matches!(p.content, PageContent::Duplicates { .. }))
        .expect("a duplicates page");
    match &page.content {
        PageContent::Duplicates { duplicates, .. } => duplicates.search.search_1.location.clone(),
        _ => unreachable!(),
    }
}

/// thumbnails right-click > open > in a new duplicate filter page.
fn open_selection_in_duplicate_filter_page(client: &Client) {
    use slint::Model as _;
    // (back on the first tab, whose files they are)
    client.ui.invoke_tab_chosen(0, 0);
    let page = client.bound.current.borrow().clone();
    let files: Vec<HashId> = page.borrow().results()[..2].to_vec();
    page.borrow_mut().select_files(&files);
    client.ui.invoke_thumbnail_menu_requested(-1);
    let menu = client.ui.get_thumbnail_menu();
    let at = (0..menu.open_a.row_count())
        .map(|i| menu.open_a.row_data(i).unwrap())
        .find(|r| r.label == "in a new duplicate filter page")
        .expect("the open menu offers it")
        .id;
    client.ui.invoke_menu_chosen(at);
}

// leaf: audit-options-duplicates-open-in-a-new-duplicates-filter-page-set-to-combined-local-file-domains-when-hitting-open-files-in-a-new-duplicates-filter-page
#[test]
fn opening_files_in_a_duplicates_page_uses_combined_local_file_domains_only_if_asked() {
    use hydrus_core::service::builtin_keys;
    use hydrus_search::LocationContext;
    let (dirs, store) = crate::options_gui_support::basic_store();
    let client = Client::with(dirs, store);
    client.ui.invoke_search_edited("system:everything".into());
    client.ui.invoke_search_accepted();
    let combined = LocationContext::single(hydrus_core::ServiceKey::new(
        builtin_keys::COMBINED_LOCAL_FILE_DOMAINS.to_vec(),
    ));

    let options = client.open_options();
    show_page(&options, PAGE);
    let label = "Set to \"combined local file domains\" when hitting \"Open files in a new duplicates filter page\":";
    let (at, r) = row(&options, label);
    // a checkbox, on in the reference's defaults, in the "open in a new duplicates filter page" box
    assert_eq!((r.kind, r.checked), (1, true));
    assert_eq!(
        box_of(&options, label),
        "open in a new duplicates filter page"
    );

    open_selection_in_duplicate_filter_page(&client);
    assert_eq!(newest_duplicates_location(&client), combined);

    // unticked: the page keeps the domain of the page the files came from
    options.invoke_check_toggled(at, false);
    options.invoke_apply();
    assert!(
        !client
            .setting::<hydrus_store::settings::PageSettings>()
            .duplicate_filter_uses_all_my_files
    );
    open_selection_in_duplicate_filter_page(&client);
    client.ui.invoke_tab_chosen(0, 0);
    let page_domain = client.bound.current.borrow().borrow().location().clone();
    assert_ne!(page_domain, combined, "the page searches one domain");
    assert_eq!(newest_duplicates_location(&client), page_domain);
}

// leaf: audit-options-duplicates-open-in-a-new-duplicates-filter-page-set-to-combined-local-file-domains-when-hitting-open-files-in-a-new-duplicates-filter-page
#[test]
fn the_shortcut_command_and_the_viewer_open_duplicates_pages_as_the_reference_does() {
    use hydrus_core::pages::PageContent;
    use hydrus_core::shortcuts::{Binding, Gesture, Settings};
    use hydrus_store::settings::{PageSettings, TagSearchActivation};
    use slint::ComponentHandle as _;
    let recorded = hydrus_testkit::fixture_json("open_in_duplicates.json");
    let (dirs, store) = crate::options_gui_support::basic_store();
    // command 154 on "y" in the thumbnails' set and in the viewer's
    store
        .write(|ctx| {
            let mut settings: Settings = hydrus_store::settings::get(ctx.conn())?;
            for set in ["media", "media_viewer"] {
                settings.sets.entry(set.into()).or_default().push(Binding {
                    gesture: Gesture::keyboard(0, u32::from('y'), 0, false).unwrap(),
                    action: 154,
                    text: None,
                    content: None,
                });
            }
            hydrus_store::settings::set(ctx.conn(), &settings)
        })
        .unwrap();
    let client = Client::with(dirs, store.clone());
    client.ui.invoke_search_edited("system:everything".into());
    client.ui.invoke_search_accepted();
    // the recorded files, by their sha256, on that search page
    let wanted: Vec<hydrus_core::Sha256> = recorded["hashes"]
        .as_array()
        .unwrap()
        .iter()
        .map(|h| h.as_str().unwrap().parse().unwrap())
        .collect();
    let files: Vec<HashId> = store
        .read(|conn| {
            wanted
                .iter()
                .map(|h| Ok(hydrus_store::master::hash_id(conn, h)?.unwrap()))
                .collect::<hydrus_store::Result<Vec<_>>>()
        })
        .unwrap();
    let source_tab = 0;
    let page = client.bound.current.borrow().clone();
    assert!(files.iter().all(|f| page.borrow().results().contains(f)));
    let page_location = page.borrow().location().clone();
    let combined = hydrus_search::LocationContext::single(hydrus_core::ServiceKey::new(
        hydrus_core::service::builtin_keys::COMBINED_LOCAL_FILE_DOMAINS.to_vec(),
    ));
    let keys_hex = |location: &hydrus_search::LocationContext| -> Vec<String> {
        location
            .current()
            .iter()
            .map(hydrus_core::ServiceKey::to_hex)
            .collect()
    };
    let recorded_keys = |v: &serde_json::Value| -> Vec<String> {
        v.as_array()
            .unwrap()
            .iter()
            .map(|k| k.as_str().unwrap().to_owned())
            .collect()
    };
    // the recording's own locations are the ones the page and the option name
    assert_eq!(
        keys_hex(&page_location),
        recorded_keys(&recorded["page_location"])
    );
    assert_eq!(keys_hex(&combined), recorded_keys(&recorded["combined"]));
    let newest = |client: &Client| {
        let pages = client.bound.pages.borrow();
        pages
            .session()
            .all_pages()
            .into_iter()
            .rev()
            .find(|p| matches!(p.content, PageContent::Duplicates { .. }))
            .cloned()
    };
    for case in recorded["cases"].as_array().unwrap() {
        let combined_option = case["combined_option"].as_bool().unwrap();
        let activate = case["activate_option"].as_bool().unwrap();
        let call = &case["calls"][0];
        client
            .store
            .write(move |ctx| {
                let mut pages: PageSettings = hydrus_store::settings::get(ctx.conn())?;
                pages.duplicate_filter_uses_all_my_files = combined_option;
                hydrus_store::settings::set(ctx.conn(), &pages)?;
                let mut search: TagSearchActivation = hydrus_store::settings::get(ctx.conn())?;
                search.activate_main = activate;
                hydrus_store::settings::set(ctx.conn(), &search)
            })
            .unwrap();
        client.ui.invoke_tab_chosen(0, source_tab);
        let before = client.bound.pages.borrow().page_count();
        let requests = client.ui.get_tag_search_activation_requests();
        if case["where"] == "thumbnails" {
            page.borrow_mut().select_files(&files);
            client.focus_thumbnails();
            assert!(client.ui.get_grid_focused(), "{case}");
            client
                .ui
                .window()
                .dispatch_event(slint::platform::WindowEvent::KeyPressed { text: "y".into() });
            client
                .ui
                .window()
                .dispatch_event(slint::platform::WindowEvent::KeyReleased { text: "y".into() });
        } else {
            let at = page
                .borrow()
                .results()
                .iter()
                .position(|f| *f == files[0])
                .unwrap();
            client
                .ui
                .invoke_thumbnail_activated(i32::try_from(at).unwrap());
            let viewer = client
                .bound
                .viewer
                .borrow()
                .as_ref()
                .unwrap()
                .clone_strong();
            assert!(viewer.invoke_shortcut_key("y".into(), 0), "{case}");
            viewer.invoke_close_requested();
        }
        assert_eq!(
            client.bound.pages.borrow().page_count(),
            before + 1,
            "{case}"
        );
        let opened = newest(&client).expect("a duplicates page");
        assert_eq!(opened.name, call["name"].as_str().unwrap(), "{case}");
        let PageContent::Duplicates { duplicates, .. } = &opened.content else {
            unreachable!()
        };
        let search = &duplicates.search.search_1;
        // the location the recording shows the reference's page searching
        assert_eq!(
            keys_hex(&search.location),
            recorded_keys(&call["current"]),
            "{case}"
        );
        assert!(search.location.deleted().is_empty(), "{case}");
        assert_eq!(
            keys_hex(&search.location),
            keys_hex(if combined_option {
                &combined
            } else {
                &page_location
            }),
            "{case}"
        );
        // the page's files: the recording's own hashes (the thumbnails' whole selection, or the
        // viewer's one shown file), not just how many
        assert_eq!(call["predicates"].as_array().unwrap().len(), 1);
        assert_eq!(search.predicates.len(), 1, "{case}");
        let opened_hashes: Vec<String> = search
            .predicates
            .iter()
            .find_map(|p| match p {
                hydrus_core::search::predicate::Predicate::System(
                    hydrus_core::search::predicate::SystemPredicate::Hash {
                        hashes: hydrus_core::search::predicate::FileHashes::Sha256(h),
                        ..
                    },
                ) => Some(h.iter().map(ToString::to_string).collect()),
                _ => None,
            })
            .expect("a system:hash predicate");
        let recorded_hashes: Vec<String> = if case["where"] == "thumbnails" {
            assert_eq!(call["predicates"][0], "system:hash is in 3 hashes");
            recorded["hashes"]
                .as_array()
                .unwrap()
                .iter()
                .map(|h| h.as_str().unwrap().to_owned())
                .collect()
        } else {
            vec![
                call["predicates"][0]
                    .as_str()
                    .unwrap()
                    .strip_prefix("system:hash is ")
                    .unwrap()
                    .to_owned(),
            ]
        };
        let mut opened_sorted = opened_hashes;
        opened_sorted.sort();
        let mut recorded_sorted = recorded_hashes;
        recorded_sorted.sort();
        assert_eq!(opened_sorted, recorded_sorted, "{case}");
        assert_eq!(
            client.ui.get_tag_search_activation_requests() - requests,
            i32::from(call["activate_window"].as_bool().unwrap()),
            "{case}"
        );
    }
}

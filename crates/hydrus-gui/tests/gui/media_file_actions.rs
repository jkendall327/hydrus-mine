//! File actions from the thumbnail menu, through the real window and the real
//! store: archive and inbox, delete from a domain, undelete and physical
//! deletion (`ClientGUIMediaMenus`, `ClientGUIMediaActions`).

use hydrus_core::HashId;
use slint::{ComponentHandle as _, Model as _};

use super::media_support::*;

fn domains(fixture: &Fixture, file: HashId) -> Vec<String> {
    let snapshot = fixture.store.snapshot();
    let batch = fixture
        .store
        .read(|c| hydrus_store::media::load(c, &snapshot.services, None, &[file]))
        .unwrap();
    let mut names: Vec<String> = batch.results[0]
        .current
        .iter()
        .filter_map(|c| snapshot.services.get(c.service).ok())
        .filter(|s| {
            matches!(
                s.service_type(),
                hydrus_core::ServiceType::LocalFileDomain
                    | hydrus_core::ServiceType::LocalFileTrashDomain
            )
        })
        .map(|s| s.name.clone())
        .collect();
    names.sort();
    names
}

fn inboxed(fixture: &Fixture, files: &[HashId]) -> Vec<HashId> {
    let mut inbox: Vec<HashId> = fixture
        .store
        .read(|c| hydrus_store::media::inboxed(c, files))
        .unwrap()
        .into_iter()
        .collect();
    inbox.sort_unstable();
    inbox
}

// leaf: audit-media-context-lifecycle
// leaf: audit-options-files-and-trash-confirm-sending-more-than-one-file-to-archive-or-inbox
#[test]
fn archive_inbox_delete_undelete_and_physical_delete_from_the_menu() {
    let fixture = start_local();
    let results = fixture.results();
    // one file in the inbox, one archived
    let inbox_now = inboxed(&fixture, &results);
    let in_inbox = *inbox_now.first().unwrap();
    let archived = *results.iter().find(|f| !inbox_now.contains(f)).unwrap();
    let both = [in_inbox, archived];
    fixture.select_files(&both);
    let menu = fixture.menu(-1);
    let filter = rows(&menu.filter);
    // archive: the inbox file leaves the inbox
    fixture
        .ui
        .invoke_menu_chosen(find(&filter, "archive selected"));
    assert!(inboxed(&fixture, &both).is_empty());
    // and back: both in the inbox again
    let menu = fixture.menu(-1);
    let reinbox = find(&rows(&menu.filter), "re-inbox selected");
    fixture.ui.invoke_menu_chosen(reinbox);
    // (two files to act on: asked first, "no" leaving them be)
    assert_eq!(fixture.ui.get_question(), "Send 2 files to inbox?");
    fixture.ui.invoke_answer(false);
    assert!(inboxed(&fixture, &both).is_empty());
    fixture.ui.invoke_menu_chosen(reinbox);
    fixture.ui.invoke_answer(true);
    let mut want = both.to_vec();
    want.sort_unstable();
    assert_eq!(inboxed(&fixture, &both), want);

    // delete from its domain: asks first, naming the domain
    let file = results[0];
    let before = domains(&fixture, file);
    assert!(!before.contains(&"trash".to_owned()), "{before:?}");
    fixture.select_files(&[file]);
    let menu = fixture.menu(-1);
    let delete = rows(&menu.delete);
    let (label, id) = delete[0].clone();
    let domain = label.strip_prefix("delete from ").unwrap().to_owned();
    assert!(before.contains(&domain));
    fixture.ui.invoke_menu_chosen(id);
    assert_eq!(
        super::common::delete_question(&fixture.bound),
        format!("Send one file from {domain} to trash?")
    );
    super::common::answer_delete(&fixture.bound, false);
    assert_eq!(domains(&fixture, file), before);
    fixture.ui.invoke_menu_chosen(id);
    super::common::answer_delete(&fixture.bound, true);
    let mut after = domains(&fixture, file);
    assert!(
        after.contains(&"trash".to_owned()) && !after.contains(&domain),
        "{after:?}"
    );
    after.retain(|d| d != "trash");
    assert_eq!(after.len(), before.len() - 1);
    let reason = fixture
        .store
        .read(|c| {
            Ok(
                hydrus_store::media::load(c, &fixture.store.snapshot().services, None, &[file])?
                    .results[0]
                    .deletion_reason
                    .clone(),
            )
        })
        .unwrap();
    // (the file was deleted before, and the simple dialog keeps its reason)
    assert_eq!(reason.as_deref(), Some("Deleted via Client API."));

    // in the trash: delete physically now, and undelete
    let menu = fixture.menu(-1);
    assert!(rows(&menu.delete).is_empty());
    let trash = rows(&menu.trash);
    assert_eq!(
        trash.iter().map(|(l, _)| l.as_str()).collect::<Vec<_>>(),
        ["delete physically now", "undelete"]
    );
    fixture.ui.invoke_menu_chosen(find(&trash, "undelete"));
    // (deleted from both its domains: the chooser asks where to, and "all the
    // above" puts it back in both; `undelete_question.rs` has every case)
    let chooser = hydrus_gui::undelete::last_chooser().expect("asked where to");
    assert!(chooser.window().is_visible());
    chooser.invoke_chosen(i32::try_from(chooser.get_choices().row_count()).unwrap() - 1);
    let restored = domains(&fixture, file);
    assert!(!restored.contains(&"trash".to_owned()), "{restored:?}");
    assert!(restored.contains(&domain), "{restored:?}");

    // (undeleted into my files as well: delete is now a submenu of domains)
    assert_eq!(restored, ["art", "my files"]);
    fixture.select_files(&[file]);
    let menu = fixture.menu(-1);
    assert_eq!(menu.delete_title, "delete");
    let from = rows(&menu.delete_menu);
    assert_eq!(
        from.iter().map(|(l, _)| l.as_str()).collect::<Vec<_>>(),
        ["from art", "from my files"]
    );
    fixture.ui.invoke_menu_chosen(from[0].1);
    // (the delete files dialog, on the domain the entry names)
    let dialog = super::common::delete_dialog(&fixture.bound).unwrap();
    assert_eq!(dialog.get_selected_action(), 0);
    assert_eq!(
        super::common::delete_question(&fixture.bound),
        "Remove one file from art?\nRemove one file from my files?\nDelete from all local services? (force send to trash)"
    );
    super::common::answer_delete(&fixture.bound, true);
    // (one domain left: back to the plain entry)
    let menu = fixture.menu(-1);
    assert_eq!(rows(&menu.delete).len(), 1);
    fixture
        .ui
        .invoke_menu_chosen(find(&rows(&menu.delete), "delete from my files"));
    assert_eq!(
        super::common::delete_question(&fixture.bound),
        "Send one file from my files to trash?"
    );
    super::common::answer_delete(&fixture.bound, true);
    assert_eq!(domains(&fixture, file), ["trash"]);
    // and then for good: asked, "no" changing nothing
    let menu = fixture.menu(-1);
    let physically = find(&rows(&menu.trash), "delete physically now");
    fixture.ui.invoke_menu_chosen(physically);
    assert_eq!(
        super::common::delete_question(&fixture.bound),
        "Permanently delete one trashed file?"
    );
    super::common::answer_delete(&fixture.bound, false);
    assert!(domains(&fixture, file).contains(&"trash".to_owned()));
    fixture.ui.invoke_menu_chosen(physically);
    super::common::answer_delete(&fixture.bound, true);
    assert!(
        domains(&fixture, file).iter().all(|d| d != "trash"),
        "{:?}",
        domains(&fixture, file)
    );
}

fn blurhash(fixture: &Fixture, file: HashId) -> Option<String> {
    fixture
        .store
        .read(|c| {
            Ok(c.query_row(
                "SELECT blurhash FROM files WHERE hash_id = ?1",
                [file],
                |r| r.get::<_, Option<String>>(0),
            )?)
        })
        .unwrap()
}

fn wait_until(what: &str, mut done: impl FnMut() -> bool) {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(20);
    while !done() {
        assert!(std::time::Instant::now() < deadline, "{what}");
        slint::platform::update_timers_and_animations();
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
}

// leaf: audit-media-context-missing-maintenance
#[test]
fn maintenance_jobs_are_listed_asked_and_run_on_the_selected_files() {
    use hydrus_gui_model::thumbnail_maintenance::{HUMAN_ORDER, description};
    use hydrus_store::file_maintenance::JobType;

    let fixture = start();
    let results = fixture.results();
    let jpeg = fixture
        .store
        .read(|c| hydrus_store::media::load_basic(c, &results))
        .unwrap()
        .iter()
        .find(|m| {
            m.info
                .as_ref()
                .is_some_and(|i| i.mime == hydrus_core::Mime::ImageJpeg)
        })
        .unwrap()
        .hash_id;
    let index = i32::try_from(results.iter().position(|&f| f == jpeg).unwrap()).unwrap();
    fixture.select(index, &[]);
    let menu = fixture.menu(index);
    // every job kind, in the reference's human order
    let jobs = rows(&menu.manage_maintenance);
    assert_eq!(
        jobs.iter().map(|(l, _)| l.as_str()).collect::<Vec<_>>(),
        HUMAN_ORDER
            .iter()
            .map(|j| j.description())
            .collect::<Vec<_>>()
    );
    assert_eq!(jobs.len(), 27);

    // the question is the job's description (a few have their own wording);
    // "no" runs nothing
    let blurhash_label = JobType::Blurhash.description();
    let other = results.iter().copied().find(|&f| f != jpeg).unwrap();
    fixture
        .store
        .write(move |ctx| {
            ctx.conn().execute(
                "UPDATE files SET blurhash = NULL WHERE hash_id IN (?1, ?2)",
                [jpeg, other],
            )?;
            Ok(())
        })
        .unwrap();
    fixture.ui.invoke_menu_chosen(find(&jobs, blurhash_label));
    let question = hydrus_gui::thumbnail_maintenance_window::question().expect("a question");
    assert_eq!(question.get_window_title(), "Are you sure?");
    assert_eq!(question.get_message(), description(JobType::Blurhash));
    assert_eq!(
        question.get_message(),
        "This generates a very small version of the file's thumbnail that can be used as a placeholder while the thumbnail loads."
    );
    question.invoke_answered(false);
    std::thread::sleep(std::time::Duration::from_millis(300));
    slint::platform::update_timers_and_animations();
    assert_eq!(blurhash(&fixture, jpeg), None);

    // some jobs say how many files they work on
    fixture
        .ui
        .invoke_menu_chosen(find(&jobs, JobType::FileMetadata.description()));
    let question = hydrus_gui::thumbnail_maintenance_window::question().unwrap();
    assert!(
        question
            .get_message()
            .starts_with("This will reparse the 1 selected files' metadata."),
        "{}",
        question.get_message()
    );
    question.invoke_cancelled();

    // "yes" runs it on the selected files, and only them
    fixture.ui.invoke_menu_chosen(find(&jobs, blurhash_label));
    hydrus_gui::thumbnail_maintenance_window::question()
        .unwrap()
        .invoke_answered(true);
    wait_until("the blurhash is generated", || {
        blurhash(&fixture, jpeg).is_some()
    });
    assert_eq!(
        blurhash(&fixture, other),
        None,
        "the other file was left be"
    );
}

// leaf: audit-media-context-missing-refetch
#[test]
fn force_metadata_refetch_asks_then_opens_a_downloader_page_of_the_urls() {
    use hydrus_core::url::strings::StringMatch;
    use hydrus_core::url::{DomainMask, UrlClass, UrlClassSettings};

    // (oracle/record_force_refetch.py: the reference's rows, questions, the
    // URLs sent to the "forced urls downloader" page and its options, for one
    // file and for three, each row answered no then yes)
    let recorded = hydrus_testkit::fixture_json("force_refetch.json");
    let fixture = start_local();
    let classes = UrlClassSettings {
        url_classes: vec![UrlClass {
            name: "example post".into(),
            domain_mask: DomainMask::new(vec!["example.com".into()], vec![], false, false),
            path_components: vec![
                (StringMatch::fixed("post"), None),
                (StringMatch::any(), None),
            ],
            url_type: hydrus_core::url::UrlType::Post,
            ..UrlClass::default()
        }],
        ..UrlClassSettings::default()
    };
    fixture
        .store
        .write_and_refresh(move |ctx| hydrus_store::settings::set(ctx.conn(), &classes))
        .unwrap();
    let results = fixture.results();
    let hashes = fixture
        .store
        .read(|c| hydrus_store::master::hashes(c, &results))
        .unwrap();
    let names: Vec<String> = results.iter().map(|f| hashes[f].to_hex()).collect();
    let tab_names = |fixture: &Fixture| -> Vec<String> {
        fixture
            .bound
            .pages
            .borrow()
            .session()
            .pages
            .iter()
            .map(|page| page.name.clone())
            .collect()
    };
    let requested = |fixture: &Fixture| -> Vec<String> {
        let key = fixture
            .bound
            .pages
            .borrow()
            .session()
            .pages
            .iter()
            .find(|page| page.name == "forced urls downloader")
            .unwrap()
            .key;
        let downloader = fixture.bound.pages.borrow_mut().page(&key).unwrap();
        let queue = downloader.borrow().importer().unwrap().queue;
        fixture
            .store
            .write(move |ctx| hydrus_store::queues::take_url_requests(ctx.conn(), queue))
            .unwrap()
    };
    let mut yes_answers = 0;
    for case in recorded["cases"].as_array().unwrap() {
        let picked: Vec<i32> = case["hashes"]
            .as_array()
            .unwrap()
            .iter()
            .map(|h| {
                i32::try_from(names.iter().position(|n| n == h.as_str().unwrap()).unwrap()).unwrap()
            })
            .collect();
        fixture.select(picked[0], &picked[1..]);
        let menu = fixture.menu(picked[0]);
        let refetch = group_rows(&menu.urls_refetch);
        let theirs: Vec<&str> = case["rows"]
            .as_array()
            .unwrap()
            .iter()
            .map(|r| r["row"].as_str().unwrap())
            .collect();
        // (each row twice in the recording, answered no then yes)
        let mut offered: Vec<&str> = theirs.clone();
        offered.dedup();
        assert_eq!(
            refetch.iter().map(|(l, _)| l.as_str()).collect::<Vec<_>>(),
            offered,
            "{}: the rows",
            case["selection"]
        );
        for row in case["rows"].as_array().unwrap() {
            let id = refetch
                .iter()
                .find(|(l, _)| l == row["row"].as_str().unwrap())
                .unwrap()
                .1;
            let tabs = tab_names(&fixture).len();
            fixture.ui.invoke_menu_chosen(id);
            assert_eq!(
                fixture.ui.get_question(),
                row["asked"][0]["message"].as_str().unwrap(),
                "{}: {}",
                case["selection"],
                row["row"]
            );
            let yes = row["answer"].as_bool().unwrap();
            fixture.ui.invoke_answer(yes);
            if !yes {
                assert_eq!(tab_names(&fixture).len(), tabs, "no: no page");
                continue;
            }
            yes_answers += 1;
            let page = &row["page"];
            assert_eq!(
                tab_names(&fixture).len() - tabs,
                page["tabs_added"].as_u64().unwrap() as usize,
                "a new page the first time, the same one after"
            );
            assert!(
                tab_names(&fixture)
                    .iter()
                    .any(|n| n == page["name"].as_str().unwrap())
            );
            // the URLs the page was sent (requests the page takes up as its
            // queue), and the options that fetch metadata even for known URLs
            // and hashes
            let mut sent: Vec<String> = row["sent"][0]
                .as_array()
                .unwrap()
                .iter()
                .map(|u| u.as_str().unwrap().to_owned())
                .collect();
            sent.sort();
            let mut now = requested(&fixture);
            now.sort();
            assert_eq!(now, sent, "the URLs sent to the page");
            let key = fixture
                .bound
                .pages
                .borrow()
                .session()
                .pages
                .iter()
                .find(|page| page.name == "forced urls downloader")
                .unwrap()
                .key;
            let downloader = fixture.bound.pages.borrow_mut().page(&key).unwrap();
            let queue = downloader.borrow().importer().unwrap().queue;
            let options = fixture
                .store
                .read(|c| hydrus_store::queues::queue(c, queue))
                .unwrap()
                .unwrap()
                .options;
            let prefetch = options.prefetch.expect("prefetch options");
            assert_eq!(
                prefetch.fetch_metadata_even_if_url_recognised_and_file_already_in_db,
                page["fetch_even_if_url_known"].as_bool().unwrap()
            );
            assert_eq!(
                prefetch.fetch_metadata_even_if_hash_recognised_and_file_already_in_db,
                page["fetch_even_if_hash_known"].as_bool().unwrap()
            );
        }
    }
    assert_eq!(yes_answers, 3);
}

/// Every row of the thumbnail menu's file relationships submenu, with its id
/// (the rows are in groups, in the order the submenu lists them).
fn relationship_rows(menu: &hydrus_gui::ThumbnailMenu) -> Vec<(String, i32)> {
    [
        &menu.rel_b1,
        &menu.rel_b2,
        &menu.rel_b3,
        &menu.rel_b4,
        &menu.rel_merge,
        &menu.rel_after,
        &menu.rel_remove_one,
        &menu.rel_reset_one_a,
        &menu.rel_reset_one_b,
        &menu.rel_remove_all,
        &menu.rel_reset_all_a,
        &menu.rel_reset_all_b,
        &menu.rel_reset_all_c,
    ]
    .into_iter()
    .flat_map(rows)
    .collect()
}

/// The labels at the leaves of a recorded menu tree.
fn recorded_leaves(entries: &serde_json::Value, out: &mut Vec<String>) {
    for entry in entries.as_array().unwrap() {
        match entry.as_str() {
            Some(label) => out.push(label.to_owned()),
            None => recorded_leaves(&entry["entries"], out),
        }
    }
}

// leaf: audit-media-context-missing-duplicates
#[test]
fn file_relationship_rows_set_view_and_reset_duplicate_groups() {
    use hydrus_gui_model::file_relationships::read;
    use hydrus_store::duplicates::FileScope;

    // oracle/record_file_relationship_actions.py: on a page of the fixture's
    // JPEGs, the reference's real menu for a selection (the first file the
    // focus), the row triggered with every question captured and answered
    // yes, and the relationships of the six files involved read back
    let recorded = hydrus_testkit::fixture_json("file_relationship_actions.json");
    let fixture = start();
    let results = fixture.results();
    let hashes = fixture
        .store
        .read(|c| hydrus_store::master::hashes(c, &results))
        .unwrap();
    let position = |hex: &str| {
        results
            .iter()
            .position(|f| hashes[f].to_hex() == hex)
            .unwrap_or_else(|| panic!("{hex} is in the fixture"))
    };
    let scope = FileScope::AllKnownFiles;
    let names = recorded["names"].as_object().unwrap();
    let check_infos = |expected: &serde_json::Value, context: &str| {
        for (name, want) in expected.as_object().unwrap() {
            let file = results[position(names[name].as_str().unwrap())];
            let info = read(&fixture.store, &scope, file).unwrap().0;
            let count = |key: &str| {
                want["counts"]
                    .get(key)
                    .and_then(serde_json::Value::as_u64)
                    .unwrap_or(0) as usize
            };
            assert_eq!(
                (
                    info.is_king,
                    info.duplicates,
                    info.alternates,
                    info.false_positives,
                    info.potentials
                ),
                (
                    want["is_king"].as_bool().unwrap(),
                    count("duplicates"),
                    count("alternates"),
                    count("not related/false positive"),
                    count("potential duplicates")
                ),
                "{context}: {name}"
            );
        }
    };
    check_infos(&recorded["initial"], "before anything");
    for step in recorded["steps"].as_array().unwrap() {
        if step.get("setup").is_some() {
            // (as the duplicate filter's "not related" decision sets it)
            let pair: Vec<_> = step["selection"]
                .as_array()
                .unwrap()
                .iter()
                .map(|h| results[position(h.as_str().unwrap())])
                .collect();
            hydrus_gui_model::duplicates_filtering::set_duplicates(
                &fixture.store,
                &pair,
                hydrus_store::duplicates::PairRelationship::FalsePositive,
                false,
            )
            .unwrap();
            check_infos(&step["after"], "after setting a false positive");
            continue;
        }
        let picked: Vec<i32> = step["selection"]
            .as_array()
            .unwrap()
            .iter()
            .map(|h| i32::try_from(position(h.as_str().unwrap())).unwrap())
            .collect();
        let path: Vec<&str> = step["path"]
            .as_array()
            .unwrap()
            .iter()
            .map(|p| p.as_str().unwrap())
            .collect();
        let context = format!("{:?} on {:?}", path, step["selection"]);
        fixture.ui.invoke_select_none();
        fixture.select(picked[0], &picked[1..]);
        let menu = fixture.menu(picked[0]);
        // the submenu offers what the reference's does
        let offered = relationship_rows(&menu);
        let mut theirs = Vec::new();
        recorded_leaves(&step["menu"], &mut theirs);
        let mut ours: Vec<String> = offered.iter().map(|(label, _)| label.clone()).collect();
        // (the reference's "set a relationship with custom metadata merge
        // options", which asks for a status and merge options in a dialog, is
        // not ported)
        theirs.retain(|label| label != "set a relationship with custom metadata merge options");
        theirs.sort();
        ours.sort();
        assert_eq!(ours, theirs, "{context}: the rows");
        let row = path.last().unwrap();
        let tabs = fixture.bound.pages.borrow().tabs()[0].names.len();
        if !step["offered"].as_bool().unwrap() {
            assert!(offered.iter().all(|(label, _)| label != row), "{context}");
            continue;
        }
        let popup_texts = || -> Vec<String> {
            fixture
                .store
                .read(|c| hydrus_store::popups::all(c, hydrus_core::TimestampMs::now().0 / 1000))
                .unwrap()
                .into_iter()
                .filter_map(|job| job.status_text_1)
                .collect()
        };
        let popups_before = popup_texts().len();
        let id = find(&offered, row);
        fixture.ui.invoke_menu_chosen(id);
        // the questions asked, and their answer
        let asked = step["asked"].as_array().unwrap();
        if asked.is_empty() {
            assert!(
                fixture.ui.get_question().is_empty(),
                "{context}: no question"
            );
        } else {
            assert_eq!(asked.len(), 1);
            assert_eq!(
                fixture.ui.get_question(),
                asked[0]["message"].as_str().unwrap(),
                "{context}: the question"
            );
            fixture.ui.invoke_answer(true);
        }
        match step["page"].as_object() {
            None => assert_eq!(
                fixture.bound.pages.borrow().tabs()[0].names.len(),
                tabs,
                "{context}: no page"
            ),
            Some(page) => {
                assert_eq!(
                    fixture.bound.pages.borrow().tabs()[0].names.len(),
                    tabs + page["tabs_added"].as_u64().unwrap() as usize,
                    "{context}: a page opens"
                );
                let mut shown: Vec<String> = fixture
                    .results()
                    .iter()
                    .map(|f| {
                        fixture
                            .store
                            .read(|c| hydrus_store::master::hashes(c, &[*f]))
                            .unwrap()[f]
                            .to_hex()
                    })
                    .collect();
                shown.sort();
                let want: Vec<&str> = page["hashes"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|h| h.as_str().unwrap())
                    .collect();
                assert_eq!(shown, want, "{context}: the page's files");
                fixture.ui.invoke_tab_chosen(0, 0);
            }
        }
        // what it says in popups (the false-positive clears' count)
        let said: Vec<String> = popup_texts().split_off(popups_before);
        let recorded_said: Vec<&str> = step["popups"]
            .as_array()
            .unwrap()
            .iter()
            .map(|t| t.as_str().unwrap())
            .collect();
        assert_eq!(said, recorded_said, "{context}: popups");
        check_infos(&step["after"], &context);
    }
}

// leaf: audit-media-context-open
#[test]
fn open_rows_launch_the_focused_file_and_open_pages_of_the_selection() {
    use std::cell::RefCell;
    use std::rc::Rc;

    let launched: Rc<RefCell<Vec<String>>> = Rc::default();
    hydrus_gui::set_launcher({
        let launched = launched.clone();
        move |target| launched.borrow_mut().push(format!("launch {target}"))
    });
    hydrus_gui::set_file_browser({
        let launched = launched.clone();
        move |path| launched.borrow_mut().push(format!("browse {path}"))
    });
    // oracle/record_open_menu_actions.py: the reference's real menu for two
    // selected JPEGs (the first the focus), with advanced mode off and on, and
    // what each row sends or opens
    let recorded = hydrus_testkit::fixture_json("open_menu_actions.json");
    let fixture = start();
    let results = fixture.results();
    let hashes = fixture
        .store
        .read(|c| hydrus_store::master::hashes(c, &results))
        .unwrap();
    let position = |hex: &str| {
        results
            .iter()
            .position(|f| hashes[f].to_hex() == hex)
            .unwrap()
    };
    let tab_names = |fixture: &Fixture| fixture.bound.pages.borrow().tabs()[0].names.clone();
    for case in recorded["cases"].as_array().unwrap() {
        let advanced = case["advanced"].as_bool().unwrap();
        fixture
            .store
            .write_and_refresh(move |ctx| {
                hydrus_store::settings::set(
                    ctx.conn(),
                    &hydrus_store::settings::AdvancedMode(advanced),
                )
            })
            .unwrap();
        let selection: Vec<usize> = case["selection"]
            .as_array()
            .unwrap()
            .iter()
            .map(|h| position(h.as_str().unwrap()))
            .collect();
        let focus = results[selection[0]];
        let path = hydrus_gui::thumbnail_menu::paths(&fixture.store, &[focus])
            .pop()
            .unwrap();
        // the recorded launches name the focused file's path: the payloads
        // are that path, here ours
        let recorded_path = case["file paths"][recorded["names"]
            .as_object()
            .unwrap()
            .iter()
            .find(|(_, h)| h.as_str() == case["selection"][0].as_str())
            .unwrap()
            .0
            .as_str()]
        .as_str()
        .unwrap();
        let mut leaves = Vec::new();
        recorded_leaves(&case["menu"], &mut leaves);
        fixture.select(
            i32::try_from(selection[0]).unwrap(),
            &[i32::try_from(selection[1]).unwrap()],
        );
        let menu = fixture.menu(i32::try_from(selection[0]).unwrap());
        let all: Vec<(String, i32)> = [&menu.open_a, &menu.open_similar, &menu.open_b]
            .into_iter()
            .flat_map(rows)
            .collect();
        let ours: Vec<&str> = all.iter().map(|(l, _)| l.as_str()).collect();
        let theirs: Vec<&str> = leaves.iter().map(String::as_str).collect();
        assert_eq!(ours, theirs, "advanced {advanced}: the rows");
        assert_eq!(menu.open_similar_title, "similar files in a new page");
        for row in case["rows"].as_array().unwrap() {
            let name = row["path"]
                .as_array()
                .unwrap()
                .last()
                .unwrap()
                .as_str()
                .unwrap();
            launched.borrow_mut().clear();
            let tabs = tab_names(&fixture);
            let menu = fixture.menu(i32::try_from(selection[0]).unwrap());
            let all: Vec<(String, i32)> = [&menu.open_a, &menu.open_similar, &menu.open_b]
                .into_iter()
                .flat_map(rows)
                .collect();
            fixture.ui.invoke_menu_chosen(find(&all, name));
            if let Some(accepted) = row.get("accepted") {
                // "custom": a dialog asks the distance, from the recording's
                // 10 in steps of 2, and cancelling opens nothing
                let dialog = &row["dialogs"][0];
                let editor = hydrus_gui::similar_distance_window::editor()
                    .expect("the distance dialog opens");
                assert_eq!(editor.get_window_title(), dialog["title"].as_str().unwrap());
                assert_eq!(
                    (
                        editor.get_value(),
                        editor.get_minimum(),
                        editor.get_maximum(),
                        editor.get_step_size()
                    ),
                    (
                        dialog["value"].as_i64().unwrap() as i32,
                        dialog["minimum"].as_i64().unwrap() as i32,
                        dialog["maximum"].as_i64().unwrap() as i32,
                        dialog["step"].as_i64().unwrap() as i32
                    )
                );
                if accepted.as_bool().unwrap() {
                    editor.set_value(dialog["answered"].as_i64().unwrap() as i32);
                    editor.invoke_apply();
                    let page = fixture.bound.current.borrow().clone();
                    assert_eq!(
                        page.borrow().predicates(),
                        row["log"][0]["kwargs"]["initial_predicates"]
                            .as_array()
                            .unwrap()
                            .iter()
                            .map(|p| p.as_str().unwrap())
                            .collect::<Vec<_>>()
                    );
                    assert_eq!(tab_names(&fixture).len(), tabs.len() + 1);
                    assert_eq!(
                        tab_names(&fixture).last().unwrap(),
                        row["page name"].as_str().unwrap()
                    );
                    fixture.ui.invoke_tab_chosen(0, 0);
                } else {
                    editor.invoke_cancel();
                    assert_eq!(tab_names(&fixture).len(), tabs.len(), "cancelled: no page");
                }
                assert!(hydrus_gui::similar_distance_window::editor().is_none());
                fixture.ui.invoke_select_none();
                fixture.select(
                    i32::try_from(selection[0]).unwrap(),
                    &[i32::try_from(selection[1]).unwrap()],
                );
                continue;
            }
            let log = row["log"].as_array().unwrap();
            let context = format!("advanced {advanced}: {name}");
            // the launches (the reference's file launch is sent the path;
            // its web browser a file URL of it; its file browser the path)
            let expected_launches: Vec<String> = log
                .iter()
                .filter_map(|entry| {
                    if let Some(params) = entry.get("launch file") {
                        assert_eq!(params[0].as_str().unwrap(), recorded_path);
                        Some(format!("launch {path}"))
                    } else if let Some(params) = entry.get("launch url") {
                        // (file:///<path>, where the reference has file:////<path>)
                        assert_eq!(
                            params[0].as_str().unwrap(),
                            format!("file:///{recorded_path}")
                        );
                        Some(format!("launch file://{}", path.replace(' ', "%20")))
                    } else {
                        entry.get("show in file browser").map(|shown| {
                            assert_eq!(shown.as_str().unwrap(), recorded_path);
                            format!("browse {path}")
                        })
                    }
                })
                .collect();
            assert_eq!(*launched.borrow(), expected_launches, "{context}");
            // the pages
            let added = row["tabs_added"].as_u64().unwrap() as usize;
            let now = tab_names(&fixture);
            assert_eq!(now.len(), tabs.len() + added, "{context}: pages opened");
            if added == 0 {
                continue;
            }
            let page = fixture.bound.current.borrow().clone();
            let pub_ = &log[0];
            let back = || fixture.ui.invoke_tab_chosen(0, 0);
            if pub_["pub"] == "new_page_duplicates" {
                let session = fixture.bound.pages.borrow().session().clone();
                let last = session.pages.last().unwrap();
                assert_eq!(last.name, row["page name"].as_str().unwrap(), "{context}");
                let hash_predicate = format!(
                    "{:?}",
                    match &last.content {
                        hydrus_core::pages::PageContent::Duplicates { duplicates, .. } =>
                            duplicates.search.search_1.predicates.clone(),
                        other => panic!("{other:?}"),
                    }
                );
                for h in &case["selection"].as_array().unwrap()[..] {
                    assert!(hash_predicate.contains(h.as_str().unwrap()), "{context}");
                }
                back();
                continue;
            }
            assert_eq!(
                now.last().unwrap(),
                row["page name"].as_str().unwrap(),
                "{context}: the page's name"
            );
            let mut shown: Vec<String> = page
                .borrow()
                .results()
                .iter()
                .map(|f| hashes[f].to_hex())
                .collect();
            shown.sort();
            let want: Vec<&str> = row["page hashes"]
                .as_array()
                .unwrap()
                .iter()
                .map(|h| h.as_str().unwrap())
                .collect();
            assert_eq!(shown, want, "{context}: the page's files");
            if let Some(predicates) = pub_["kwargs"].get("initial_predicates") {
                let want: Vec<&str> = predicates
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|p| p.as_str().unwrap())
                    .collect();
                assert_eq!(page.borrow().predicates(), want, "{context}: the search");
            }
            back();
        }
        fixture.select(
            i32::try_from(selection[0]).unwrap(),
            &[i32::try_from(selection[1]).unwrap()],
        );
    }
}

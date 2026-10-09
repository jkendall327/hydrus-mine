//! File actions from the thumbnail menu, through the real window and the real
//! store: archive and inbox, delete from a domain, undelete and physical
//! deletion (`ClientGUIMediaMenus`, `ClientGUIMediaActions`).

use hydrus_core::HashId;
use hydrus_gui::media_actions;
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
        fixture.ui.get_question(),
        format!("Delete this file from {domain}?")
    );
    fixture.ui.invoke_answer(false);
    assert_eq!(domains(&fixture, file), before);
    fixture.ui.invoke_menu_chosen(id);
    fixture.ui.invoke_answer(true);
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
    assert_eq!(reason.as_deref(), Some(media_actions::DELETE_REASON));

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
    assert_eq!(fixture.ui.get_question(), "Delete this file from art?");
    fixture.ui.invoke_answer(true);
    // (one domain left: back to the plain entry)
    let menu = fixture.menu(-1);
    assert_eq!(rows(&menu.delete).len(), 1);
    fixture
        .ui
        .invoke_menu_chosen(find(&rows(&menu.delete), "delete from my files"));
    assert_eq!(fixture.ui.get_question(), "Delete this file from my files?");
    fixture.ui.invoke_answer(true);
    assert_eq!(domains(&fixture, file), ["trash"]);
    // and then for good: asked, "no" changing nothing
    let menu = fixture.menu(-1);
    let physically = find(&rows(&menu.trash), "delete physically now");
    fixture.ui.invoke_menu_chosen(physically);
    assert_eq!(fixture.ui.get_question(), "Permanently delete this file?");
    fixture.ui.invoke_answer(false);
    assert!(domains(&fixture, file).contains(&"trash".to_owned()));
    fixture.ui.invoke_menu_chosen(physically);
    fixture.ui.invoke_answer(true);
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
    use hydrus_gui::thumbnail_menu::url_facts;

    let fixture = start();
    // the fixture's first file has https://example.com/post/0: a class for it
    let classes = UrlClassSettings {
        url_classes: vec![UrlClass {
            name: "example post".into(),
            domain_mask: DomainMask::new(vec!["example.com".into()], vec![], false, false),
            path_components: vec![
                (StringMatch::fixed("post"), None),
                (StringMatch::any(), None),
            ],
            url_type: hydrus_core::url::UrlType::File,
            ..UrlClass::default()
        }],
        ..UrlClassSettings::default()
    };
    fixture
        .store
        .write_and_refresh(move |ctx| hydrus_store::settings::set(ctx.conn(), &classes))
        .unwrap();
    let results = fixture.results();
    let (index, facts) = results
        .iter()
        .enumerate()
        .map(|(i, &f)| (i, url_facts(&fixture.store, Some(f), &[f])))
        .find(|(_, facts)| !facts.focus_classes.is_empty())
        .expect("a file with a recognised URL class");
    let index = i32::try_from(index).unwrap();
    fixture.select(index, &[]);
    let menu = fixture.menu(index);
    let refetch = group_rows(&menu.urls_refetch);
    let class = &facts.focus_classes[0];
    assert_eq!(refetch[0].0, format!("this file's {class} urls"));
    assert_eq!(class, "example post");
    let tabs_before = fixture.bound.pages.borrow().tabs()[0].names.len();
    fixture.ui.invoke_menu_chosen(refetch[0].1);
    let question = fixture.ui.get_question().to_string();
    assert!(
        question.starts_with(
            "Open a new search page and force metadata redownload for 1 \"example post\" URLs?"
        ),
        "{question}"
    );
    assert!(
        question.contains(&format!(" \"{class}\" URLs? This is inefficient and should only be done to fill in known gaps in one-time jobs.\n\nDO NOT USE THIS TO RECHECK TEN THOUSAND URLS EVERY MONTH JUST FOR MAYBE A FEW NEW TAGS.")),
        "{question}"
    );
    // "no": no page
    fixture.ui.invoke_answer(false);
    assert_eq!(
        fixture.bound.pages.borrow().tabs()[0].names.len(),
        tabs_before
    );
    // "yes": a new page, named for the job
    fixture.ui.invoke_menu_chosen(refetch[0].1);
    fixture.ui.invoke_answer(true);
    let names: Vec<String> = fixture
        .bound
        .pages
        .borrow()
        .session()
        .pages
        .iter()
        .map(|page| page.name.clone())
        .collect();
    assert_eq!(names.len(), tabs_before + 1);
    assert_eq!(names.last().unwrap(), "forced urls downloader");
    // whose import options fetch metadata even for known URLs and hashes
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
    assert!(prefetch.fetch_metadata_even_if_url_recognised_and_file_already_in_db);
    assert!(prefetch.fetch_metadata_even_if_hash_recognised_and_file_already_in_db);
}

// leaf: audit-media-context-missing-duplicates
#[test]
#[allow(clippy::many_single_char_names)]
fn file_relationship_rows_set_view_and_reset_duplicate_groups() {
    use hydrus_gui_model::file_relationships::read;
    use hydrus_store::duplicates::FileScope;

    let fixture = start();
    let results = fixture.results();
    let scope = FileScope::AllKnownFiles;
    let info = |file: HashId| read(&fixture.store, &scope, file).unwrap().0;
    // (the fixture's first files already have some relationships; these have none)
    let (a, b, e, f, c, d) = (
        results[3], results[4], results[5], results[6], results[8], results[9],
    );
    for file in [a, b, c, d, e, f] {
        let lone = info(file);
        assert!(!lone.in_group && lone.alternates + lone.potentials + lone.false_positives == 0);
    }
    fixture.select(3, &[4]);
    let menu = fixture.menu(3);
    assert!(menu.has_relationships);
    let better = find(
        &rows(&menu.rel_b2),
        "set this file as better than the 1 other selected",
    );
    fixture.ui.invoke_menu_chosen(better);
    let (focused, other) = (info(a), info(b));
    assert!(focused.in_group && focused.is_king && focused.duplicates == 1);
    assert!(other.in_group && !other.is_king);

    // the file now says so, and its group can be viewed in a new page
    let menu = fixture.menu(3);
    let first = rows(&menu.rel_b1);
    assert_eq!(
        first.iter().map(|(l, _)| l.as_str()).collect::<Vec<_>>(),
        [
            "this file is in a duplicate file group",
            "this is the best quality file of its group",
            "view 1 duplicates"
        ]
    );
    let tabs = fixture.bound.pages.borrow().tabs()[0].names.len();
    fixture
        .ui
        .invoke_menu_chosen(find(&first, "view 1 duplicates"));
    assert_eq!(fixture.bound.pages.borrow().tabs()[0].names.len(), tabs + 1);
    let shown = fixture.results();
    assert_eq!(shown.len(), 2);
    assert!(shown.contains(&a) && shown.contains(&b));
    fixture.ui.invoke_tab_chosen(0, 0);

    // dissolving the group undoes it
    fixture.select(3, &[4]);
    let menu = fixture.menu(3);
    let wipe = find(
        &rows(&menu.rel_reset_one_a),
        "DUPLICATE WIPE: dissolve this file's duplicate group",
    );
    fixture.ui.invoke_menu_chosen(wipe);
    if !fixture.ui.get_question().is_empty() {
        fixture.ui.invoke_answer(true);
    }
    assert!(!info(a).in_group && info(a).duplicates == 0 && !info(b).in_group);

    // alternates, and then potentials for another pair, and clearing those
    fixture.select(5, &[6]);
    let menu = fixture.menu(5);
    fixture
        .ui
        .invoke_menu_chosen(find(&rows(&menu.rel_b3), "set all selected as alternates"));
    assert_eq!((info(e).alternates, info(f).alternates), (1, 1));
    fixture.select(8, &[9]);
    let menu = fixture.menu(8);
    fixture.ui.invoke_menu_chosen(find(
        &rows(&menu.rel_after),
        "set all possible pair combinations as 'potential'",
    ));
    assert_eq!(info(c).potentials, 1);
    assert_eq!(info(d).potentials, 1);
    let menu = fixture.menu(8);
    let clear = find(
        &rows(&menu.rel_reset_all_c),
        "delete these files' potential relationships",
    );
    fixture.ui.invoke_menu_chosen(clear);
    if !fixture.ui.get_question().is_empty() {
        fixture.ui.invoke_answer(true);
    }
    assert_eq!(info(c).potentials, 0);
}

// leaf: audit-media-context-open
#[test]
fn open_rows_launch_the_focused_file_and_open_pages_of_the_selection() {
    use std::cell::RefCell;
    use std::rc::Rc;

    use hydrus_core::pages::PageContent;

    let launched: Rc<RefCell<Vec<String>>> = Rc::default();
    hydrus_gui::set_launcher({
        let launched = launched.clone();
        move |target| launched.borrow_mut().push(format!("launch {target}"))
    });
    hydrus_gui::set_file_browser({
        let launched = launched.clone();
        move |path| launched.borrow_mut().push(format!("browse {path}"))
    });
    let fixture = start();
    let results = fixture.results();
    fixture.select(3, &[5]);
    let (focus, other) = (results[3], results[5]);
    let path = hydrus_gui::thumbnail_menu::paths(&fixture.store, &[focus])
        .pop()
        .unwrap();
    let menu = fixture.menu(3);
    let launch = rows(&menu.open_b);
    assert_eq!(
        launch.iter().map(|(l, _)| l.as_str()).collect::<Vec<_>>(),
        [
            "focused file using Default OS File Launch",
            "focused file in web browser",
            "focused file in file browser"
        ]
    );
    // each launches the focused file (not the other selected one)
    for (_, id) in &launch {
        fixture.ui.invoke_menu_chosen(*id);
    }
    let url = format!("file://{}", path.replace(' ', "%20"));
    assert_eq!(
        launched.borrow().as_slice(),
        [
            format!("launch {path}"),
            format!("launch {url}"),
            format!("browse {path}")
        ]
    );
    // a page of the selected files, and a duplicates page searching them
    let menu = fixture.menu(3);
    let pages_before = fixture.bound.pages.borrow().session().pages.len();
    fixture
        .ui
        .invoke_menu_chosen(find(&rows(&menu.open_a), "in a new page"));
    let page = fixture.bound.current.borrow().clone();
    let mut files = page.borrow().results().to_vec();
    files.sort_unstable();
    let mut want = vec![focus, other];
    want.sort_unstable();
    assert_eq!(files, want);
    fixture.ui.invoke_tab_chosen(0, 0);
    fixture.select_files(&[focus, other]);
    let menu = fixture.menu(-1);
    fixture
        .ui
        .invoke_menu_chosen(find(&rows(&menu.open_a), "in a new duplicate filter page"));
    let session = fixture.bound.pages.borrow().session().clone();
    assert_eq!(session.pages.len(), pages_before + 2);
    let last = session.pages.last().unwrap();
    assert_eq!(last.name, "duplicates");
    let PageContent::Duplicates { duplicates, .. } = &last.content else {
        panic!("{:?}", last.content);
    };
    let hashes = fixture
        .store
        .read(|c| hydrus_store::master::hashes(c, &[focus, other]))
        .unwrap();
    let hash_predicate = format!("{:?}", duplicates.search.search_1.predicates);
    for hash in hashes.values() {
        assert!(
            hash_predicate.contains(&hash.to_string()),
            "{hash_predicate}"
        );
    }
}

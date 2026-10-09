//! The thumbnail menu's select and remove submenus, rearrange submenu and
//! share submenu, driven through the real window as the reference's
//! `ClientGUIMediaMenus` entries act on a page of the `basic` fixture's files.

use std::cell::RefCell;
use std::collections::HashSet;
use std::rc::Rc;

use hydrus_core::HashId;
use hydrus_gui::Clip;

use super::media_support::{Fixture, find, group_rows, in_domain, rows, start, start_local};

/// What each select row should select, worked out from the store.
fn expected(fixture: &Fixture, label: &str, selected: &[HashId]) -> Vec<HashId> {
    let results = fixture.results();
    let inbox: HashSet<HashId> = fixture
        .store
        .read(|c| hydrus_store::media::inboxed(c, &results))
        .unwrap()
        .into_iter()
        .collect();
    let domain =
        |name: &str| -> HashSet<HashId> { in_domain(&fixture.store, name).into_iter().collect() };
    let chosen: HashSet<HashId> = match label {
        l if l.starts_with("all") || l.starts_with("combined") => results.iter().copied().collect(),
        l if l.starts_with("inbox") => inbox,
        l if l.starts_with("archive") => results
            .iter()
            .filter(|f| !inbox.contains(f))
            .copied()
            .collect(),
        l if l.starts_with("art") => domain("art"),
        l if l.starts_with("my files") => domain("my files"),
        l if l.starts_with("not selected") => results
            .iter()
            .filter(|f| !selected.contains(f))
            .copied()
            .collect(),
        l if l.starts_with("none") => HashSet::new(),
        l if l.starts_with("selected") => selected.iter().copied().collect(),
        other => panic!("{other}"),
    };
    results.into_iter().filter(|f| chosen.contains(f)).collect()
}

// leaf: audit-media-context-selection
#[test]
fn select_and_remove_rows_count_and_act_on_each_filter_and_domain() {
    let fixture = start();
    let results = fixture.results();
    fixture.select(0, &[2]);
    let selected_before = fixture.selected();
    assert_eq!(selected_before.len(), 2);
    let menu = fixture.menu(0);
    let select = group_rows(&menu.select);
    let labels: Vec<_> = select.iter().map(|(l, _)| l.as_str()).collect();
    assert_eq!(labels.len(), 8, "{labels:?}");
    assert!(menu.has_select && menu.has_remove);
    // each select row's count is the number of files it selects, and choosing
    // it selects exactly those
    for (label, _) in &select {
        let want = expected(&fixture, label, &selected_before);
        if let Some(count) = label
            .rsplit_once(" (")
            .map(|(_, c)| c.trim_end_matches(')'))
        {
            assert_eq!(count, want.len().to_string(), "{label}");
        }
        fixture.select_files(&selected_before);
        let menu = fixture.menu(-1);
        fixture.ui.invoke_menu_chosen(find(
            &group_rows(&menu.select),
            label.split(" (").next().unwrap(),
        ));
        let mut got = fixture.selected();
        got.sort_unstable();
        let mut want = want;
        want.sort_unstable();
        assert_eq!(got, want, "select {label}");
    }
    let all_remove = {
        let fresh = fixture.another();
        fresh.select(0, &[2]);
        group_rows(&fresh.menu(-1).remove)
    };
    assert_eq!(all_remove.len(), 8);
    // each remove row takes those files out of the page, not the store
    for (label, _) in &all_remove {
        let fixture = fixture.another();
        fixture.select(0, &[2]);
        let selected = fixture.selected();
        let menu = fixture.menu(0);
        let rows_now = group_rows(&menu.remove);
        let (name, count) = label.rsplit_once(" (").unwrap();
        let want = expected(&fixture, name, &selected);
        assert_eq!(
            count.trim_end_matches(')'),
            want.len().to_string(),
            "{label}"
        );
        fixture.ui.invoke_menu_chosen(find(&rows_now, name));
        let removed: HashSet<HashId> = want.iter().copied().collect();
        let left: Vec<HashId> = results
            .iter()
            .copied()
            .filter(|f| !removed.contains(f))
            .collect();
        assert_eq!(fixture.results(), left, "remove {label}");
        // (still in the database)
        assert_eq!(in_domain(&fixture.store, "my files").len(), results.len());
    }
}

#[test]
fn rearrange_rows_move_the_selected_thumbnails_as_the_reference_does() {
    let fixture = start();
    let items = fixture.results();
    let order = |idx: &[usize]| -> Vec<HashId> { idx.iter().map(|&i| items[i]).collect() };
    let n = items.len();
    let rest = |skip: &[usize]| -> Vec<usize> { (0..n).filter(|i| !skip.contains(i)).collect() };
    let cat = |a: &[usize], b: &[usize]| -> Vec<usize> { a.iter().chain(b).copied().collect() };
    let labels = |fixture: &Fixture| -> Vec<String> {
        let menu = fixture.menu(-1);
        rows(&menu.rearrange).into_iter().map(|(l, _)| l).collect()
    };
    let choose = |fixture: &Fixture, label: &str| {
        let menu = fixture.menu(-1);
        fixture
            .ui
            .invoke_menu_chosen(find(&rows(&menu.rearrange), label));
    };

    // two together mid-page: all four moves
    fixture.select(5, &[6]);
    assert_eq!(
        labels(&fixture),
        ["to start", "back one", "forward one", "to end"]
    );
    choose(&fixture, "back one");
    let mut now = cat(&[0, 1, 2, 3], &[5, 6, 4]);
    now.extend(7..n);
    assert_eq!(fixture.results(), order(&now));
    assert_eq!(fixture.selected().len(), 2, "still selected");
    // (forward undoes back)
    choose(&fixture, "forward one");
    assert_eq!(fixture.results(), items);
    choose(&fixture, "forward one");
    let mut now2 = cat(&[0, 1, 2, 3, 4], &[7]);
    now2.extend([5, 6]);
    now2.extend(8..n);
    assert_eq!(fixture.results(), order(&now2));
    choose(&fixture, "to end");
    let mut now3 = rest(&[5, 6]);
    now3.extend([5, 6]);
    assert_eq!(fixture.results(), order(&now3));
    choose(&fixture, "to start");
    assert_eq!(fixture.results(), order(&cat(&[5, 6], &rest(&[5, 6]))));
    // a selection at the start can't go back; nor to start
    assert_eq!(labels(&fixture), ["forward one", "to end"]);

    // with gaps, "to here" gathers them at the focused thumbnail
    let fixture = fixture.another();
    fixture.select(5, &[1]);
    assert_eq!(
        labels(&fixture),
        ["to start", "back one", "to here", "forward one", "to end"]
    );
    choose(&fixture, "to here");
    assert_eq!(
        fixture.results(),
        order(&cat(
            &[0, 2, 3, 4, 6],
            &cat(&[1, 5], &(7..n).collect::<Vec<_>>())
        ))
    );
}

/// The hash of each file, in hex, as the recordings name them.
fn hex_names(fixture: &Fixture, files: &[HashId]) -> Vec<String> {
    let hashes = fixture
        .store
        .read(|c| hydrus_store::master::hashes(c, files))
        .unwrap();
    files.iter().map(|f| hashes[f].to_hex()).collect()
}

// leaf: audit-media-context-rearrange
#[test]
fn rearrange_menu_offers_and_makes_the_moves_the_reference_did() {
    let fixture = start_local();
    // (the recorded page is sorted by file size, smallest first)
    {
        let page = fixture.bound.current.borrow();
        let mut page = page.borrow_mut();
        page.set_sort_by(hydrus_search::SortBy::FileSize);
        page.set_sort_order(hydrus_search::SortOrder::Ascending);
    }
    let recorded = hydrus_testkit::fixture_json("thumbnail_rearrange.json");
    let initial: Vec<String> = recorded["initial"]
        .as_array()
        .unwrap()
        .iter()
        .map(|h| h.as_str().unwrap().to_owned())
        .collect();
    assert_eq!(hex_names(&fixture, &fixture.results()), initial, "the page");
    let strings = |v: &serde_json::Value| -> Vec<String> {
        v.as_array()
            .unwrap()
            .iter()
            .map(|h| h.as_str().unwrap().to_owned())
            .collect()
    };
    let mut made = 0;
    for case in recorded["cases"].as_array().unwrap() {
        let hits: Vec<i32> = case["indices"]
            .as_array()
            .unwrap()
            .iter()
            .map(|i| i32::try_from(i.as_u64().unwrap()).unwrap())
            .collect();
        let hit = |fixture: &Fixture| fixture.select(hits[0], &hits[1..]);
        fixture.bound.current.borrow().borrow_mut().refresh();
        hit(&fixture);
        let menu = fixture.menu(-1);
        let ours: Vec<String> = rows(&menu.rearrange).into_iter().map(|(l, _)| l).collect();
        let offered = if case["offered"].is_null() {
            Vec::new()
        } else {
            strings(&case["offered"])
        };
        assert_eq!(ours, offered, "the rearrange rows for {}", case["indices"]);
        for (name, theirs) in case["moves"].as_object().unwrap() {
            // (the page as it began)
            fixture.bound.current.borrow().borrow_mut().refresh();
            assert_eq!(hex_names(&fixture, &fixture.results()), initial);
            hit(&fixture);
            let expected = strings(&theirs["order"]);
            if offered.contains(name) {
                let menu = fixture.menu(-1);
                fixture
                    .ui
                    .invoke_menu_chosen(find(&rows(&menu.rearrange), name));
                made += 1;
            } else {
                // (not offered: the reference's move did nothing)
                assert_eq!(expected, initial, "{name} for {}", case["indices"]);
            }
            assert_eq!(
                hex_names(&fixture, &fixture.results()),
                expected,
                "{name} for {}",
                case["indices"]
            );
            let mut kept = hex_names(&fixture, &fixture.selected());
            kept.sort();
            let mut want = strings(&theirs["selected"]);
            want.sort();
            assert_eq!(kept, want, "still selected after {name}");
        }
    }
    assert!(made > 30, "{made}");
}

// leaf: audit-media-context-share
#[test]
fn share_rows_copy_the_selected_files_paths_hashes_and_ids() {
    let fixture = start();
    let copied: Rc<RefCell<Vec<Clip>>> = Rc::default();
    hydrus_gui::set_clipper({
        let copied = copied.clone();
        move |clip| copied.borrow_mut().push(clip.clone())
    });
    let results = fixture.results();
    // two still images, with pixel hashes
    let stills: Vec<usize> = fixture
        .store
        .read(|c| hydrus_store::media::load_basic(c, &results))
        .unwrap()
        .iter()
        .enumerate()
        .filter(|(_, m)| {
            m.info
                .as_ref()
                .is_some_and(|i| i.mime == hydrus_core::Mime::ImageJpeg)
        })
        .map(|(i, _)| i)
        .collect();
    assert!(stills.len() >= 2);
    let (first, second) = (
        i32::try_from(stills[0]).unwrap(),
        i32::try_from(stills[1]).unwrap(),
    );
    fixture.select(first, &[second]);
    let files = fixture.selected();
    assert_eq!(files.len(), 2);
    let basic = fixture
        .store
        .read(|c| hydrus_store::media::load_basic(c, &files))
        .unwrap();
    let last_text = || match copied.borrow().last() {
        Some(Clip::Text(text)) => text.clone(),
        other => panic!("{other:?}"),
    };
    let chosen = |group: &str, prefix: &str| {
        let menu = fixture.menu(-1);
        let list = match group {
            "hashes" => rows(&menu.share_hashes),
            "a" => rows(&menu.share_a),
            "b" => rows(&menu.share_b),
            "c" => rows(&menu.share_c),
            "hash" => rows(&menu.share_hash),
            _ => unreachable!(),
        };
        fixture.ui.invoke_menu_chosen(find(&list, prefix));
    };
    let expect_lines = |basic_hash: &dyn Fn(&hydrus_store::media::MediaResult) -> String| {
        files
            .iter()
            .map(|f| basic_hash(basic.iter().find(|m| m.hash_id == *f).unwrap()))
            .collect::<Vec<_>>()
            .join("\n")
    };
    // file ids, one per line
    chosen("b", "copy file ids");
    assert_eq!(last_text(), expect_lines(&|m| m.hash_id.get().to_string()));
    // sha256 are the files' hashes
    chosen("hashes", "sha256");
    assert_eq!(last_text(), expect_lines(&|m| m.hash.to_string()));
    // the others are the right lengths, one per file, and each is the digest
    // the file menu shows for the focused file
    for (prefix, length) in [("md5", 32), ("sha1", 40), ("sha512", 128)] {
        chosen("hashes", prefix);
        let text = last_text();
        let lines: Vec<&str> = text.lines().collect();
        assert_eq!(lines.len(), 2, "{prefix}");
        assert!(
            lines
                .iter()
                .all(|l| l.len() == length && l.bytes().all(|b| b.is_ascii_hexdigit())),
            "{text}"
        );
    }
    let menu = fixture.menu(-1);
    let focused_md5 = rows(&menu.share_hash)
        .into_iter()
        .find(|(l, _)| l.starts_with("md5"))
        .unwrap()
        .0;
    chosen("hashes", "md5");
    assert!(focused_md5.contains(last_text().lines().next().unwrap()));
    // blurhash and pixel hashes, as the store has them
    chosen("hashes", "blurhash");
    let blurhashes = last_text();
    assert!(!blurhashes.is_empty());
    chosen("hashes", "pixel hashes");
    let pixels = last_text();
    assert_eq!(pixels.lines().count(), 2);
    assert!(pixels.lines().all(|l| l.len() == 64));
    // paths: where the files are
    chosen("a", "copy paths");
    let snapshot = fixture.store.snapshot();
    let want = files
        .iter()
        .map(|f| {
            let m = basic.iter().find(|m| m.hash_id == *f).unwrap();
            snapshot
                .storage
                .file_path(&m.hash, m.info.as_ref().unwrap().mime)
                .unwrap()
                .display()
                .to_string()
        })
        .collect::<Vec<_>>()
        .join("\n");
    assert_eq!(last_text(), want);
    // the files themselves
    chosen("a", "copy files");
    match copied.borrow().last() {
        Some(Clip::Files(paths)) => assert_eq!(paths.len(), 2),
        other => panic!("{other:?}"),
    }
    // the focused file's own: its path, hash and id
    chosen("c", "copy path");
    let focus = fixture.bound.current.borrow().borrow().focused().unwrap();
    let focus_file = fixture.results()[focus];
    let m = basic.iter().find(|m| m.hash_id == focus_file).unwrap();
    assert_eq!(
        last_text(),
        snapshot
            .storage
            .file_path(&m.hash, m.info.as_ref().unwrap().mime)
            .unwrap()
            .display()
            .to_string()
    );
    chosen("hash", "sha256");
    assert_eq!(last_text(), m.hash.to_string());
}

//! The options window's tag presentation and tag sort pages against the
//! reference's panels: each control, and the tag lists drawing by the
//! values applied.

use slint::{ComponentHandle as _, Model as _};

use hydrus_core::Tag;
use hydrus_core::tag_presentation::TagPresentation;
use hydrus_gui::SearchPage;

use crate::options_gui_support::{Client, box_of, items, row, show_page};

/// Tags that tell the rendering options apart, on the first file.
const TAGS: [&str; 5] = [
    "creator:ab_cd",
    "3:three",
    "page:7",
    "title:\u{1F600}",
    "plain_tag",
];

fn add_tags(client: &Client) {
    let file = {
        let mut page = SearchPage::new(client.store.clone());
        page.add_predicate("system:everything");
        page.results()[0]
    };
    let own = client
        .store
        .snapshot()
        .services
        .by_name("my tags")
        .unwrap()
        .id;
    client
        .store
        .write_content(move |w| {
            for tag in TAGS {
                let tag = hydrus_store::master::intern_tag(w.conn(), &Tag::new(tag).unwrap())?;
                w.update_mappings(
                    own,
                    &hydrus_store::content::MappingAction::Add,
                    tag,
                    &[file],
                )?;
            }
            Ok(())
        })
        .unwrap();
}

/// The rows of a new page's tag list: every file's tags with their counts.
fn rows(client: &Client) -> Vec<String> {
    let mut page = SearchPage::new(client.store.clone());
    page.add_predicate("system:everything");
    page.tag_rows().iter().map(|r| (*r).to_owned()).collect()
}

/// The rows the five tags make, in the order of [`TAGS`] (each is on one
/// file; a tag not shown as given is looked for by what it should read).
fn shows(client: &Client, expected: [&str; 5]) {
    let rows = rows(client);
    for want in expected {
        assert!(
            rows.contains(&format!("{want} (1)")),
            "{want:?} is not among {rows:?}"
        );
    }
}

fn edit(client: &Client, change: &dyn Fn(&hydrus_gui::OptionsWindow)) {
    let options = client.open_options();
    show_page(&options, "tag presentation");
    change(&options);
    options.invoke_apply();
}

fn check(options: &hydrus_gui::OptionsWindow, label: &str, on: bool) {
    let (at, _) = row(options, label);
    options.invoke_check_toggled(at, on);
}

// leaf: audit-options-tag-presentation-namespace-rendering-show-namespaces
// leaf: audit-options-tag-presentation-namespace-rendering-show-namespace-if-it-is-a-number
// leaf: audit-options-tag-presentation-namespace-rendering-show-namespace-if-subtag-is-a-number
// leaf: audit-options-tag-presentation-namespace-rendering-if-shown-namespace-connecting-string
// leaf: audit-options-tag-presentation-other-rendering-experimental-replace-all-underscores-with-spaces
// leaf: audit-options-tag-presentation-other-rendering-experimental-replace-all-emojis-with
#[test]
fn namespace_and_text_rendering_options_change_how_the_tag_list_writes_tags() {
    let client = Client::basic();
    add_tags(&client);
    shows(
        &client,
        [
            "creator:ab_cd",
            "3:three",
            "page:7",
            "title:\u{1F600}",
            "plain_tag",
        ],
    );

    // the controls: the reference's labels and boxes, kinds and defaults
    let options = client.open_options();
    show_page(&options, "tag presentation");
    for (label, kind, checked, text, container) in [
        ("Show namespaces: ", 1, true, "", "namespace rendering"),
        (
            "Show namespace if it is a number: ",
            1,
            true,
            "",
            "namespace rendering",
        ),
        (
            "Show namespace if subtag is a number: ",
            1,
            true,
            "",
            "namespace rendering",
        ),
        (
            "If shown, namespace connecting string: ",
            6,
            false,
            ":",
            "namespace rendering",
        ),
        (
            "EXPERIMENTAL: Replace all underscores with spaces: ",
            1,
            false,
            "",
            "other rendering",
        ),
        (
            "EXPERIMENTAL: Replace all emojis with \u{25A1}: ",
            1,
            false,
            "",
            "other rendering",
        ),
    ] {
        let (_, r) = row(&options, label);
        assert_eq!(
            (r.kind, r.checked, r.text.as_str()),
            (kind, checked, text),
            "{label}"
        );
        assert_eq!(box_of(&options, label), container, "{label}");
    }
    options.invoke_cancel();

    // no namespaces: only the subtag, unless a number is involved
    edit(&client, &|o| check(o, "Show namespaces: ", false));
    shows(
        &client,
        ["ab_cd", "3:three", "page:7", "\u{1F600}", "plain_tag"],
    );
    // and the number namespace not shown either
    edit(&client, &|o| {
        check(o, "Show namespace if it is a number: ", false);
    });
    shows(
        &client,
        ["ab_cd", "three", "page:7", "\u{1F600}", "plain_tag"],
    );
    // and a number subtag not either
    edit(&client, &|o| {
        check(o, "Show namespace if subtag is a number: ", false);
    });
    shows(&client, ["ab_cd", "three", "7", "\u{1F600}", "plain_tag"]);
    let saved = client.setting::<TagPresentation>();
    assert!(
        !saved.show_namespaces
            && !saved.show_number_namespaces
            && !saved.show_subtag_number_namespaces
    );
    // namespaces on again, with another connecting string: all shown
    edit(&client, &|o| {
        check(o, "Show namespaces: ", true);
        let (at, _) = row(o, "If shown, namespace connecting string: ");
        o.invoke_text_edited(at, " = ".into());
    });
    shows(
        &client,
        [
            "creator = ab_cd",
            "3 = three",
            "page = 7",
            "title = \u{1F600}",
            "plain_tag",
        ],
    );
    // namespaces off, numbers on: the numbers are shown with that string
    edit(&client, &|o| {
        check(o, "Show namespaces: ", false);
        check(o, "Show namespace if it is a number: ", true);
        check(o, "Show namespace if subtag is a number: ", true);
    });
    shows(
        &client,
        ["ab_cd", "3 = three", "page = 7", "\u{1F600}", "plain_tag"],
    );
    edit(&client, &|o| {
        check(o, "Show namespaces: ", true);
        let (at, _) = row(o, "If shown, namespace connecting string: ");
        o.invoke_text_edited(at, ":".into());
    });
    shows(
        &client,
        [
            "creator:ab_cd",
            "3:three",
            "page:7",
            "title:\u{1F600}",
            "plain_tag",
        ],
    );
    // underscores as spaces
    edit(&client, &|o| {
        check(
            o,
            "EXPERIMENTAL: Replace all underscores with spaces: ",
            true,
        );
    });
    shows(
        &client,
        [
            "creator:ab cd",
            "3:three",
            "page:7",
            "title:\u{1F600}",
            "plain tag",
        ],
    );
    // emojis as boxes
    edit(&client, &|o| {
        check(
            o,
            "EXPERIMENTAL: Replace all underscores with spaces: ",
            false,
        );
        check(o, "EXPERIMENTAL: Replace all emojis with \u{25A1}: ", true);
    });
    shows(
        &client,
        [
            "creator:ab_cd",
            "3:three",
            "page:7",
            "title:\u{25A1}",
            "plain_tag",
        ],
    );
    let saved = client.setting::<TagPresentation>();
    assert!(saved.replace_emojis && !saved.replace_underscores);
}

// leaf: audit-options-tag-sort-tag-sort-default-tag-sort-in-search-pages
// leaf: audit-options-tag-sort-tag-sort-default-tag-sort-in-the-media-viewer
#[test]
fn the_default_tag_sorts_order_new_search_page_and_media_viewer_tag_lists() {
    use hydrus_core::tag_sort::{TagGroupBy, TagSort, TagSortType};
    use slint::Model as _;
    let client = Client::basic();
    add_tags(&client);
    let search_page_rows = |client: &Client| rows(client);
    let viewer_rows = |client: &Client| {
        let mut page = SearchPage::new(client.store.clone());
        page.add_predicate("system:everything");
        let viewer =
            hydrus_gui::MediaViewer::new(client.store.clone(), page.results().to_vec(), 0).unwrap();
        // (the file with the extra tags: look through the first few)
        let mut best = viewer.tag_rows();
        for index in 1..page.results().len() {
            let other =
                hydrus_gui::MediaViewer::new(client.store.clone(), page.results().to_vec(), index)
                    .unwrap();
            if other.tag_rows().len() > best.len() {
                best = other.tag_rows();
            }
        }
        best.into_iter().map(|(row, _)| row).collect::<Vec<_>>()
    };

    let options = client.open_options();
    show_page(&options, "tag sort");
    let search = "Default tag sort in search pages: ";
    let viewer = "Default tag sort in the media viewer: ";
    for label in [search, viewer] {
        let (_, r) = row(&options, label);
        assert_eq!(r.kind, 12, "{label}");
        assert_eq!(box_of(&options, label), "tag sort", "{label}");
        // the reference's default: tag, a-z, grouped by the user's namespaces
        assert_eq!(
            (r.index, r.order_index, r.group_index),
            (0, 0, 2),
            "{label}"
        );
        let types: Vec<String> = (0..r.items.row_count())
            .map(|i| r.items.row_data(i).unwrap().to_string())
            .collect();
        assert_eq!(types, ["sort by tag", "sort by subtag", "sort by count"]);
    }
    options.invoke_cancel();

    let sort_both = |sort_type: i32, order: i32, group: i32| {
        let options = client.open_options();
        show_page(&options, "tag sort");
        for label in [search, viewer] {
            let (at, _) = row(&options, label);
            options.invoke_tag_sort_chosen(at, 0, sort_type);
            options.invoke_tag_sort_chosen(at, 1, order);
            options.invoke_tag_sort_chosen(at, 2, group);
        }
        options.invoke_apply();
    };
    // a-z, not grouped; then z-a, which reverses every list
    sort_both(0, 0, 0);
    let presentation = client.setting::<TagPresentation>();
    let az = TagSort {
        sort_type: TagSortType::Tag,
        ascending: true,
        group_by: TagGroupBy::Nothing,
    };
    assert_eq!(
        (
            presentation.search_page_sort,
            presentation.media_viewer_sort
        ),
        (az, az)
    );
    let (page_az, viewer_az) = (search_page_rows(&client), viewer_rows(&client));
    assert!(page_az.len() > 10 && viewer_az.len() >= TAGS.len());
    sort_both(0, 1, 0);
    let presentation = client.setting::<TagPresentation>();
    let za = TagSort {
        ascending: false,
        ..az
    };
    assert_eq!(
        (
            presentation.search_page_sort,
            presentation.media_viewer_sort
        ),
        (za, za)
    );
    let reversed = |mut v: Vec<String>| {
        v.reverse();
        v
    };
    assert_eq!(search_page_rows(&client), reversed(page_az));
    assert_eq!(viewer_rows(&client), reversed(viewer_az));
}

// leaf: audit-options-tag-suggestions-suggested-tags-default-notebook-page
#[test]
fn the_default_suggested_tags_page_is_the_one_manage_tags_opens_on() {
    use hydrus_store::settings::TagSuggestionSettings;
    let client = Client::basic();
    // a recent tag, so that its page is offered
    let mine = client
        .store
        .snapshot()
        .services
        .by_name("my tags")
        .unwrap()
        .clone();
    let (service, key) = (mine.id, mine.key.clone());
    client
        .store
        .write(move |ctx| {
            // (and a most used tag, so that its page is offered too)
            let mut tabs: hydrus_store::settings::TagAutocompleteTabs =
                hydrus_store::settings::get(ctx.conn())?;
            tabs.most_used
                .insert(key.to_hex(), vec!["suggest:favourite".to_owned()]);
            hydrus_store::settings::set(ctx.conn(), &tabs)?;
            let tag =
                hydrus_store::master::intern_tag(ctx.conn(), &Tag::new("suggest:recent").unwrap())?;
            ctx.conn().execute(
                "INSERT INTO recent_tags(service_id,tag_id,used_ms) VALUES(?,?,?)",
                rusqlite::params![service, tag, hydrus_core::time::TimestampMs::now().0],
            )?;
            Ok(())
        })
        .unwrap();
    let label = "Default notebook page: ";
    client.ui.invoke_search_edited("system:everything".into());
    client.ui.invoke_search_accepted();
    let manage_tags_page = |client: &Client| {
        client.ui.invoke_select_all();
        client.ui.invoke_manage_tags_selected();
        let manage = client
            .bound
            .manage_tags
            .borrow()
            .as_ref()
            .unwrap()
            .clone_strong();
        let mine = manage
            .get_service_names()
            .iter()
            .position(|s| s == "my tags")
            .unwrap();
        manage.invoke_service_chosen(i32::try_from(mine).unwrap());
        let page = manage.get_suggested_page();
        manage.invoke_cancel();
        page
    };
    let options = client.open_options();
    show_page(&options, "tag suggestions");
    let (_, r) = row(&options, label);
    assert_eq!(box_of(&options, label), "suggested tags");
    assert_eq!(
        items(&r),
        ["most used", "related", "file_lookup_scripts", "recent"]
    );
    assert_eq!(r.index, 1, "the reference's default is related");
    options.invoke_cancel();
    assert_eq!(manage_tags_page(&client), 2, "related");
    for (choice, saved, page) in [(3, "recent", 1), (0, "favourites", 0), (1, "related", 2)] {
        let options = client.open_options();
        show_page(&options, "tag suggestions");
        let (at, _) = row(&options, label);
        options.invoke_choice_chosen(at, choice);
        options.invoke_apply();
        assert_eq!(
            client.setting::<TagSuggestionSettings>().default_page,
            saved
        );
        assert_eq!(manage_tags_page(&client), page, "{saved}");
    }
}

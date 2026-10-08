//! The options window's gui pages and advanced pages against the
//! reference's panels: each control's label, box and kind, and that the
//! saved value reaches what reads it (the tab names, the close question,
//! the menus advanced mode extends).

use hydrus_gui::page_chooser::NewPage;

use crate::options_gui_support::{
    Client, box_of, hover_line, items, menu_lines, press_title, row, show_page,
};

// leaf: audit-options-advanced-advanced-mode
#[test]
fn advanced_mode_is_a_checkbox_that_shows_the_menu_entries_it_gates() {
    let client = Client::basic();
    let nudge = "nudge subscriptions awake";
    let pause_menu = |client: &Client| -> Vec<String> {
        press_title(&client.ui, "network");
        hover_line(&client.ui, "pause");
        let lines = menu_lines(&client.ui).pop().unwrap();
        client.ui.invoke_menu_dismissed();
        lines
    };
    assert!(
        !pause_menu(&client).iter().any(|l| l == nudge),
        "not in normal mode"
    );

    let options = client.open_options();
    show_page(&options, "advanced");
    // the only control of the page
    let (at, r) = row(&options, "Advanced mode: ");
    assert_eq!((r.kind, r.checked), (1, false));
    options.invoke_check_toggled(at, true);
    assert!(
        !client.setting::<hydrus_store::settings::AdvancedMode>().0,
        "waits for apply"
    );
    options.invoke_apply();
    assert!(client.setting::<hydrus_store::settings::AdvancedMode>().0);
    assert!(
        pause_menu(&client).iter().any(|l| l == nudge),
        "advanced mode adds it"
    );

    // and off again
    let options = client.open_options();
    show_page(&options, "advanced");
    let (at, r) = row(&options, "Advanced mode: ");
    assert!(r.checked, "shows the saved value");
    options.invoke_check_toggled(at, false);
    options.invoke_apply();
    assert!(!pause_menu(&client).iter().any(|l| l == nudge));
}

/// Tabs of the top row, as the window shows them.
fn top_tabs(client: &Client) -> Vec<String> {
    use slint::Model as _;
    let row = client.ui.get_tab_rows().row_data(0).unwrap();
    (0..row.names.row_count())
        .map(|i| row.names.row_data(i).unwrap().to_string())
        .collect()
}

/// Three top tabs: a search page with files and a long name, a page of
/// pages, and a URL import page with two imports waiting.
fn named_tabs(client: &Client) {
    use hydrus_store::queues::{self, NewFileSeed, SeedType};
    client.ui.invoke_search_edited("system:everything".into());
    client.ui.invoke_search_accepted();
    client
        .bound
        .pages
        .borrow_mut()
        .rename_shown("a rather long page name");
    client
        .bound
        .pages
        .borrow_mut()
        .new_page(&NewPage::Pages)
        .unwrap();
    client.bound.pages.borrow_mut().rename_shown("book");
    client
        .bound
        .pages
        .borrow_mut()
        .new_page(&NewPage::Urls)
        .unwrap();
    let queue = match &client.bound.pages.borrow().shown().content {
        hydrus_core::pages::PageContent::Downloader { queues, .. } => queues[0],
        other => panic!("a URL page, not {other:?}"),
    };
    client
        .store
        .write(move |ctx| {
            let seed = |n: u32| NewFileSeed {
                seed_type: SeedType::Url,
                data: format!("https://names.example/{n}.jpg"),
                data_for_comparison: format!("https://names.example/{n}.jpg"),
                source_time: None,
                referral_url: None,
                meta: queues::FileSeedMeta::default(),
            };
            queues::add_file_seeds(ctx.conn(), queue, &[seed(1), seed(2)], false, 0)?;
            queues::set_paused(ctx.conn(), queue, Some(true), Some(true))?;
            Ok(())
        })
        .unwrap();
    // (the URL page sits inside the page of pages: the top row shows that)
    (client.bound.sync)();
}

// leaf: audit-options-gui-pages-page-tab-names-max-characters-to-display-in-a-page-name
// leaf: audit-options-gui-pages-page-tab-names-show-page-file-count-after-its-name
// leaf: audit-options-gui-pages-page-tab-names-show-import-page-x-y-progress-after-its-name
// leaf: audit-options-gui-pages-page-tab-names-suffix-page-of-pages-tab-names-with-a-decorator-string
// leaf: audit-options-gui-pages-page-tab-names-decorator-string
#[test]
fn page_tab_name_options_are_the_references_and_rename_the_tabs() {
    let client = Client::basic();
    named_tabs(&client);
    assert_eq!(
        top_tabs(&client),
        ["a rather long page \u{2026} (30)", "pages (0/2) \u{2193}"],
        "the reference's defaults: 20 characters, counts if any, progress, the arrow"
    );
    let options = client.open_options();
    show_page(&options, "gui pages");
    for label in [
        "Max characters to display in a page name: ",
        "Show page file count after its name: ",
        "Show import page x/y progress after its name: ",
        "Suffix 'page of pages' tab names with a decorator string: ",
        "  Decorator string: ",
    ] {
        assert_eq!(box_of(&options, label), "page tab names", "{label}");
    }
    let (_, r) = row(&options, "Max characters to display in a page name: ");
    assert_eq!((r.kind, r.minimum, r.maximum, r.number), (2, 1, 256, 20));
    let (_, r) = row(&options, "Show page file count after its name: ");
    assert_eq!(r.kind, 5);
    assert_eq!(
        items(&r),
        [
            "for all pages",
            "for import pages",
            "for no pages",
            "for all pages, but only if greater than zero"
        ]
    );
    assert_eq!(r.index, 3, "the reference's default");
    let (_, r) = row(&options, "Show import page x/y progress after its name: ");
    assert_eq!((r.kind, r.checked), (1, true));
    let (_, r) = row(
        &options,
        "Suffix 'page of pages' tab names with a decorator string: ",
    );
    assert_eq!((r.kind, r.checked), (1, true));
    let (_, r) = row(&options, "  Decorator string: ");
    assert_eq!((r.kind, r.text.as_str()), (6, " \u{2193}"));
    options.invoke_cancel();
    assert_eq!(
        top_tabs(&client)[1],
        "pages (0/2) \u{2193}",
        "cancel changes nothing"
    );

    // (each edit is applied from a new window, as a user would)
    let edit = |change: &dyn Fn(&hydrus_gui::OptionsWindow)| {
        let options = client.open_options();
        show_page(&options, "gui pages");
        change(&options);
        options.invoke_apply();
    };
    // fewer characters: the name elided to 10 ("a rather " and the ellipsis)
    edit(&|o| {
        let (at, _) = row(o, "Max characters to display in a page name: ");
        o.invoke_number_edited(at, 10);
    });
    assert_eq!(
        top_tabs(&client),
        ["a rather \u{2026} (30)", "pages (0/2) \u{2193}"]
    );
    assert_eq!(
        client
            .setting::<hydrus_core::pages::PageNameSettings>()
            .max_chars,
        10
    );
    edit(&|o| {
        let (at, _) = row(o, "Max characters to display in a page name: ");
        o.invoke_number_edited(at, 20);
    });
    // the file counts: for all pages (a page of pages with no files says 0)
    let counts = |index: i32| {
        edit(&|o| {
            let (at, _) = row(o, "Show page file count after its name: ");
            o.invoke_choice_chosen(at, index);
        });
        top_tabs(&client)
    };
    assert_eq!(
        counts(0),
        [
            "a rather long page \u{2026} (30)",
            "pages (0 - 0/2) \u{2193}"
        ]
    );
    // for import pages: neither of these two is one
    assert_eq!(
        counts(1),
        ["a rather long page \u{2026}", "pages (0/2) \u{2193}"]
    );
    assert_eq!(
        counts(2),
        ["a rather long page \u{2026}", "pages (0/2) \u{2193}"]
    );
    assert_eq!(
        counts(3),
        ["a rather long page \u{2026} (30)", "pages (0/2) \u{2193}"]
    );
    // the import progress
    edit(&|o| {
        let (at, _) = row(o, "Show import page x/y progress after its name: ");
        o.invoke_check_toggled(at, false);
    });
    assert_eq!(
        top_tabs(&client),
        ["a rather long page \u{2026} (30)", "pages \u{2193}"]
    );
    assert!(
        !client
            .setting::<hydrus_core::pages::PageNameSettings>()
            .import_progress
    );
    // the decorator string, and whether it is added
    edit(&|o| {
        let (at, _) = row(o, "  Decorator string: ");
        o.invoke_text_edited(at, " >>".into());
    });
    assert_eq!(top_tabs(&client)[1], "pages >>");
    edit(&|o| {
        let (at, _) = row(
            o,
            "Suffix 'page of pages' tab names with a decorator string: ",
        );
        o.invoke_check_toggled(at, false);
    });
    assert_eq!(top_tabs(&client)[1], "pages");
    let saved = client.setting::<hydrus_core::pages::PageNameSettings>();
    assert!(!saved.decorate_notebooks);
    assert_eq!(saved.notebook_decorator, " >>", "kept while unused");
}

// leaf: audit-options-gui-pages-opening-and-closing-confirm-when-closing-a-non-empty-importer-page
#[test]
fn closing_a_non_empty_importer_page_asks_only_if_the_option_says_so() {
    use hydrus_store::queues::{self, NewFileSeed, SeedType};
    let client = Client::basic();
    let url_page = |client: &Client| {
        client
            .bound
            .pages
            .borrow_mut()
            .new_page(&NewPage::Urls)
            .unwrap();
        let queue = match &client.bound.pages.borrow().shown().content {
            hydrus_core::pages::PageContent::Downloader { queues, .. } => queues[0],
            other => panic!("a URL page, not {other:?}"),
        };
        client
            .store
            .write(move |ctx| {
                let url = "https://confirm.example/1.jpg".to_owned();
                let seed = NewFileSeed {
                    seed_type: SeedType::Url,
                    data: url.clone(),
                    data_for_comparison: url,
                    source_time: None,
                    referral_url: None,
                    meta: queues::FileSeedMeta::default(),
                };
                queues::add_file_seeds(ctx.conn(), queue, &[seed], false, 0)?;
                // (paused: not "still importing", only holding an import)
                queues::set_paused(ctx.conn(), queue, Some(true), Some(true))?;
                Ok(())
            })
            .unwrap();
        (client.bound.sync)();
    };

    let options = client.open_options();
    show_page(&options, "gui pages");
    let label = "Confirm when closing a non-empty importer page: ";
    let (at, r) = row(&options, label);
    assert_eq!((r.kind, r.checked), (1, true), "the reference's default");
    assert_eq!(box_of(&options, label), "opening and closing");
    options.invoke_cancel();

    url_page(&client);
    let tabs = top_tab_count(&client);
    client.ui.invoke_close_page();
    assert_eq!(
        client.ui.get_question(),
        "Close \"url import\"?\n\nThis is a urls import page holding 1 import objects."
    );
    client.ui.invoke_answer(false);
    assert_eq!(top_tab_count(&client), tabs, "declining keeps the page");

    let options = client.open_options();
    show_page(&options, "gui pages");
    options.invoke_check_toggled(at, false);
    options.invoke_apply();
    assert!(
        !client
            .setting::<hydrus_core::pages::DownloaderPageSettings>()
            .confirm_non_empty_close
    );
    client.ui.invoke_close_page();
    assert_eq!(client.ui.get_question(), "", "no question now");
    assert_eq!(top_tab_count(&client), tabs - 1, "closed at once");
}

fn top_tab_count(client: &Client) -> usize {
    client.bound.pages.borrow().session().all_pages().len()
}

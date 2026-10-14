//! The reference's named windows (file logs, manage tags, review services
//! and the rest) opening by their own frame location and keeping it as they
//! close, as `SetInitialTLWSizeAndPosition` and `SaveTLWSizeAndPosition` do
//! for every frame key: each opened as a user opens it, through the menus,
//! and closed by its window's frame, in every case the reference's real
//! window was put through (`oracle/record_frame_state.py`, recorded on the
//! media viewer's key; the functions are the same for every key). The main
//! window and the media viewer are covered in `shell_geometry.rs` and
//! `options_gui_frames.rs`; placement by gravity and position in
//! `tests/model/frame_placement.rs`, and the off-screen rescue in
//! `window_rescue.rs`.

// (window sizes are whole pixels)
#![allow(clippy::float_cmp)]

use std::sync::Arc;

use hydrus_core::windows::{FrameLocation, WindowSettings};
use hydrus_gui::{Bound, MainWindow, Pages, SearchPage, bind, headless};
use hydrus_store::Store;
use slint::ComponentHandle;
use slint::Model as _;

use crate::common::main_menu;

trait Shown {
    fn win(&self) -> &slint::Window;
}

impl<T: ComponentHandle> Shown for T {
    fn win(&self) -> &slint::Window {
        self.window()
    }
}

/// Manage tags opened from the viewer, which closes with it.
struct FromViewer {
    tags: hydrus_gui::ManageTagsWindow,
    viewer: hydrus_gui::MediaViewerWindow,
}

impl Shown for FromViewer {
    fn win(&self) -> &slint::Window {
        self.tags.window()
    }
}

impl Drop for FromViewer {
    fn drop(&mut self) {
        self.viewer.invoke_close_requested();
    }
}

struct Client {
    store: Arc<Store>,
    ui: MainWindow,
    bound: Bound,
    /// A folder of one file to import.
    work: tempfile::TempDir,
    _dirs: [tempfile::TempDir; 2],
}

impl Client {
    fn new() -> Self {
        let (dirs, store) = crate::subscriptions::store();
        let ui = MainWindow::new().unwrap();
        ui.show().unwrap();
        let bound = bind(&ui, Pages::single(SearchPage::new(store.clone())));
        ui.invoke_search_edited("system:everything".into());
        ui.invoke_search_accepted();
        Self {
            store,
            ui,
            bound,
            work: {
                let work = tempfile::tempdir().unwrap();
                std::fs::copy(
                    hydrus_testkit::fixture_path("media/bmp_24.bmp"),
                    work.path().join("bmp_24.bmp"),
                )
                .unwrap();
                work
            },
            _dirs: dirs,
        }
    }

    fn frame(&self, key: &str) -> FrameLocation {
        self.store
            .read(hydrus_store::settings::get::<WindowSettings>)
            .unwrap()
            .frame(key)
            .unwrap_or_else(|| panic!("{key} has a frame"))
            .clone()
    }

    fn keep(&self, key: &'static str, frame: FrameLocation) {
        self.store
            .write(move |ctx| {
                let mut settings: WindowSettings = hydrus_store::settings::get(ctx.conn())?;
                settings.set_frame(key, frame);
                hydrus_store::settings::set(ctx.conn(), &settings)
            })
            .unwrap();
    }

    /// The first thumbnail's menu, `pick` choosing an entry from it.
    fn thumbnail_menu(&self, pick: impl Fn(&hydrus_gui::ThumbnailMenu) -> i32) {
        self.ui.invoke_thumbnail_clicked(0, false, false);
        self.ui.invoke_thumbnail_menu_requested(0);
        let id = pick(&self.ui.get_thumbnail_menu());
        self.ui.invoke_menu_chosen(id);
    }

    /// A urls download page, for its file and search logs.
    fn download_page(&self) {
        self.ui.invoke_new_page();
        self.ui.invoke_chooser_pressed(4);
        self.ui.invoke_chooser_pressed(8);
    }
}

fn row(rows: &slint::ModelRc<hydrus_gui::MenuRow>, label: &str) -> i32 {
    rows.iter()
        .find(|r| r.label == label)
        .unwrap_or_else(|| panic!("{label} in the menu"))
        .id
}

fn opened<T: ComponentHandle + 'static>(
    slot: &std::rc::Rc<std::cell::RefCell<Option<T>>>,
    what: &str,
) -> Box<dyn Shown> {
    Box::new(
        slot.borrow()
            .as_ref()
            .map_or_else(|| panic!("{what} opened"), ComponentHandle::clone_strong),
    )
}

type Open = fn(&Client) -> Box<dyn Shown>;
/// How a dialog is accepted (a frame has none: it is kept as it closes).
type Accept = Option<fn(&Client)>;

fn apply<T: ComponentHandle + 'static>(
    slot: &std::rc::Rc<std::cell::RefCell<Option<T>>>,
    invoke: fn(&T),
) {
    let window = slot.borrow().as_ref().map(ComponentHandle::clone_strong);
    invoke(&window.expect("the dialog is open"));
}

/// Each frame key, how a user opens its window, and (a dialog) accepts it.
fn windows() -> Vec<(&'static str, Open, Accept)> {
    vec![
        (
            "review_services",
            |c| {
                main_menu::choose(&c.ui, &["services"], "review");
                opened(&c.bound.services_review, "review services")
            },
            None,
        ),
        (
            "mr_bones",
            |c| {
                main_menu::choose(&c.ui, &["database"], "how boned am I?");
                Box::new(hydrus_gui::how_boned_window::opened().expect("how boned opened"))
            },
            None,
        ),
        (
            "file_history_chart",
            |c| {
                main_menu::choose(&c.ui, &["database"], "view file history");
                opened(&c.bound.file_history, "file history")
            },
            None,
        ),
        (
            "manage_subscriptions_dialog",
            |c| Box::new(crate::subscriptions::open_dialog(&c.ui, &c.bound)),
            Some(|c| {
                apply(
                    &c.bound.subscriptions,
                    hydrus_gui::SubscriptionsWindow::invoke_apply,
                );
            }),
        ),
        (
            "edit_subscription_dialog",
            |c| {
                // (one to edit)
                c.store
                    .write(|ctx| {
                        let conn = ctx.conn();
                        if hydrus_store::subscriptions::subscriptions(conn)?.is_empty() {
                            hydrus_store::subscriptions::create_subscription(
                                conn,
                                "zz artist",
                                &hydrus_core::subscriptions::SubscriptionSettings::default(),
                            )?;
                        }
                        Ok(())
                    })
                    .unwrap();
                let list = crate::subscriptions::open_dialog(&c.ui, &c.bound);
                list.invoke_row_clicked(0, false, false);
                list.invoke_edit();
                let shown = opened(&c.bound.edit_subscription, "edit subscription");
                // (the list stays, hidden behind it, until this one is done)
                let _ = list.hide();
                shown
            },
            Some(|c| {
                apply(
                    &c.bound.edit_subscription,
                    hydrus_gui::EditSubscriptionWindow::invoke_apply,
                );
            }),
        ),
        (
            "manage_tags_dialog",
            |c| {
                c.ui.invoke_select_all();
                c.ui.invoke_manage_tags_selected();
                opened(&c.bound.manage_tags, "manage tags")
            },
            Some(|c| {
                apply(
                    &c.bound.manage_tags,
                    hydrus_gui::ManageTagsWindow::invoke_apply,
                );
            }),
        ),
        (
            "manage_tags_frame",
            |c| {
                c.ui.invoke_thumbnail_activated(0);
                let viewer = c.bound.viewer.borrow().as_ref().unwrap().clone_strong();
                viewer.invoke_manage_tags();
                let tags = c
                    .bound
                    .manage_tags
                    .borrow()
                    .as_ref()
                    .unwrap()
                    .clone_strong();
                Box::new(FromViewer { tags, viewer })
            },
            None,
        ),
        (
            "manage_urls_dialog",
            |c| {
                c.thumbnail_menu(|m| m.urls_manage);
                opened(&c.bound.manage_urls, "manage urls")
            },
            Some(|c| {
                apply(
                    &c.bound.manage_urls,
                    hydrus_gui::ManageUrlsWindow::invoke_apply,
                );
            }),
        ),
        (
            "manage_times_dialog",
            |c| {
                c.thumbnail_menu(|m| row(&m.manage, "times"));
                opened(&c.bound.manage_times, "manage times")
            },
            Some(|c| {
                apply(
                    &c.bound.manage_times,
                    hydrus_gui::ManageTimesWindow::invoke_apply,
                );
            }),
        ),
        (
            "manage_notes_dialog",
            |c| {
                c.thumbnail_menu(|m| row(&m.manage, "notes"));
                opened(&c.bound.manage_notes, "manage notes")
            },
            Some(|c| {
                apply(
                    &c.bound.manage_notes,
                    hydrus_gui::ManageNotesWindow::invoke_apply,
                );
            }),
        ),
        (
            "export_files_frame",
            |c| {
                c.thumbnail_menu(|m| row(&m.share_export, "export files"));
                opened(&c.bound.export_files.window, "export files")
            },
            None,
        ),
        (
            "file_import_status",
            |c| {
                c.download_page();
                c.ui.invoke_open_file_log();
                opened(&c.bound.file_log, "file log")
            },
            None,
        ),
        (
            "local_import_filename_tagging",
            |c| {
                // file > import files…, on a folder, then its tags button
                let review = || {
                    c.bound
                        .review_imports
                        .borrow()
                        .as_ref()
                        .map(|(w, _)| w.clone_strong())
                };
                if review().is_none() {
                    main_menu::choose(&c.ui, &["file"], "import files\u{2026}");
                    review()
                        .unwrap()
                        .invoke_path_entered(c.work.path().display().to_string().into());
                }
                let review = review().unwrap();
                for _ in 0..1000 {
                    if !review.get_working() && review.get_rows().row_count() > 0 {
                        break;
                    }
                    std::thread::sleep(std::time::Duration::from_millis(10));
                    slint::platform::update_timers_and_animations();
                }
                review.invoke_add_tags();
                opened(&c.bound.filename_tagging, "filename tagging")
            },
            Some(|c| {
                apply(
                    &c.bound.filename_tagging,
                    hydrus_gui::FilenameTaggingWindow::invoke_apply,
                );
            }),
        ),
        (
            "gallery_import_log",
            |c| {
                c.download_page();
                c.ui.invoke_open_search_log();
                opened(&c.bound.file_log, "search log")
            },
            None,
        ),
    ]
}

#[test]
#[allow(clippy::cast_possible_truncation)]
fn named_windows_open_by_their_frames_and_keep_them_as_the_reference_s_do() {
    let recorded = hydrus_testkit::fixture_json("frame_state.json");
    let cases = recorded["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 40);
    let _windows = headless::init();
    for (key, open, accept) in windows() {
        let client = Client::new();
        for case in cases {
            let frame = &case["frame"];
            let (maximised, fullscreen) = (
                frame["maximised"].as_bool().unwrap(),
                frame["fullscreen"].as_bool().unwrap(),
            );
            let remember_size = frame["remember_size"].as_bool().unwrap();
            let position = frame["last_position"]
                .as_array()
                .map(|p| (p[0].as_i64().unwrap() as i32, p[1].as_i64().unwrap() as i32));
            let user = case["user"].as_str().unwrap();
            let what = format!(
                "{key}: remember size {remember_size}, maximised {maximised}, fullscreen {fullscreen}, position {position:?}, then {user}"
            );
            client.keep(
                key,
                FrameLocation {
                    remember_size,
                    last_size: Some((700, 500)),
                    remember_position: position.is_some(),
                    last_position: position,
                    maximised,
                    fullscreen,
                    ..client.frame(key)
                },
            );

            // opened
            let shown = open(&client);
            let window = shown.win();
            let opened = &case["opened"];
            assert_eq!(
                (window.is_maximized(), window.is_fullscreen()),
                (
                    opened["maximised"].as_bool().unwrap(),
                    opened["fullscreen"].as_bool().unwrap()
                ),
                "{what}: opened"
            );
            if !maximised && !fullscreen && remember_size {
                let size = window.size().to_logical(window.scale_factor());
                assert_eq!(
                    (size.width, size.height),
                    (
                        opened["size"][0].as_f64().unwrap() as f32,
                        opened["size"][1].as_f64().unwrap() as f32
                    ),
                    "{what}: its size"
                );
            }

            // the user changes it, and closes it by its frame
            match user {
                "restored" => {
                    window.set_fullscreen(false);
                    window.set_maximized(false);
                }
                "maximised by hand" => {
                    window.set_fullscreen(false);
                    window.set_maximized(true);
                }
                "fullscreened by hand" => window.set_fullscreen(true),
                _ => {}
            }
            let after = &case["after_user"];
            assert_eq!(
                (
                    window.is_maximized() && !window.is_fullscreen(),
                    window.is_fullscreen()
                ),
                (
                    after["maximised"].as_bool().unwrap(),
                    after["fullscreen"].as_bool().unwrap()
                ),
                "{what}: after the user"
            );
            // (resized by hand, if it is a normal window that keeps its size)
            let resized = !window.is_maximized() && !window.is_fullscreen() && remember_size;
            if resized {
                window.set_size(slint::LogicalSize::new(640.0, 430.0));
            }
            match accept {
                Some(accept) => accept(&client),
                None => window.dispatch_event(slint::platform::WindowEvent::CloseRequested),
            }
            assert!(!window.is_visible(), "{what}: closed");
            drop(shown);

            // what is kept
            let kept = client.frame(key);
            let saved = &case["saved"];
            assert_eq!(
                kept.remember_size,
                saved["remember_size"].as_bool().unwrap(),
                "{what}"
            );
            assert_eq!(
                (kept.maximised, kept.fullscreen),
                (
                    saved["maximised"].as_bool().unwrap(),
                    saved["fullscreen"].as_bool().unwrap()
                ),
                "{what}: the switches kept"
            );
            if resized {
                assert_eq!(kept.last_size, Some((640, 430)), "{what}: the size kept");
            } else if saved["last_size"] == serde_json::json!([700, 500]) {
                assert_eq!(kept.last_size, Some((700, 500)), "{what}: the size kept");
            }
        }

        // a dialog cancelled (closed by its frame) keeps nothing
        if accept.is_some() {
            let before = FrameLocation {
                remember_size: true,
                last_size: Some((700, 500)),
                maximised: false,
                fullscreen: false,
                ..client.frame(key)
            };
            client.keep(key, before.clone());
            let shown = open(&client);
            let window = shown.win();
            window.set_size(slint::LogicalSize::new(640.0, 430.0));
            window.dispatch_event(slint::platform::WindowEvent::CloseRequested);
            assert!(!window.is_visible(), "{key}: cancelled");
            drop(shown);
            assert_eq!(
                client.frame(key),
                before,
                "{key}: nothing kept when cancelled"
            );
        }

        // with no size of its own, its default gravity grows it to the main
        // window's height less the padding (frame_placement.json: the main
        // window's client area 900 by 700; the display, which would also
        // bound it, isn't there to see headless)
        let placement = hydrus_testkit::fixture_json("frame_placement.json");
        let case = placement["cases"]
            .as_array()
            .unwrap()
            .iter()
            .find(|c| {
                c["frame"]["gravity"] == serde_json::json!([-1, 1])
                    && c["frame"]["position"] == "topleft"
                    && c["frame"]["remember_size"] == false
            })
            .unwrap();
        let main = &placement["main_client"];
        client.ui.window().set_size(slint::LogicalSize::new(
            main[0].as_f64().unwrap() as f32,
            main[1].as_f64().unwrap() as f32,
        ));
        // (the ordinary dialogs' frame grows windows across: its placement,
        // made as the window was, gives way to the window's own frame)
        client.keep(
            "regular_dialog",
            FrameLocation {
                default_gravity: (1, -1),
                ..client.frame("regular_dialog")
            },
        );
        client.keep(
            key,
            FrameLocation {
                remember_size: false,
                remember_position: false,
                last_size: None,
                last_position: None,
                default_gravity: (-1, 1),
                default_position: "topleft".into(),
                maximised: false,
                fullscreen: false,
            },
        );
        let shown = open(&client);
        let window = shown.win();
        // (laid out at a size of its own as it is shown, as the toolkit
        // does: then grown)
        assert_eq!(window.size().width, 0, "{key}: not laid out yet");
        window.set_size(slint::LogicalSize::new(400.0, 300.0));
        let grown = case["size"][1].as_f64().unwrap() as f32;
        let height = || window.size().to_logical(window.scale_factor()).height;
        let started = std::time::Instant::now();
        while height() != grown && started.elapsed() < std::time::Duration::from_secs(5) {
            slint::platform::update_timers_and_animations();
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        assert_eq!(height(), grown, "{key}: grown by its gravity");
        // (and once both have had the chance, its width is still its own)
        for _ in 0..20 {
            slint::platform::update_timers_and_animations();
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        assert_eq!(
            window.size().to_logical(window.scale_factor()).width,
            400.0,
            "{key}: not grown by the ordinary dialogs' frame"
        );
        window.dispatch_event(slint::platform::WindowEvent::CloseRequested);
    }
}

/// A window with no frame of its own (help > about) opens by the ordinary
/// dialogs' frame: grown by its gravity once laid out, as
/// `frame_placement.json` has it (the main window's client area 900 by
/// 700; the display isn't there to see headless).
#[test]
#[allow(clippy::cast_possible_truncation)]
fn other_windows_open_by_the_ordinary_dialogs_frame() {
    let placement = hydrus_testkit::fixture_json("frame_placement.json");
    let case = placement["cases"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| {
            c["frame"]["gravity"] == serde_json::json!([-1, 1])
                && c["frame"]["position"] == "topleft"
                && c["frame"]["remember_size"] == false
        })
        .unwrap();
    let _windows = headless::init();
    let client = Client::new();
    let main = &placement["main_client"];
    client.ui.window().set_size(slint::LogicalSize::new(
        main[0].as_f64().unwrap() as f32,
        main[1].as_f64().unwrap() as f32,
    ));
    client.keep(
        "regular_dialog",
        FrameLocation {
            default_gravity: (-1, 1),
            ..client.frame("regular_dialog")
        },
    );
    main_menu::choose(&client.ui, &["help"], "about");
    let about = opened(&client.bound.about, "about");
    let window = about.win();
    assert_eq!(window.size().width, 0, "not laid out yet");
    window.set_size(slint::LogicalSize::new(400.0, 300.0));
    let grown = case["size"][1].as_f64().unwrap() as f32;
    let size = || window.size().to_logical(window.scale_factor());
    let started = std::time::Instant::now();
    while size().height != grown && started.elapsed() < std::time::Duration::from_secs(5) {
        slint::platform::update_timers_and_animations();
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    assert_eq!(
        (size().width, size().height),
        (400.0, grown),
        "its own width, grown to the main window's height"
    );
}

/// A frame key's row edited in Options > gui > frame locations (the editor
/// child, then the options window's apply), as a user does.
fn edit_in_options(client: &Client, key: &str, change: &dyn Fn(&hydrus_gui::FrameLocationWindow)) {
    main_menu::choose(&client.ui, &["file"], "options\u{2026}");
    let options = client
        .bound
        .options
        .borrow()
        .as_ref()
        .expect("the options window")
        .clone_strong();
    crate::options_gui_support::show_page(&options, "gui");
    let rows = options.get_frame_rows();
    let at = (0..rows.row_count())
        .position(|i| rows.row_data(i).unwrap().cells.row_data(0).unwrap() == key)
        .unwrap_or_else(|| panic!("a row for {key}"));
    options.invoke_frame_clicked(at as i32, false, false);
    options.invoke_frame_action("edit".into());
    let child = client
        .bound
        .options_frame_child
        .borrow()
        .as_ref()
        .expect("the editor opens")
        .clone_strong();
    assert!(
        child
            .get_message()
            .starts_with(&format!("Setting frame location info for {key}.")),
        "{key}: {}",
        child.get_message()
    );
    change(&child);
    child.invoke_apply();
    options.invoke_apply();
}

/// Close a window as the user does: a frame by its close button, a dialog
/// by cancelling it.
fn dismiss(shown: &dyn Shown) {
    shown
        .win()
        .dispatch_event(slint::platform::WindowEvent::CloseRequested);
}

#[test]
#[allow(clippy::cast_possible_truncation)]
fn the_frame_editor_decides_how_each_named_window_next_opens() {
    let _windows = headless::init();
    let placement = hydrus_testkit::fixture_json("frame_placement.json");
    let case_both = placement["cases"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| {
            c["frame"]["gravity"] == serde_json::json!([1, 1])
                && c["frame"]["position"] == "topleft"
                && c["frame"]["remember_size"] == false
        })
        .unwrap();
    for (key, open, _) in windows() {
        let client = Client::new();
        // (the recorded main window's client area; a window is grown to it
        // less the padding, which the display would also bound if there were
        // one)
        let main = &placement["main_client"];
        let (main_width, main_height) = (
            main[0].as_f64().unwrap() as f32,
            main[1].as_f64().unwrap() as f32,
        );
        let both = (main_width - 48.0, main_height - 48.0);
        assert_eq!(
            both.1,
            case_both["size"][1].as_f64().unwrap() as f32,
            "the recorded height of a window grown on both axes"
        );
        client
            .ui
            .window()
            .set_size(slint::LogicalSize::new(main_width, main_height));
        let size_of = |window: &slint::Window| {
            let size = window.size().to_logical(window.scale_factor());
            (size.width, size.height)
        };

        // a size remembered, neither maximised nor fullscreen
        edit_in_options(&client, key, &|child| {
            child.set_remember_size(true);
            child.set_size_none(false);
            child.set_last_width(700);
            child.set_last_height(500);
            child.set_maximised(false);
            child.set_fullscreen(false);
        });
        assert_eq!(client.frame(key).last_size, Some((700, 500)), "{key}");
        let shown = open(&client);
        assert_eq!(size_of(shown.win()), (700.0, 500.0), "{key}: its size");
        assert!(
            !shown.win().is_maximized() && !shown.win().is_fullscreen(),
            "{key}"
        );
        dismiss(&*shown);

        // started maximised
        edit_in_options(&client, key, &|child| {
            child.set_remember_size(true);
            child.set_maximised(true);
            child.set_fullscreen(false);
        });
        let shown = open(&client);
        assert!(
            shown.win().is_maximized() && !shown.win().is_fullscreen(),
            "{key}: maximised"
        );
        dismiss(&*shown);

        // started fullscreen
        edit_in_options(&client, key, &|child| {
            child.set_maximised(false);
            child.set_fullscreen(true);
        });
        let shown = open(&client);
        assert!(shown.win().is_fullscreen(), "{key}: fullscreen");
        dismiss(&*shown);

        // no size of its own, grown by its gravity to the main window less
        // the padding on both axes
        edit_in_options(&client, key, &|child| {
            child.set_remember_size(false);
            child.set_maximised(false);
            child.set_fullscreen(false);
            child.set_gravity_x(0);
            child.set_gravity_y(0);
        });
        assert_eq!(client.frame(key).default_gravity, (1, 1), "{key}");
        let shown = open(&client);
        if shown.win().size().width == 0 {
            shown.win().set_size(slint::LogicalSize::new(400.0, 300.0));
        }
        let started = std::time::Instant::now();
        while size_of(shown.win()) != both && started.elapsed() < std::time::Duration::from_secs(5)
        {
            slint::platform::update_timers_and_animations();
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        assert_eq!(size_of(shown.win()), both, "{key}: grown");
        dismiss(&*shown);

        // width only: its own height is kept
        edit_in_options(&client, key, &|child| child.set_gravity_y(1));
        assert_eq!(client.frame(key).default_gravity, (1, -1), "{key}");
        let shown = open(&client);
        if shown.win().size().width == 0 {
            shown.win().set_size(slint::LogicalSize::new(400.0, 300.0));
        }
        let started = std::time::Instant::now();
        while size_of(shown.win()).0 != main_width - 48.0
            && started.elapsed() < std::time::Duration::from_secs(5)
        {
            slint::platform::update_timers_and_animations();
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        assert_eq!(
            size_of(shown.win()),
            (main_width - 48.0, 300.0),
            "{key}: width only"
        );

        // centred on the main window (a software window has no place to
        // read, so the place it is given is the one worked out for it)
        edit_in_options(&client, key, &|child| {
            child.set_remember_position(false);
            child.set_default_position(1);
        });
        let frame = client.frame(key);
        assert_eq!(frame.default_position, "center", "{key}");
        let placed = hydrus_gui::windows::placement(shown.win(), &frame, false);
        let (width, height) = placed.size;
        assert_eq!(
            placed.position,
            Some((
                (main_width as i32 - 1) / 2 - (width - 1) / 2,
                (main_height as i32 - 1) / 2 - (height - 1) / 2
            )),
            "{key}: centred"
        );
        dismiss(&*shown);
    }
}

#[test]
fn the_choice_dialogs_open_by_the_quick_select_and_yes_no_frames_and_keep_them_when_a_choice_is_made()
 {
    let _windows = headless::init();
    let client = Client::new();
    // (with no "no", the reference's `SelectFromListButtons`; with one, `GetYesYesNo`)
    for (key, no_label) in [("quick_select_dialog", ""), ("quick_yesno_dialog", "no")] {
        let ask = |title: &'static str| hydrus_gui::choice_buttons::Ask {
            title,
            message: "pick",
            choices: vec!["a".into(), "b".into()],
            no_label,
        };
        edit_in_options(&client, key, &|child| {
            child.set_remember_size(true);
            child.set_size_none(false);
            child.set_last_width(500);
            child.set_last_height(400);
        });
        let shown = hydrus_gui::choice_buttons::open(&ask("first"), |_| ())
            .unwrap()
            .expect("shown");
        let size = shown
            .window()
            .size()
            .to_logical(shown.window().scale_factor());
        assert_eq!((size.width, size.height), (500.0, 400.0), "{key}");
        // cancelled, it keeps nothing
        shown
            .window()
            .set_size(slint::LogicalSize::new(450.0, 350.0));
        shown.invoke_cancelled();
        assert_eq!(client.frame(key).last_size, Some((500, 400)), "{key}");
        // a choice keeps its size
        let shown = hydrus_gui::choice_buttons::open(&ask("second"), |_| ())
            .unwrap()
            .expect("shown");
        shown
            .window()
            .set_size(slint::LogicalSize::new(450.0, 350.0));
        shown.invoke_chosen(1);
        assert_eq!(client.frame(key).last_size, Some((450, 350)), "{key}");
    }

    // a message with no choices is neither dialog: it takes neither frame
    // (the reference shows those through its message dialogs) and keeps nothing
    let message = hydrus_gui::choice_buttons::Ask {
        title: "Information",
        message: "a message",
        choices: Vec::new(),
        no_label: "OK",
    };
    let shown = hydrus_gui::choice_buttons::open(&message, |_| ())
        .unwrap()
        .expect("shown");
    assert_ne!(
        shown
            .window()
            .size()
            .to_logical(shown.window().scale_factor())
            .width,
        500.0,
        "not opened by a quick frame"
    );
    shown
        .window()
        .set_size(slint::LogicalSize::new(333.0, 222.0));
    shown.invoke_cancelled();
    shown
        .window()
        .set_size(slint::LogicalSize::new(333.0, 222.0));
    for key in ["quick_select_dialog", "quick_yesno_dialog"] {
        assert_eq!(client.frame(key).last_size, Some((450, 350)), "{key}");
    }
}

//! Real Options text control, owned Apply/Cancel, saved consumers and canonical OR.
use hydrus_core::{
    Tag,
    search::predicate::Predicate,
    tag_presentation::{NamespaceColours, TagPresentation},
};
use hydrus_gui::{MainWindow, OptionsWindow, Pages, bind, headless};
use hydrus_store::{or_connector, settings};
use slint::{ComponentHandle as _, Model as _};
const LABEL: &str = "OR connecting string (on one line): ";
// Fixed 900x900, scale-one diagnostic of the existing one-line field. The
// physical copy fails closed if this finite coordinate no longer focuses it;
// fresh images must independently confirm the caption, field and glyphs.
fn capture_and_copy_reopened(
    native: &slint::platform::software_renderer::MinimalSoftwareWindow,
    window: &OptionsWindow,
    raw: &str,
    filename: &str,
) {
    use slint::platform::{Key, PointerEventButton, WindowAdapter as _, WindowEvent};
    use std::time::{Duration, Instant};

    assert!(std::ptr::eq(native.window(), window.window()));
    assert!(window.window().is_visible());
    let started = Instant::now();
    let mut previous = None;
    let pixels = loop {
        let pixels = headless::render(native, 900, 900);
        if started.elapsed() >= Duration::from_millis(35)
            && !window.window().has_active_animations()
            && previous.as_ref() == Some(&pixels)
        {
            break pixels;
        }
        assert!(
            started.elapsed() < Duration::from_secs(2),
            "{filename}: reopened Options did not settle; visible={}, animations={}",
            window.window().is_visible(),
            window.window().has_active_animations(),
        );
        previous = Some(pixels);
        std::thread::sleep(Duration::from_millis(5));
    };
    assert_eq!(window.window().size(), slint::PhysicalSize::new(900, 900));
    headless::save_png(
        &std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join(filename),
        &pixels,
        900,
        900,
    )
    .unwrap();

    if raw == " 🦊 " {
        // Inside this supported field's text area, excluding its border and
        // caret. Keep the PNG even if glyph painting regresses to blank again.
        let ink = (502..524)
            .flat_map(|y| (640..690).map(move |x| (y * 900 + x) * 4))
            .filter(|&pixel| {
                pixels[pixel..pixel + 3]
                    .iter()
                    .all(|channel| *channel < 180)
            })
            .count();
        assert!(
            ink >= 10,
            "{filename}: the saved fox must paint visible glyph ink"
        );
    }

    // Capture before caret/selection paint. Copy is real focused TextInput
    // SelectAll/Copy, not the row model or a text-edited callback.
    let sentinel = format!("impossible unopened connector clipboard: {filename}");
    assert_ne!(raw, sentinel);
    headless::set_clipboard_text(&sentinel);
    let position = slint::LogicalPosition::new(750.0, 514.0);
    native.dispatch_event(WindowEvent::PointerMoved { position });
    native.dispatch_event(WindowEvent::PointerPressed {
        position,
        button: PointerEventButton::Left,
    });
    native.dispatch_event(WindowEvent::PointerReleased {
        position,
        button: PointerEventButton::Left,
    });
    native.dispatch_event(WindowEvent::KeyPressed {
        text: Key::Control.into(),
    });
    for text in ["a", "c"] {
        native.dispatch_event(WindowEvent::KeyPressed { text: text.into() });
        native.dispatch_event(WindowEvent::KeyReleased { text: text.into() });
    }
    native.dispatch_event(WindowEvent::KeyReleased {
        text: Key::Control.into(),
    });
    assert_eq!(
        headless::clipboard_text().as_deref(),
        Some(raw),
        "{filename}: the actual focused field must copy every raw byte, including spaces and Unicode"
    );
    assert!(window.window().is_visible());
}

fn open(ui: &MainWindow, bound: &hydrus_gui::Bound) -> (OptionsWindow, i32) {
    ui.invoke_menu_title_pressed(0, 20.0, 22.0);
    let index = ui
        .get_menu_panes()
        .row_data(0)
        .unwrap()
        .lines
        .iter()
        .position(|r| r.label == "options…")
        .unwrap();
    ui.invoke_menu_line_clicked(0, i32::try_from(index).unwrap(), 0.0, 0.0, 0.0);
    let window = bound.options.borrow().as_ref().unwrap().clone_strong();
    let page = window
        .get_pages()
        .iter()
        .position(|p| p.text == "tag presentation")
        .unwrap();
    window.invoke_page_chosen(i32::try_from(page).unwrap());
    let row = window
        .get_rows()
        .iter()
        .position(|r| r.label == LABEL)
        .unwrap();
    (window, i32::try_from(row).unwrap())
}
#[test]
fn actual_options_raw_connector_preserves_live_or_label_colour_query_and_retired_owner() {
    let fixture = hydrus_testkit::fixture_json("or_connector.json");
    let (_dirs, store) = super::subscriptions::store();
    let windows = headless::init();
    let ui = MainWindow::new().unwrap();
    ui.show().unwrap();
    let bound = bind(
        &ui,
        Pages::single(super::common::all_local_page(store.clone())),
    );
    let predicate = Predicate::Or(vec![
        Predicate::Tag {
            tag: Tag::new("character:alpha").unwrap(),
            inclusive: true,
        },
        Predicate::Tag {
            tag: Tag::new("series:beta").unwrap(),
            inclusive: false,
        },
    ]);
    bound
        .current
        .borrow()
        .borrow_mut()
        .apply_or_editor(vec![predicate]);
    ui.invoke_refresh_page();
    let canonical = bound
        .current
        .borrow()
        .borrow()
        .favourite_to_save()
        .unwrap()
        .search
        .predicates;
    let original_colours: NamespaceColours = store.read(settings::get).unwrap();
    let (cancel, row) = open(&ui, &bound);
    assert_eq!(
        cancel
            .get_rows()
            .row_data(usize::try_from(row).unwrap())
            .unwrap()
            .text,
        fixture["loaded"].as_str().unwrap()
    );
    cancel.invoke_text_edited(row, "cancelled connector".into());
    cancel.invoke_cancel();
    cancel.invoke_apply();
    assert_eq!(
        store.read(or_connector::load).unwrap().text,
        fixture["cancel"]["after"]
    );
    let (hidden, row) = open(&ui, &bound);
    let before_hidden = store.read(or_connector::load).unwrap();
    hidden.hide().unwrap();
    hidden.invoke_text_edited(row, "hidden raw edit".into());
    hidden.show().unwrap();
    hidden.invoke_apply();
    assert_eq!(store.read(or_connector::load).unwrap(), before_hidden);
    for case in fixture["cases"].as_array().unwrap() {
        let before = store.read(or_connector::load).unwrap();
        let (edit, row) = open(&ui, &bound);
        edit.invoke_text_edited(row, case["shown"].as_str().unwrap().into());
        assert_eq!(store.read(or_connector::load).unwrap(), before);
        edit.hide().unwrap();
        edit.invoke_apply();
        assert_eq!(
            store.read(or_connector::load).unwrap(),
            before,
            "hidden Apply cannot persist a staged connector"
        );
        edit.show().unwrap();
        // Publish concurrent colour/presentation changes while this owner holds
        // an older Settings snapshot; saving this control must preserve them.
        store
            .write_and_refresh(|c| {
                let mut tags: TagPresentation = settings::get(c.conn())?;
                tags.namespace_connector = " = ".into();
                settings::set(c.conn(), &tags)?;
                let mut colours: NamespaceColours = settings::get(c.conn())?;
                colours.or_connector = Some("character".into());
                settings::set(c.conn(), &colours)
            })
            .unwrap();
        let tags: TagPresentation = store.read(settings::get).unwrap();
        let colours: NamespaceColours = store.read(settings::get).unwrap();
        edit.invoke_apply();
        ui.invoke_refresh_page();
        assert_eq!(store.read(or_connector::load).unwrap().text, case["saved"]);
        assert_eq!(store.read::<TagPresentation>(settings::get).unwrap(), tags);
        assert_eq!(
            store.read::<NamespaceColours>(settings::get).unwrap(),
            colours
        );
        let label = ui.get_predicates().row_data(0).unwrap();
        assert_eq!(label.text, "-series = beta OR character = alpha");
        let expected = colours.colour(Some("character"));
        assert_eq!(
            [
                label.colour.red(),
                label.colour.green(),
                label.colour.blue()
            ],
            expected
        );
        assert_eq!(
            bound
                .current
                .borrow()
                .borrow()
                .favourite_to_save()
                .unwrap()
                .search
                .predicates,
            canonical,
            "display setting never rewrites stored query syntax"
        );
        let (reopen, reopened_row) = open(&ui, &bound);
        assert_eq!(
            reopen
                .get_rows()
                .row_data(usize::try_from(reopened_row).unwrap())
                .unwrap()
                .text,
            case["reopened"].as_str().unwrap()
        );
        let raw = case["reopened"].as_str().unwrap();
        if let Some(filename) = match raw {
            " / custom / " => Some("or-connector-ascii-reopened-native.png"),
            " 🦊 " => Some("or-connector-fox-reopened-native.png"),
            _ => None,
        } {
            let native = windows.get(windows.count() - 1).unwrap();
            let full_settings = store.read(hydrus_gui::options::Settings::load).unwrap();
            let query = bound.current.borrow().borrow().favourite_to_save().unwrap();
            let live_rows: Vec<_> = ui
                .get_predicates()
                .iter()
                .map(|row| (row.text, row.colour))
                .collect();
            let control = reopen
                .get_rows()
                .row_data(usize::try_from(reopened_row).unwrap())
                .unwrap();
            assert_eq!(control.label, LABEL);
            assert_eq!(control.kind, 6);
            assert_eq!(control.text, raw);
            assert!(std::ptr::eq(
                bound.options.borrow().as_ref().unwrap().window(),
                reopen.window()
            ));
            capture_and_copy_reopened(&native, &reopen, raw, filename);
            assert_eq!(
                store.read(hydrus_gui::options::Settings::load).unwrap(),
                full_settings
            );
            assert_eq!(
                bound.current.borrow().borrow().favourite_to_save().unwrap(),
                query
            );
            assert_eq!(
                ui.get_predicates()
                    .iter()
                    .map(|row| (row.text, row.colour))
                    .collect::<Vec<_>>(),
                live_rows
            );
            assert_eq!(
                reopen
                    .get_rows()
                    .row_data(usize::try_from(reopened_row).unwrap())
                    .unwrap()
                    .text,
                raw
            );
            assert!(std::ptr::eq(
                bound.options.borrow().as_ref().unwrap().window(),
                reopen.window()
            ));
        }
        reopen.invoke_cancel();
        edit.invoke_text_edited(row, "retired owner".into());
        edit.invoke_apply();
        assert_eq!(store.read(or_connector::load).unwrap().text, case["saved"]);
    }
    assert_eq!(original_colours.or_connector.as_deref(), Some("system"));
    let (old, row) = open(&ui, &bound);
    old.invoke_text_edited(row, "obsolete binding".into());
    let saved = store.read(or_connector::load).unwrap();
    bound.pages.borrow_mut().save(1).unwrap();
    let successor = bind(&ui, Pages::open(store.clone()).unwrap());
    ui.invoke_refresh_page();
    old.invoke_apply();
    old.invoke_cancel();
    assert_eq!(store.read(or_connector::load).unwrap(), saved);
    let (reopen, row) = open(&ui, &successor);
    assert_eq!(
        reopen
            .get_rows()
            .row_data(usize::try_from(row).unwrap())
            .unwrap()
            .text,
        saved.text.as_str()
    );
    let native = windows.get(windows.count() - 1).unwrap();
    let pixels = headless::render(&native, 900, 900);
    headless::save_png(
        &std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join("or-connector-options-native.png"),
        &pixels,
        900,
        900,
    )
    .unwrap();
    reopen.invoke_cancel();
    let (closed, row) = open(&ui, &successor);
    closed.invoke_text_edited(row, "accepted-close draft".into());
    store
        .write(|c| {
            let mut settings: hydrus_store::settings::GuiSettings = settings::get(c.conn())?;
            settings.confirm_exit = false;
            settings::set(c.conn(), &settings)?;
            // This boundary tests completed exit, independently of due maintenance.
            let mut shutdown: hydrus_store::settings::ShutdownWork =
                hydrus_store::settings::get(c.conn())?;
            shutdown.action = 0;
            settings::set(c.conn(), &shutdown)
        })
        .unwrap();
    ui.window()
        .dispatch_event(slint::platform::WindowEvent::CloseRequested);
    assert!(!ui.window().is_visible());
    closed.show().unwrap();
    closed.invoke_text_edited(row, "retired re-shown edit".into());
    closed.invoke_apply();
    assert_eq!(store.read(or_connector::load).unwrap(), saved);
    let reopened = hydrus_store::Store::open(store.dir()).unwrap();
    assert_eq!(reopened.read(or_connector::load).unwrap(), saved);
}

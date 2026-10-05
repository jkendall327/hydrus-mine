//! Saved GUI edits reach real canonical viewer tag searches and retired owners refuse them.
use hydrus_core::{Tag, pages::PageContent, search::predicate::Predicate};
use hydrus_gui::{MainWindow, MediaViewerWindow, OptionsWindow, Pages, SearchPage, bind, headless};
use hydrus_store::settings::{self, GuiSettings, TagSearchActivation};
use slint::{
    ComponentHandle as _, LogicalPosition, Model as _,
    platform::{PointerEventButton, WindowEvent},
};

const ACTIVATE: &str =
    "Switch to main window when creating new file search page from media viewer: ";
fn options(ui: &MainWindow, bound: &hydrus_gui::Bound) -> OptionsWindow {
    ui.invoke_menu_title_pressed(0, 20.0, 20.0);
    let lines = ui.get_menu_panes().row_data(0).unwrap().lines;
    let at = lines
        .iter()
        .position(|line| line.label == "options\u{2026}")
        .unwrap();
    ui.invoke_menu_line_clicked(0, i32::try_from(at).unwrap(), 0.0, 0.0, 0.0);
    let options = bound.options.borrow().as_ref().unwrap().clone_strong();
    let page = options
        .get_pages()
        .iter()
        .position(|page| page.text == "gui")
        .unwrap();
    options.invoke_page_chosen(i32::try_from(page).unwrap());
    options
}
fn row(options: &OptionsWindow, label: &str) -> i32 {
    i32::try_from(
        options
            .get_rows()
            .iter()
            .position(|row| row.label == label)
            .unwrap(),
    )
    .unwrap()
}
fn request(viewer: &MediaViewerWindow, file: &str, tag: &str) {
    viewer.invoke_tag_search_requested(file.into(), tag.into());
}
fn count(bound: &hydrus_gui::Bound) -> usize {
    bound.pages.borrow().session().pages.len()
}

#[test]
fn actual_middle_press_uses_canonical_tag_location_defaults_and_saved_activation() {
    let (_directories, store) = crate::subscriptions::store();
    let windows = headless::init();
    let ui = MainWindow::new().unwrap();
    ui.show().unwrap();
    let bound = bind(&ui, Pages::single(SearchPage::new(store.clone())));
    ui.invoke_search_edited("system:filetype is jpeg".into());
    ui.invoke_search_accepted();
    let first = bound.current.borrow().borrow().files()[0];
    let location = bound.current.borrow().borrow().location().clone();
    let service = store.snapshot().services.by_name("my tags").unwrap().id;
    let tag = Tag::new("series:canonical_under_score").unwrap();
    let written_tag = tag.clone();
    store
        .write_content(move |writer| {
            let id = hydrus_store::master::intern_tag(writer.conn(), &written_tag)?;
            writer.update_mappings(
                service,
                &hydrus_store::content::MappingAction::Add,
                id,
                &[first],
            )
        })
        .unwrap();
    store
        .write(|ctx| {
            let mut presentation: hydrus_core::tag_presentation::TagPresentation =
                settings::get(ctx.conn())?;
            presentation.namespace_connector = " ⇢ ".into();
            presentation.replace_underscores = true;
            settings::set(ctx.conn(), &presentation)
        })
        .unwrap();
    ui.invoke_thumbnail_activated(0);
    let viewer = bound.viewer.borrow().as_ref().unwrap().clone_strong();
    let drawn = windows.get(windows.count() - 1).unwrap();
    for enabled in [false, true] {
        let options = options(&ui, &bound);
        options.invoke_check_toggled(row(&options, ACTIVATE), enabled);
        options.invoke_apply();
        assert_eq!(
            store
                .read(settings::get::<TagSearchActivation>)
                .unwrap()
                .activate_main,
            enabled
        );
        headless::render(&drawn, 1000, 650);
        let at = viewer
            .get_tag_identities()
            .iter()
            .position(|value| value == tag.as_str())
            .unwrap();
        let rendered = viewer.get_tags().row_data(at).unwrap().text;
        assert!(rendered.contains(" ⇢ "));
        assert!(rendered.contains("canonical under score"));
        assert_ne!(rendered, tag.as_str());
        let position =
            LogicalPosition::new(20.0, 6.0 + (at as f32 + 0.5) * viewer.get_tag_row_height());
        viewer
            .window()
            .dispatch_event(WindowEvent::PointerMoved { position });
        headless::render(&drawn, 1000, 650);
        assert!(viewer.get_tags_showing());
        let before = count(&bound);
        let requests = ui.get_tag_search_activation_requests();
        viewer.window().dispatch_event(WindowEvent::PointerPressed {
            position,
            button: PointerEventButton::Middle,
        });
        viewer
            .window()
            .dispatch_event(WindowEvent::PointerReleased {
                position,
                button: PointerEventButton::Middle,
            });
        assert_eq!(
            count(&bound),
            before + 1,
            "real hover middle press opens one page"
        );
        assert_eq!(
            ui.get_tag_search_activation_requests() - requests,
            i32::from(enabled)
        );
        let pages = bound.pages.borrow();
        let PageContent::Search { search, .. } = &pages.shown().content else {
            panic!("tag search did not create a search page");
        };
        assert_eq!(
            search.predicates,
            vec![Predicate::Tag {
                tag: tag.clone(),
                inclusive: true
            }]
        );
        assert_eq!(search.location, location);
        assert_eq!(
            search.tags.service,
            store
                .read(settings::get::<settings::SearchDefaults>)
                .unwrap()
                .tag_service
        );
        assert_eq!(pages.shown().name, tag.as_str());
    }
    let draft = options(&ui, &bound);
    draft.invoke_check_toggled(row(&draft, ACTIVATE), false);
    draft.invoke_cancel();
    let reopened = options(&ui, &bound);
    assert!(
        reopened
            .get_rows()
            .row_data(usize::try_from(row(&reopened, ACTIVATE)).unwrap())
            .unwrap()
            .checked
    );
    reopened.invoke_cancel();
    let pixels = headless::render_snapshot(&drawn, 1000, 650);
    headless::save_png(
        &std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join("viewer-tag-search.png"),
        &pixels,
        1000,
        650,
    )
    .unwrap();
}

#[test]
fn stale_file_hidden_closed_reshown_and_rebound_viewers_cannot_launch() {
    let (_directories, store) = crate::subscriptions::store();
    let _windows = headless::init();
    let ui = MainWindow::new().unwrap();
    ui.show().unwrap();
    let old = bind(&ui, Pages::single(SearchPage::new(store.clone())));
    ui.invoke_search_edited("system:filetype is jpeg".into());
    ui.invoke_search_accepted();
    let files = old.current.borrow().borrow().files();
    assert!(files.len() > 1);
    let service = store.snapshot().services.by_name("my tags").unwrap().id;
    store
        .write_content(move |writer| {
            let tag = hydrus_store::master::intern_tag(
                writer.conn(),
                &Tag::new("owner:shared tag").unwrap(),
            )?;
            writer.update_mappings(
                service,
                &hydrus_store::content::MappingAction::Add,
                tag,
                &files[..2],
            )
        })
        .unwrap();
    ui.invoke_thumbnail_activated(0);
    let viewer = old.viewer.borrow().as_ref().unwrap().clone_strong();
    let file = viewer.get_tag_file();
    let tag = slint::SharedString::from("owner:shared tag");
    let before = count(&old);
    viewer.invoke_context_menu_requested();
    let menu = viewer.get_context_menu();
    let period = [
        &menu.slideshow.g1,
        &menu.slideshow.g2,
        &menu.slideshow.g3,
        &menu.slideshow.g4,
    ]
    .into_iter()
    .find_map(|rows| {
        rows.iter()
            .find(|row| row.label == "custom interval")
            .map(|row| row.id)
    })
    .unwrap();
    viewer.invoke_menu_chosen(period);
    assert!(viewer.get_period_asked());
    request(&viewer, &file, &tag);
    assert_eq!(
        count(&old),
        before,
        "pending slideshow period blocks retained tag dispatch"
    );
    viewer.invoke_period_answered(false, "".into());
    store
        .write(|ctx| {
            let mut preferences: settings::DeletionPreferences = settings::get(ctx.conn())?;
            preferences.advanced = true;
            settings::set(ctx.conn(), &preferences)
        })
        .unwrap();
    viewer.invoke_delete();
    let deletion = old
        .viewer_deletion
        .borrow()
        .as_ref()
        .unwrap()
        .clone_strong();
    request(&viewer, &file, &tag);
    assert_eq!(
        count(&old),
        before,
        "the owned advanced-delete child blocks tag searches"
    );
    deletion.invoke_cancel();
    assert!(old.viewer_deletion.borrow().is_none());
    request(&viewer, &file, "absent:invented");
    viewer.invoke_next();
    assert_ne!(viewer.get_tag_file(), file);
    request(&viewer, &file, &tag);
    assert_eq!(count(&old), before);
    viewer.invoke_previous();
    viewer.hide().unwrap();
    request(&viewer, &file, &tag);
    viewer.show().unwrap();
    ui.hide().unwrap();
    request(&viewer, &file, &tag);
    ui.show().unwrap();
    assert_eq!(count(&old), before);
    request(&viewer, &file, &tag);
    assert_eq!(
        count(&old),
        before + 1,
        "ordinary hiding does not retire the owner"
    );
    viewer.invoke_close_requested();
    viewer.show().unwrap();
    request(&viewer, &file, &tag);
    assert_eq!(
        count(&old),
        before + 1,
        "accepted viewer close is permanent"
    );
    ui.invoke_search_edited("system:filetype is jpeg".into());
    ui.invoke_search_accepted();
    ui.invoke_thumbnail_activated(0);
    let retained = old.viewer.borrow().as_ref().unwrap().clone_strong();
    let file = retained.get_tag_file();
    let tag = retained.get_tag_identities().row_data(0).unwrap();
    let old_count = count(&old);
    let current = bind(&ui, Pages::single(SearchPage::new(store.clone())));
    let current_count = count(&current);
    request(&retained, &file, &tag);
    assert_eq!(count(&old), old_count);
    assert_eq!(
        count(&current),
        current_count,
        "old viewer cannot use successor's global launcher"
    );
    ui.invoke_search_edited("system:filetype is jpeg".into());
    ui.invoke_search_accepted();
    ui.invoke_thumbnail_activated(0);
    let live = current.viewer.borrow().as_ref().unwrap().clone_strong();
    let file = live.get_tag_file();
    let tag = live.get_tag_identities().row_data(0).unwrap();
    store
        .write(|ctx| {
            let mut policy: GuiSettings = settings::get(ctx.conn())?;
            policy.confirm_exit = true;
            settings::set(ctx.conn(), &policy)
        })
        .unwrap();
    ui.window().dispatch_event(WindowEvent::CloseRequested);
    ui.invoke_answer(false);
    let before = count(&current);
    request(&live, &file, &tag);
    assert_eq!(
        count(&current),
        before + 1,
        "declined main close preserves viewer producer"
    );
    ui.window().dispatch_event(WindowEvent::CloseRequested);
    ui.invoke_answer(true);
    assert!(!ui.window().is_visible());
    ui.show().unwrap();
    request(&live, &file, &tag);
    assert_eq!(
        count(&current),
        before + 1,
        "accepted main close permanently retires the producer even after re-show"
    );
}

#[test]
fn raw_name_apply_cancel_and_title_refresh_are_scoped_to_the_current_main_binding() {
    let (_directories, store) = crate::subscriptions::store();
    store
        .write(|ctx| {
            settings::set(
                ctx.conn(),
                &GuiSettings {
                    application_display_name: String::new(),
                    confirm_exit: false,
                },
            )
        })
        .unwrap();
    let _windows = headless::init();
    let ui = MainWindow::new().unwrap();
    ui.show().unwrap();
    let old = bind(&ui, Pages::single(SearchPage::new(store.clone())));
    let draft = options(&ui, &old);
    draft.invoke_cancel();
    assert_eq!(
        store
            .read(settings::get::<GuiSettings>)
            .unwrap()
            .application_display_name,
        ""
    );
    let draft = options(&ui, &old);
    draft.invoke_apply();
    assert_eq!(
        ui.get_window_title(),
        format!("hydrus client {}", env!("CARGO_PKG_VERSION"))
    );
    let draft = options(&ui, &old);
    draft.invoke_text_edited(
        row(&draft, "Application display name: "),
        "stale name".into(),
    );
    let (_other_directories, other) = crate::subscriptions::store();
    other
        .write(|ctx| {
            settings::set(
                ctx.conn(),
                &GuiSettings {
                    application_display_name: "successor name".into(),
                    confirm_exit: false,
                },
            )
        })
        .unwrap();
    let _current = bind(&ui, Pages::single(SearchPage::new(other.clone())));
    draft.invoke_apply();
    ui.invoke_refresh_application_title();
    assert_eq!(
        ui.get_window_title(),
        format!("successor name {}", env!("CARGO_PKG_VERSION"))
    );
    ui.window().dispatch_event(WindowEvent::CloseRequested);
    ui.show().unwrap();
    other
        .write(|ctx| {
            settings::set(
                ctx.conn(),
                &GuiSettings {
                    application_display_name: "retired write".into(),
                    confirm_exit: false,
                },
            )
        })
        .unwrap();
    ui.invoke_refresh_application_title();
    assert_eq!(
        ui.get_window_title(),
        format!("successor name {}", env!("CARGO_PKG_VERSION")),
        "re-show does not resurrect the retired title consumer"
    );
}

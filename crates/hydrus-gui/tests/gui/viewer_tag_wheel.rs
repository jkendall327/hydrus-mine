//! Actual wheel scrolling/edge navigation and owned staged policy preferences.
use hydrus_gui::{MainWindow, OptionsWindow, Pages, SearchPage, bind, headless};
use hydrus_store::{
    Store,
    settings::{self, TagWheelPropagation, ViewerTagScrollSettings},
};
use slint::{ComponentHandle as _, LogicalPosition, Model as _, platform::WindowEvent};
const LABEL: &str = "Allow a mouse wheel scroll over the taglist to propagate to the main canvas:";
fn options(ui: &MainWindow, bound: &hydrus_gui::Bound) -> OptionsWindow {
    ui.invoke_menu_title_pressed(0, 20.0, 22.0);
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
        .position(|row| row.text == "media viewer hovers")
        .unwrap();
    options.invoke_page_chosen(i32::try_from(page).unwrap());
    options
}
fn row(options: &OptionsWindow) -> i32 {
    i32::try_from(
        options
            .get_rows()
            .iter()
            .position(|row| row.label == LABEL)
            .unwrap(),
    )
    .unwrap()
}
pub(super) fn choose(ui: &MainWindow, bound: &hydrus_gui::Bound, code: i32) {
    let options = options(ui, bound);
    options.invoke_choice_chosen(row(&options), code);
    options.invoke_apply();
}
pub(super) fn wheel(viewer: &hydrus_gui::MediaViewerWindow) {
    viewer
        .window()
        .dispatch_event(WindowEvent::PointerScrolled {
            position: LogicalPosition::new(50.0, 140.0),
            delta_x: 0.0,
            delta_y: -60.0,
        });
}
#[test]
fn long_tag_hover_scrolls_before_policy_gated_real_navigation_and_zoom() {
    let (dirs, store) = crate::subscriptions::store();
    let windows = headless::init();
    let ui = MainWindow::new().unwrap();
    let bound = bind(&ui, Pages::single(SearchPage::new(store.clone())));
    ui.invoke_search_edited("system:filetype is jpeg".into());
    ui.invoke_search_accepted();
    let files = bound.current.borrow().borrow().results().to_vec();
    assert!(files.len() > 1);
    let first = files[0];
    let service = store.snapshot().services.by_name("my tags").unwrap().id;
    store
        .write_content(move |writer| {
            for i in 0..200 {
                let tag = hydrus_core::Tag::new(&format!("wheel:{i:03}")).unwrap();
                let id = hydrus_store::master::intern_tag(writer.conn(), &tag)?;
                writer.update_mappings(
                    service,
                    &hydrus_store::content::MappingAction::Add,
                    id,
                    &[first],
                )?;
            }
            Ok(())
        })
        .unwrap();
    ui.invoke_thumbnail_activated(0);
    let viewer = bound.viewer.borrow().as_ref().unwrap().clone_strong();
    let drawn = windows.get(windows.count() - 1).unwrap();
    headless::render(&drawn, 1000, 300);
    viewer.window().dispatch_event(WindowEvent::PointerMoved {
        position: LogicalPosition::new(50.0, 140.0),
    });
    headless::render(&drawn, 1000, 300);
    assert!(viewer.get_tags_showing());
    assert!(viewer.get_tags().row_count() >= 200);
    assert!(viewer.get_tag_scroll_maximum() > 0.0);
    choose(&ui, &bound, 0);
    assert_eq!(viewer.get_tag_wheel_policy(), 0);
    let caption = viewer.get_caption();
    wheel(&viewer);
    assert!(
        viewer.get_tag_scroll_offset() > 0.0,
        "actual list consumes wheel before policy"
    );
    assert_eq!(viewer.get_caption(), caption);
    let pixels = headless::render_snapshot(&drawn, 1000, 300);
    headless::save_png(
        &std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join("viewer-tags-wheel.png"),
        &pixels,
        1000,
        300,
    )
    .unwrap();
    viewer.set_tag_scroll_offset(viewer.get_tag_scroll_maximum());
    wheel(&viewer);
    assert_eq!(viewer.get_caption(), caption, "never propagates at an edge");
    choose(&ui, &bound, 1);
    wheel(&viewer);
    assert_eq!(
        viewer.get_caption(),
        caption,
        "bar blocks no-scrollbar-only mode"
    );
    choose(&ui, &bound, 3);
    wheel(&viewer);
    assert_ne!(
        viewer.get_caption(),
        caption,
        "edge reaches actual next-file action"
    );
    assert_eq!(viewer.get_tag_scroll_offset().to_bits(), 0.0_f32.to_bits());
    viewer.invoke_previous();
    assert_eq!(viewer.get_caption(), caption);
    choose(&ui, &bound, 2);
    std::thread::sleep(std::time::Duration::from_millis(650));
    viewer.set_tag_scroll_offset(viewer.get_tag_scroll_maximum() - 1.0);
    wheel(&viewer);
    assert_eq!(viewer.get_caption(), caption);
    wheel(&viewer);
    assert_eq!(
        viewer.get_caption(),
        caption,
        "recent list scrolling suppresses edge cascade"
    );
    std::thread::sleep(std::time::Duration::from_millis(650));
    wheel(&viewer);
    assert_ne!(
        viewer.get_caption(),
        caption,
        "quiet same-direction edge navigates"
    );
    assert!(
        viewer.get_tag_scroll_maximum() <= 0.0,
        "next fixture file has a short tag list"
    );
    choose(&ui, &bound, 0);
    let second = viewer.get_caption();
    wheel(&viewer);
    assert_eq!(viewer.get_caption(), second);
    choose(&ui, &bound, 1);
    wheel(&viewer);
    assert_ne!(
        viewer.get_caption(),
        second,
        "no scrollbar permits propagation"
    );
    choose(&ui, &bound, 3);
    let shown = viewer.get_caption();
    let width = viewer.get_media_width();
    viewer.window().dispatch_event(WindowEvent::KeyPressed {
        text: slint::platform::Key::Control.into(),
    });
    viewer
        .window()
        .dispatch_event(WindowEvent::PointerScrolled {
            position: LogicalPosition::new(50.0, 140.0),
            delta_x: 0.0,
            delta_y: 60.0,
        });
    viewer.window().dispatch_event(WindowEvent::KeyReleased {
        text: slint::platform::Key::Control.into(),
    });
    assert!(
        viewer.get_media_width() > width,
        "propagated control wheel reaches real zoom"
    );
    assert_eq!(viewer.get_caption(), shown);
    let draft = options(&ui, &bound);
    draft.invoke_choice_chosen(row(&draft), 0);
    draft.invoke_cancel();
    assert_eq!(viewer.get_tag_wheel_policy(), 3);
    let disk = Store::open(dirs[1].path()).unwrap();
    assert_eq!(
        disk.read(settings::get::<ViewerTagScrollSettings>)
            .unwrap()
            .0,
        TagWheelPropagation::Immediately
    );
    let reopened = options(&ui, &bound);
    assert_eq!(
        reopened
            .get_rows()
            .row_data(usize::try_from(row(&reopened)).unwrap())
            .unwrap()
            .index,
        3
    );
    reopened.invoke_cancel();
    viewer.invoke_close_requested();
    viewer.show().unwrap();
    let stale = viewer.get_caption();
    viewer.invoke_tag_wheel(-60.0, false, 50.0, 140.0);
    assert_eq!(viewer.get_caption(), stale);
    viewer.hide().unwrap();
}

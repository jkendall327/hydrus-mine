//! Real Options ownership, modifier pointer/key input, current preview pixels and timing.
use hydrus_gui::{MainWindow, OptionsWindow, Pages, bind, headless};
use hydrus_store::{
    settings::{self, FileViewingStatistics, GuiSettings},
    thumbnail_preview_selection::Preferences,
};
use slint::platform::{Key, PointerEventButton, WindowEvent};
use slint::{ComponentHandle as _, Model as _};
use std::{cell::Cell, rc::Rc, sync::Arc};

fn options(ui: &MainWindow, bound: &hydrus_gui::Bound) -> OptionsWindow {
    ui.invoke_menu_title_pressed(0, 20.0, 22.0);
    let lines = ui.get_menu_panes().row_data(0).unwrap().lines;
    let row = lines
        .iter()
        .position(|line| line.label == "options…")
        .unwrap();
    ui.invoke_menu_line_clicked(0, row as i32, 0.0, 0.0, 0.0);
    let window = bound.options.borrow().as_ref().unwrap().clone_strong();
    let page = window
        .get_pages()
        .iter()
        .position(|page| page.text == "thumbnails")
        .unwrap();
    window.invoke_page_chosen(page as i32);
    window
}
fn indices(window: &OptionsWindow) -> [i32; 4] {
    let rows = window.get_rows();
    let ctrl = rows
        .iter()
        .position(|row| row.label.starts_with("On ctrl-selection"))
        .unwrap() as i32;
    let shift = rows
        .iter()
        .position(|row| row.label.starts_with("On shift-selection"))
        .unwrap() as i32;
    [ctrl, ctrl + 1, shift, shift + 1]
}
fn settle(native: &slint::platform::software_renderer::MinimalSoftwareWindow) {
    for _ in 0..8 {
        headless::render(native, 1100, 700);
    }
}
fn with_modifier(
    native: &slint::platform::software_renderer::MinimalSoftwareWindow,
    modifier: Option<Key>,
    action: impl FnOnce(),
) {
    let text: Option<slint::SharedString> = modifier.map(Into::into);
    if let Some(text) = &text {
        native.dispatch_event(WindowEvent::KeyPressed { text: text.clone() });
    }
    action();
    if let Some(text) = text {
        native.dispatch_event(WindowEvent::KeyReleased { text });
    }
}
fn click(
    native: &slint::platform::software_renderer::MinimalSoftwareWindow,
    ui: &MainWindow,
    index: usize,
    modifier: Option<Key>,
) {
    let columns = ui.get_grid_columns() as usize;
    let width = ui.get_thumbnail_width() + 2.0 * ui.get_thumbnail_margin();
    let height = ui.get_thumbnail_height() + 2.0 * ui.get_thumbnail_margin();
    let position = slint::LogicalPosition::new(
        ui.get_grid_origin_x() + (index % columns) as f32 * width + width / 2.0,
        ui.get_grid_origin_y()
            + ui.get_grid_scroll()
            + (index / columns) as f32 * height
            + height / 2.0,
    );
    with_modifier(native, modifier, || {
        native.dispatch_event(WindowEvent::PointerPressed {
            position,
            button: PointerEventButton::Left,
        });
        native.dispatch_event(WindowEvent::PointerReleased {
            position,
            button: PointerEventButton::Left,
        });
    });
    settle(native);
}
fn preview(ui: &MainWindow, bound: &hydrus_gui::Bound, width: u32) {
    let started = std::time::Instant::now();
    loop {
        bound.preview.refresh();
        if ui.get_preview_media().size().width == width && !ui.get_preview_loading() {
            break;
        }
        assert!(
            started.elapsed() < std::time::Duration::from_secs(5),
            "owned preview did not display expected target {width}"
        );
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
}
#[test]
fn staged_controls_reach_pointer_range_key_preview_and_permanent_owner_retirement() {
    let (_directories, store) = crate::subscriptions::store();
    // Private synthetic metadata makes None versus Some(0) unambiguous without a movie decoder.
    let mut initial = super::common::all_local_page(store.clone());
    initial.enter();
    let ids = initial.results()[..3].to_vec();
    store
        .write(|ctx| {
            for (index, id) in ids.iter().enumerate() {
                ctx.conn().execute(
                    "UPDATE files SET mime=2,width=16,height=16,duration_ms=?1 WHERE hash_id=?2",
                    rusqlite::params![if index == 1 { Some(0_i64) } else { None }, id.0],
                )?;
            }
            ctx.conn()
                .execute("DELETE FROM file_viewing_stats WHERE canvas_type=1", [])?;
            settings::set(ctx.conn(), &Preferences::default())?;
            let mut gui: GuiSettings = settings::get(ctx.conn())?;
            gui.confirm_exit = true;
            settings::set(ctx.conn(), &gui)?;
            let mut stats: FileViewingStatistics = settings::get(ctx.conn())?;
            stats.active = true;
            stats.preview_min_ms = None;
            stats.preview_max_ms = None;
            settings::set(ctx.conn(), &stats)
        })
        .unwrap();
    drop(initial);
    let windows = headless::init();
    let ui = MainWindow::new().unwrap();
    let bound = bind(
        &ui,
        Pages::single(super::common::all_local_page(store.clone())),
    );
    ui.invoke_search_edited("system:everything".into());
    ui.invoke_search_accepted();
    assert_eq!(
        &bound.current.borrow().borrow().results()[..3],
        ids.as_slice()
    );
    let clock = Rc::new(Cell::new(100_i64));
    bound.preview.set_clock(Rc::new({
        let clock = clock.clone();
        move || clock.get()
    }));
    bound.preview.set_decoder(Arc::new({
        let ids = ids.clone();
        move |_, id| {
            let width = ids.iter().position(|file| *file == id)? as u32 + 11;
            hydrus_media::Raster::new(width, 1, 3, vec![100; width as usize * 3]).ok()
        }
    }));
    ui.show().unwrap();
    let native = windows.get(0).unwrap();
    native.dispatch_event(WindowEvent::WindowActiveChanged(true));
    settle(&native);
    let cancelled = options(&ui, &bound);
    let rows = indices(&cancelled);
    assert!(
        !cancelled
            .get_rows()
            .row_data(rows[1] as usize)
            .unwrap()
            .enabled
    );
    assert!(
        !cancelled
            .get_rows()
            .row_data(rows[3] as usize)
            .unwrap()
            .enabled
    );
    cancelled.invoke_check_toggled(rows[0], true);
    cancelled.invoke_cancel();
    cancelled.invoke_apply();
    assert_eq!(
        store.read(settings::get::<Preferences>).unwrap(),
        Preferences::default()
    );
    let applied = options(&ui, &bound);
    for row in indices(&applied) {
        applied.invoke_check_toggled(row, true);
    }
    let last = (0..100)
        .take_while(|&n| windows.get(n).is_some())
        .last()
        .unwrap();
    let pixels = headless::render(&windows.get(last).unwrap(), 1100, 900);
    headless::save_png(
        &std::path::Path::new(env!("CARGO_TARGET_TMPDIR"))
            .join("thumbnail_preview_selection_options.png"),
        &pixels,
        1100,
        900,
    );
    applied.invoke_apply();
    let saved: Preferences = store.read(settings::get).unwrap();
    assert_eq!(
        saved,
        Preferences {
            ctrl_focus: true,
            ctrl_only_static: true,
            shift_focus: true,
            shift_only_static: true
        }
    );
    let reopened = options(&ui, &bound);
    for row in indices(&reopened) {
        assert!(reopened.get_rows().row_data(row as usize).unwrap().checked);
    }
    reopened.invoke_cancel();
    settle(&native);
    click(&native, &ui, 0, None);
    preview(&ui, &bound, 11);
    clock.set(200);
    click(&native, &ui, 1, Some(Key::Control));
    preview(&ui, &bound, 11);
    assert_eq!(
        bound.current.borrow().borrow().focused(),
        Some(0),
        "Some(0) retains the existing preview"
    );
    assert_eq!(
        bound
            .current
            .borrow()
            .borrow()
            .selected_indices()
            .into_iter()
            .collect::<Vec<_>>(),
        vec![0, 1]
    );
    clock.set(300);
    click(&native, &ui, 2, Some(Key::Control));
    preview(&ui, &bound, 13);
    clock.set(400);
    click(&native, &ui, 2, Some(Key::Control));
    bound.preview.refresh();
    assert_eq!(bound.current.borrow().borrow().focused(), None);
    assert!(!ui.get_preview_has_media());
    clock.set(500);
    click(&native, &ui, 1, Some(Key::Shift));
    preview(&ui, &bound, 12);
    assert_eq!(
        bound.current.borrow().borrow().focused(),
        Some(1),
        "anchorless Shift fallback always focuses"
    );
    clock.set(600);
    click(&native, &ui, 2, Some(Key::Shift));
    preview(&ui, &bound, 13);
    clock.set(700);
    click(&native, &ui, 1, Some(Key::Shift));
    preview(&ui, &bound, 13);
    assert_eq!(
        bound.current.borrow().borrow().focused(),
        Some(2),
        "duration rejection keeps last preview while range contracts"
    );
    clock.set(800);
    with_modifier(&native, Some(Key::Shift), || {
        native.dispatch_event(WindowEvent::KeyPressed {
            text: Key::RightArrow.into(),
        });
        native.dispatch_event(WindowEvent::KeyReleased {
            text: Key::RightArrow.into(),
        });
    });
    preview(&ui, &bound, 13);
    assert_eq!(bound.current.borrow().borrow().focused(), Some(2));
    // Saved changes are consulted by the next event, without reopening the page.
    store
        .write(|ctx| {
            let mut policy: Preferences = settings::get(ctx.conn())?;
            policy.shift_only_static = false;
            settings::set(ctx.conn(), &policy)
        })
        .unwrap();
    clock.set(900);
    click(&native, &ui, 1, Some(Key::Shift));
    preview(&ui, &bound, 12);
    let views = |id| {
        store
            .read(|conn| hydrus_store::media::viewing_stats(conn, &[id]))
            .unwrap()
            .into_iter()
            .find(|stat| stat.canvas == hydrus_core::CanvasType::Preview)
            .unwrap()
            .views
    };
    assert_eq!(views(ids[0]), 1);
    assert_eq!(views(ids[1]), 1);
    assert_eq!(views(ids[2]), 2);
    ui.window().dispatch_event(WindowEvent::CloseRequested);
    ui.invoke_answer(false);
    assert!(ui.window().is_visible());
    click(&native, &ui, 0, None);
    preview(&ui, &bound, 11);
    ui.window().dispatch_event(WindowEvent::CloseRequested);
    ui.invoke_answer(true);
    assert!(!ui.window().is_visible());
    let retired = bound.current.borrow().borrow().selected_items();
    ui.show().unwrap();
    ui.invoke_thumbnail_clicked(2, false, false);
    ui.invoke_move_focus(1, true, 5, 4);
    bound.preview.refresh();
    assert_eq!(bound.current.borrow().borrow().selected_items(), retired);
    assert!(!ui.get_preview_has_media());
    let successor = bind(&ui, Pages::single(super::common::all_local_page(store)));
    ui.invoke_search_edited("system:everything".into());
    ui.invoke_search_accepted();
    ui.invoke_thumbnail_clicked(2, false, false);
    assert_eq!(successor.current.borrow().borrow().focused(), Some(2));
    assert_eq!(
        bound.current.borrow().borrow().selected_items(),
        retired,
        "successor does not mutate former owner"
    );
}

#[test]
fn live_store_collection_gate_uses_all_members_and_preserves_zero_duration_distinction() {
    let (_directories, store) = crate::subscriptions::store();
    let mut original = super::common::all_local_page(store.clone());
    original.enter();
    let files = original.results().to_vec();
    assert!(files.len() > 3);
    store
        .write(|ctx| {
            ctx.conn()
                .execute("UPDATE files SET duration_ms=NULL", [])?;
            ctx.conn().execute(
                "UPDATE files SET duration_ms=1 WHERE hash_id=?1",
                [files.last().unwrap().0],
            )?;
            settings::set(
                ctx.conn(),
                &Preferences {
                    ctrl_focus: true,
                    ctrl_only_static: true,
                    ..Preferences::default()
                },
            )
        })
        .unwrap();
    let collected = || {
        let mut page = super::common::all_local_page(store.clone());
        page.enter();
        page.set_collect(hydrus_core::pages::PageCollect {
            namespaces: vec!["synthetic-no-namespace".into()],
            ratings: Vec::new(),
            collect_unmatched: true,
            tag_context: Default::default(),
        });
        assert_eq!(page.results().len(), 1);
        assert_eq!(
            page.collection(page.results()[0]).unwrap().len(),
            files.len()
        );
        page
    };
    let mut page = collected();
    assert_eq!(page.results()[0], files[0], "static preview representative");
    page.hit(Some(0), true, false);
    assert_eq!(
        page.focused(),
        None,
        "positive duration elsewhere in collection blocks focus"
    );
    assert_eq!(page.selected_counts().collections, 1);
    store
        .write(|ctx| {
            settings::set(
                ctx.conn(),
                &Preferences {
                    ctrl_focus: true,
                    ..Preferences::default()
                },
            )
        })
        .unwrap();
    page.select_none();
    page.hit(Some(0), true, false);
    assert_eq!(
        page.focused(),
        Some(0),
        "next live event sees updated policy"
    );
    store
        .write(|ctx| {
            ctx.conn().execute("UPDATE files SET duration_ms=0", [])?;
            settings::set(
                ctx.conn(),
                &Preferences {
                    ctrl_focus: true,
                    ctrl_only_static: true,
                    ..Preferences::default()
                },
            )
        })
        .unwrap();
    let mut zero = collected();
    zero.hit(Some(0), true, false);
    assert_eq!(zero.focused(), Some(0), "Qt zero aggregate is None");
    let mut single = super::common::all_local_page(store.clone());
    single.enter();
    single.hit(Some(0), true, false);
    assert_eq!(
        single.focused(),
        None,
        "singleton Some(0) remains a duration"
    );
}

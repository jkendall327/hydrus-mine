//! The rating service editor's star appearance: the choice of shapes or SVGs,
//! the SVGs offered, and an SVG star drawn in the live preview, against
//! `oracle/record_star_svgs.py` (the reference's
//! `EditServiceStarRatingsSubPanel` and `DrawShape`).

use std::path::PathBuf;

use hydrus_gui::{EditServiceWindow, MainWindow, Pages, bind, headless};
use hydrus_store::services::{ServiceKind, StarAppearance};
use slint::{ComponentHandle as _, Model as _};

fn recorded() -> serde_json::Value {
    hydrus_testkit::fixture_json("star_svgs.json")
}

fn names_of(value: &serde_json::Value) -> Vec<String> {
    value["svg_choices"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c[0].as_str().unwrap().to_owned())
        .collect()
}

fn open_services(ui: &MainWindow) {
    let titles = ui.get_menu_titles();
    let index = titles.iter().position(|t| t.label == "services").unwrap();
    ui.invoke_menu_title_pressed(i32::try_from(index).unwrap(), 10.0, 22.0);
    let lines = ui.get_menu_panes().row_data(0).unwrap().lines;
    let index = lines
        .iter()
        .position(|l| l.label.starts_with("edit"))
        .unwrap();
    ui.invoke_menu_line_clicked(0, i32::try_from(index).unwrap(), 0.0, 0.0, 0.0);
}

/// The edit window of the rating service called `name`.
fn edit_service(
    ui: &MainWindow,
    bound: &hydrus_gui::Bound,
    name: &str,
) -> (hydrus_gui::ServicesEditorWindow, EditServiceWindow) {
    open_services(ui);
    let manage = bound
        .services_editor
        .manage
        .borrow()
        .as_ref()
        .unwrap()
        .clone_strong();
    let index = manage
        .get_rows()
        .iter()
        .position(|r| r.cells.row_data(0).unwrap() == name)
        .unwrap();
    manage.invoke_row_clicked(i32::try_from(index).unwrap(), false, false);
    manage.invoke_edit_clicked();
    let edit = bound
        .services_editor
        .edit
        .borrow()
        .as_ref()
        .unwrap()
        .clone_strong();
    (manage, edit)
}

fn appearance_of(store: &hydrus_store::Store, name: &str) -> StarAppearance {
    match &store.snapshot().services.by_name(name).unwrap().kind {
        ServiceKind::RatingNumerical(c) => c.appearance.clone(),
        ServiceKind::RatingLike(c) => c.appearance.clone(),
        _ => panic!("not a star rating"),
    }
}

fn shown_names(edit: &EditServiceWindow) -> Vec<String> {
    edit.get_svg_names().iter().map(|n| n.to_string()).collect()
}

/// The same names in the same order, but for the order of names that differ
/// only in case (the reference's order between them is its directory's).
fn same_names(ours: &[String], theirs: &[String]) -> bool {
    let fold = |names: &[String]| names.iter().map(|n| n.to_lowercase()).collect::<Vec<_>>();
    fold(ours) == fold(theirs) && {
        let mut a = ours.to_vec();
        let mut b = theirs.to_vec();
        a.sort();
        b.sort();
        a == b
    }
}

// leaf: audit-media-service-star-appearance
#[test]
fn star_editor_offers_the_installed_and_users_svgs_and_keeps_the_choice_like_the_reference() {
    let _windows = headless::init();
    let recorded = recorded();
    let (dirs, store) = crate::subscriptions::store();
    let ui = MainWindow::new().unwrap();
    let bound = bind(&ui, Pages::open(store.clone()).unwrap());

    // the installed SVGs, shapes chosen
    let (manage, edit) = edit_service(&ui, &bound, "stars");
    assert_eq!(shown_names(&edit), names_of(&recorded["installed"]));
    assert!(!edit.get_use_svg());
    assert!(matches!(
        appearance_of(&store, "stars"),
        StarAppearance::Shape(_)
    ));
    // choosing "svgs" and "smile" is what the reference then holds
    edit.set_use_svg(true);
    let smile = recorded["installed_smile_chosen"]["value"]["rating_svg"]
        .as_str()
        .unwrap()
        .to_owned();
    let index = shown_names(&edit).iter().position(|n| *n == smile).unwrap();
    edit.set_svg_index(i32::try_from(index).unwrap());
    edit.invoke_preview_edited();
    edit.invoke_apply_clicked();
    manage.invoke_apply_clicked();
    assert_eq!(
        appearance_of(&store, "stars"),
        StarAppearance::Svg(smile.clone())
    );

    // reopened on the service: svgs are chosen, with its SVG shown
    let (manage, edit) = edit_service(&ui, &bound, "stars");
    assert!(edit.get_use_svg());
    assert_eq!(
        usize::try_from(edit.get_svg_index()).unwrap(),
        shown_names(&edit).iter().position(|n| *n == smile).unwrap()
    );
    // going back to shapes saves a shape again
    edit.set_use_svg(false);
    edit.invoke_apply_clicked();
    manage.invoke_apply_clicked();
    assert!(matches!(
        appearance_of(&store, "stars"),
        StarAppearance::Shape(_)
    ));

    // an SVG that is not there leaves the first one chosen
    let missing = recorded["opened_on_missing_svg"]["value"]["rating_svg"]
        .as_str()
        .unwrap()
        .to_owned();
    let service = store.snapshot().services.by_name("stars").unwrap().clone();
    let mut kind = service.kind.clone();
    if let ServiceKind::RatingNumerical(c) = &mut kind {
        c.appearance = StarAppearance::Svg("gone".into());
    }
    store
        .write_and_refresh(move |ctx| {
            hydrus_store::services::update_config(ctx.conn(), service.id, &kind)
        })
        .unwrap();
    let (manage, edit) = edit_service(&ui, &bound, "stars");
    assert!(edit.get_use_svg());
    assert_eq!(
        shown_names(&edit)[usize::try_from(edit.get_svg_index()).unwrap()],
        missing
    );
    manage.invoke_cancel_clicked();

    // the user's own SVGs join the list; a non-SVG file does not
    let user = hydrus_store::star_shapes::user_dir(store.dir());
    std::fs::create_dir_all(&user).unwrap();
    for file in ["my star.svg", "Smile.SVG", "smile.svg"] {
        std::fs::write(
            user.join(file),
            "<svg xmlns=\"http://www.w3.org/2000/svg\"/>",
        )
        .unwrap();
    }
    std::fs::write(user.join("readme.md"), "not an svg").unwrap();
    let (manage, edit) = edit_service(&ui, &bound, "stars");
    assert!(
        same_names(&shown_names(&edit), &names_of(&recorded["with_user_svgs"])),
        "{:?} vs {:?}",
        shown_names(&edit),
        names_of(&recorded["with_user_svgs"])
    );
    manage.invoke_cancel_clicked();
    drop(dirs);
}

fn fixture(name: &str) -> PathBuf {
    hydrus_testkit::fixture_path(name)
}

/// The painted pixels of a recorded PNG (anything not transparent).
fn qt_mask(name: &str) -> (u32, Vec<bool>) {
    let decoder = png::Decoder::new(std::io::BufReader::new(
        std::fs::File::open(fixture(name)).unwrap(),
    ));
    let mut reader = decoder.read_info().unwrap();
    let mut pixels = vec![0; reader.output_buffer_size().unwrap()];
    let info = reader.next_frame(&mut pixels).unwrap();
    assert_eq!(info.color_type, png::ColorType::Rgba);
    (
        info.width,
        pixels[..info.buffer_size()]
            .chunks_exact(4)
            .map(|p| p[3] >= 64)
            .collect(),
    )
}

/// How much of what either picture paints both paint.
fn overlap(a: &[bool], b: &[bool]) -> f64 {
    let both = a.iter().zip(b).filter(|(x, y)| **x && **y).count();
    let either = a.iter().zip(b).filter(|(x, y)| **x || **y).count();
    both as f64 / either.max(1) as f64
}

/// Scroll to the example display and draw the first star of its first row:
/// the pixels, as RGBA, with the background they are on.
fn first_star(
    native: &std::rc::Rc<slint::platform::software_renderer::MinimalSoftwareWindow>,
    edit: &EditServiceWindow,
    size: usize,
) -> (Vec<[u8; 4]>, [u8; 4]) {
    headless::render(native, 640, 1000);
    // the preview is below the fold: one wheel dispatch reaches it
    native.dispatch_event(slint::platform::WindowEvent::PointerScrolled {
        position: slint::LogicalPosition::new(24.0, 500.0),
        delta_x: 0.0,
        delta_y: -10000.0,
    });
    let mut pixels = headless::render(native, 640, 1000);
    for _ in 0..10 {
        std::thread::sleep(std::time::Duration::from_millis(30));
        slint::platform::update_timers_and_animations();
        pixels = headless::render(native, 640, 1000);
    }
    let stride = 640 * 4;
    let (x0, y0) = (
        edit.get_first_preview_x() as usize + 2,
        edit.get_first_preview_y() as usize + 2,
    );
    let at =
        |x: usize, y: usize| -> [u8; 4] { pixels[y * stride + x * 4..][..4].try_into().unwrap() };
    let star = (0..size)
        .flat_map(|y| (0..size).map(move |x| (x, y)))
        .map(|(x, y)| at(x0 + x, y0 + y))
        .collect();
    (star, at(x0 - 1, y0 - 1))
}

fn painted(star: &[[u8; 4]], background: [u8; 4]) -> Vec<bool> {
    star.iter()
        .map(|p| p.iter().zip(background).any(|(a, b)| a.abs_diff(b) > 40))
        .collect()
}

fn near(pixel: [u8; 4], colour: &str) -> bool {
    let rgb: Vec<u8> = (0..3)
        .map(|i| u8::from_str_radix(&colour[1 + i * 2..3 + i * 2], 16).unwrap())
        .collect();
    pixel[3] == 255 && rgb.iter().zip(pixel).all(|(a, b)| a.abs_diff(b) < 12)
}

fn set_icon_size(store: &hydrus_store::Store, size: u32) {
    store
        .write(move |ctx| {
            let mut settings: hydrus_core::thumbnail::ThumbnailRatingSettings =
                hydrus_store::settings::get(ctx.conn())?;
            settings.icon_size = f64::from(size);
            hydrus_store::settings::set(ctx.conn(), &settings)
        })
        .unwrap();
}

fn last_window(
    windows: &headless::Windows,
) -> std::rc::Rc<slint::platform::software_renderer::MinimalSoftwareWindow> {
    let last = (0..100)
        .take_while(|&n| windows.get(n).is_some())
        .last()
        .unwrap();
    windows.get(last).unwrap()
}

// leaf: audit-media-service-star-appearance
#[test]
fn svg_stars_are_drawn_in_the_live_preview_where_the_reference_paints_them() {
    let windows = headless::init();
    let recorded = recorded();
    let (_dirs, store) = crate::subscriptions::store();
    let ui = MainWindow::new().unwrap();
    let bound = bind(&ui, Pages::open(store.clone()).unwrap());
    let user = hydrus_store::star_shapes::user_dir(store.dir());
    std::fs::create_dir_all(&user).unwrap();
    std::fs::write(
        user.join("my star.svg"),
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"64\" height=\"64\" viewBox=\"0 0 64 64\"><path d=\"M8 8h48v48h-48z M20 20v24h24v-24z\" fill=\"#000000\" fill-rule=\"evenodd\"/></svg>",
    )
    .unwrap();
    let border = recorded["border"].as_str().unwrap().to_owned();
    let fill = recorded["fill"].as_str().unwrap().to_owned();
    for size in [16_u32, 24, 32] {
        set_icon_size(&store, size);
        for name in ["smile", "star", "spiral", "my star"] {
            let (manage, edit) = edit_service(&ui, &bound, "stars");
            edit.set_stars(1);
            edit.set_icon_padding(0);
            edit.invoke_colour_edited(2, true, border.clone().into());
            edit.invoke_colour_edited(2, false, fill.clone().into());
            edit.set_use_svg(true);
            let index = shown_names(&edit).iter().position(|n| n == name).unwrap();
            edit.set_svg_index(i32::try_from(index).unwrap());
            edit.invoke_preview_edited();
            let (star, background) = first_star(&last_window(&windows), &edit, size as usize);
            let file = format!("star_svg_{}_{size}.png", name.replace(' ', "_"));
            let (width, theirs) = qt_mask(&file);
            assert_eq!(width, size);
            let similar = overlap(&painted(&star, background), &theirs);
            assert!(
                similar > 0.65,
                "{name} at {size}: paints {similar:.2} of what Qt paints"
            );
            // the fill is the star's colour, the border its edge
            assert!(
                star.iter().any(|p| near(*p, &fill)),
                "{name} at {size}: fill"
            );
            assert!(
                star.iter().any(|p| near(*p, &border)),
                "{name} at {size}: border"
            );
            edit.invoke_cancel_clicked();
            manage.invoke_cancel_clicked();
        }
    }
}

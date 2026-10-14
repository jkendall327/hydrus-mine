//! The rating service editors star appearance: the choice of shapes or SVGs,
//! the SVGs offered, and an SVG star drawn wherever ratings are drawn (the
//! editor's examples, the thumbnails, the media viewer, the preview window),
//! against `oracle/record_star_svgs.py` (the reference's
//! `EditServiceStarRatingsSubPanel` and `DrawShape`).

use std::path::PathBuf;
use std::rc::Rc;

use hydrus_gui::{EditServiceWindow, MainWindow, Pages, bind, headless};
use hydrus_store::services::{PenBrush, Rgb, ServiceKind, StarAppearance, StarShape};
use slint::platform::WindowAdapter as _;
use slint::platform::software_renderer::MinimalSoftwareWindow;
use slint::platform::{PointerEventButton, WindowEvent};
use slint::{ComponentHandle as _, Model as _};

use crate::options_gui_support::{Client, basic_store};

type Native = Rc<MinimalSoftwareWindow>;

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

/// Give the rating service `name` this appearance and, if asked, these
/// colours (all four roles) shown in thumbnails.
fn configure(
    store: &hydrus_store::Store,
    name: &str,
    appearance: StarAppearance,
    colours: Option<PenBrush>,
) {
    let service = store.snapshot().services.by_name(name).unwrap().clone();
    let mut kind = service.kind.clone();
    let (config_appearance, display) = match &mut kind {
        ServiceKind::RatingNumerical(c) => (&mut c.appearance, &mut c.display),
        ServiceKind::RatingLike(c) => (&mut c.appearance, &mut c.display),
        _ => panic!("not a star rating"),
    };
    *config_appearance = appearance;
    if let Some(colours) = colours {
        display.colours.like = colours;
        display.colours.dislike = colours;
        display.colours.null = colours;
        display.colours.mixed = colours;
        display.show_in_thumbnail = true;
        display.show_in_thumbnail_even_when_null = true;
    }
    store
        .write_and_refresh(move |ctx| {
            hydrus_store::services::update_config(ctx.conn(), service.id, &kind)
        })
        .unwrap();
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

/// The headless window drawing `window`.
fn native_of(windows: &headless::Windows, window: &slint::Window) -> Native {
    (0..windows.count())
        .filter_map(|n| windows.get(n))
        .find(|native| std::ptr::eq(native.window(), window))
        .expect("the window is one of those made")
}

/// A left click, with the pointer coming from elsewhere as a user's does.
fn click(native: &Native, x: f32, y: f32) {
    let position = slint::LogicalPosition::new(x, y);
    native.dispatch_event(WindowEvent::PointerMoved { position });
    native.dispatch_event(WindowEvent::PointerPressed {
        position,
        button: PointerEventButton::Left,
    });
    native.dispatch_event(WindowEvent::PointerReleased {
        position,
        button: PointerEventButton::Left,
    });
}

/// Draw `native` until `ready` says what is wanted is on screen.
fn render_until(
    native: &Native,
    size: (u32, u32),
    mut ready: impl FnMut(&[u8]) -> bool,
) -> Vec<u8> {
    let started = std::time::Instant::now();
    loop {
        slint::platform::update_timers_and_animations();
        let pixels = headless::render(native, size.0, size.1);
        if ready(&pixels) {
            return pixels;
        }
        assert!(
            started.elapsed() < std::time::Duration::from_secs(5),
            "what was to be drawn never was"
        );
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
}

fn states(edit: &EditServiceWindow, recorded: &serde_json::Value, label: &str) {
    assert_eq!(
        edit.get_shape_enabled(),
        recorded["shape_enabled"],
        "{label}: shapes list"
    );
    assert_eq!(
        edit.get_svg_enabled(),
        recorded["svg_enabled"],
        "{label}: svgs list"
    );
}

// leaf: audit-media-service-star-appearance
#[test]
fn star_editor_offers_the_installed_and_users_svgs_and_keeps_the_choice_like_the_reference() {
    let windows = headless::init();
    let recorded = recorded();
    let (dirs, store) = crate::subscriptions::store();
    configure(&store, "stars", StarAppearance::Shape(StarShape(3)), None);
    let ui = MainWindow::new().unwrap();
    let bound = bind(&ui, Pages::open(store.clone()).unwrap());

    // shapes chosen: the shapes list in use, the installed SVGs offered
    let (manage, edit) = edit_service(&ui, &bound, "stars");
    let native = native_of(&windows, edit.window());
    for _ in 0..3 {
        headless::render(&native, 640, 1000);
    }
    assert_eq!(shown_names(&edit), names_of(&recorded["installed"]));
    assert!(!edit.get_use_svg());
    states(&edit, &recorded["installed"], "installed");
    // clicking "svgs": the svg list in use, the first SVG chosen
    let svgs = (edit.get_svgs_radio_x() + 4.0, edit.get_svgs_radio_y() + 4.0);
    click(&native, svgs.0, svgs.1);
    headless::render(&native, 640, 1000);
    assert!(edit.get_use_svg());
    states(
        &edit,
        &recorded["installed_svg_selected"],
        "installed_svg_selected",
    );
    let first = recorded["installed_svg_selected"]["value"]["rating_svg"]
        .as_str()
        .unwrap();
    assert_eq!(shown_names(&edit)[edit.get_svg_index() as usize], first);
    // "shapes" again: back to the shape it held
    let shapes = (
        edit.get_shapes_radio_x() + 4.0,
        edit.get_shapes_radio_y() + 4.0,
    );
    click(&native, shapes.0, shapes.1);
    headless::render(&native, 640, 1000);
    assert!(!edit.get_use_svg());
    states(
        &edit,
        &recorded["installed_shape_again"],
        "installed_shape_again",
    );
    edit.invoke_apply_clicked();
    manage.invoke_apply_clicked();
    assert_eq!(
        appearance_of(&store, "stars"),
        StarAppearance::Shape(StarShape(
            recorded["installed_shape_again"]["value"]["shape"]
                .as_u64()
                .unwrap() as u8
        ))
    );

    // "svgs" and "smile": what the reference then holds
    let (manage, edit) = edit_service(&ui, &bound, "stars");
    let native = native_of(&windows, edit.window());
    for _ in 0..3 {
        headless::render(&native, 640, 1000);
    }
    click(&native, svgs.0, svgs.1);
    let smile = recorded["installed_smile_chosen"]["value"]["rating_svg"]
        .as_str()
        .unwrap()
        .to_owned();
    let index = shown_names(&edit).iter().position(|n| *n == smile).unwrap();
    edit.set_svg_index(i32::try_from(index).unwrap());
    states(
        &edit,
        &recorded["installed_smile_chosen"],
        "installed_smile_chosen",
    );
    edit.invoke_apply_clicked();
    manage.invoke_apply_clicked();
    assert_eq!(
        appearance_of(&store, "stars"),
        StarAppearance::Svg(smile.clone())
    );

    // opened on a service that draws an SVG: svgs chosen, its SVG shown
    let spiral = recorded["opened_on_svg"]["value"]["rating_svg"]
        .as_str()
        .unwrap()
        .to_owned();
    configure(&store, "stars", StarAppearance::Svg(spiral.clone()), None);
    let (manage, edit) = edit_service(&ui, &bound, "stars");
    assert!(edit.get_use_svg());
    assert_eq!(shown_names(&edit)[edit.get_svg_index() as usize], spiral);
    states(&edit, &recorded["opened_on_svg"], "opened_on_svg");
    edit.invoke_apply_clicked();
    manage.invoke_apply_clicked();
    assert_eq!(appearance_of(&store, "stars"), StarAppearance::Svg(spiral));

    // an SVG that is not there leaves the first one chosen
    let missing = recorded["opened_on_missing_svg"]["value"]["rating_svg"]
        .as_str()
        .unwrap()
        .to_owned();
    configure(&store, "stars", StarAppearance::Svg("gone".into()), None);
    let (manage, edit) = edit_service(&ui, &bound, "stars");
    assert!(edit.get_use_svg());
    assert_eq!(
        shown_names(&edit)[usize::try_from(edit.get_svg_index()).unwrap()],
        missing
    );
    states(
        &edit,
        &recorded["opened_on_missing_svg"],
        "opened_on_missing_svg",
    );
    manage.invoke_cancel_clicked();

    // the user's own SVGs join the list; a non-SVG file does not, and a
    // file called just ".svg" is named ".svg", as Python names it
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
    std::fs::write(
        user.join(".svg"),
        "<svg xmlns=\"http://www.w3.org/2000/svg\"/>",
    )
    .unwrap();
    let (manage, edit) = edit_service(&ui, &bound, "stars");
    assert!(shown_names(&edit).contains(&".svg".to_owned()));
    manage.invoke_cancel_clicked();
    drop(dirs);
}

fn fixture(name: &str) -> PathBuf {
    hydrus_testkit::fixture_path(name)
}

/// What a picture paints, cropped to the box that holds it.
struct Mask {
    width: usize,
    height: usize,
    painted: Vec<bool>,
}

impl Mask {
    /// The painted points of `grid` (`width` wide), cropped.
    fn of(width: usize, grid: &[bool]) -> Option<Self> {
        let points: Vec<(usize, usize)> = grid
            .iter()
            .enumerate()
            .filter(|(_, on)| **on)
            .map(|(i, _)| (i % width, i / width))
            .collect();
        let left = points.iter().map(|p| p.0).min()?;
        let right = points.iter().map(|p| p.0).max()?;
        let top = points.iter().map(|p| p.1).min()?;
        let bottom = points.iter().map(|p| p.1).max()?;
        let (w, h) = (right - left + 1, bottom - top + 1);
        let mut painted = vec![false; w * h];
        for (x, y) in points {
            painted[(y - top) * w + (x - left)] = true;
        }
        Some(Self {
            width: w,
            height: h,
            painted,
        })
    }

    /// This with each painted point's neighbours painted too: the thin
    /// outline of the default star, whose anti-aliased points are in neither
    /// colour, closed up.
    fn grown(&self) -> Self {
        let (w, h) = (self.width + 2, self.height + 2);
        let mut painted = vec![false; w * h];
        for y in 0..self.height {
            for x in 0..self.width {
                if self.painted[y * self.width + x] {
                    for (dy, dx) in (0..3).flat_map(|dy| (0..3).map(move |dx| (dy, dx))) {
                        painted[(y + dy) * w + x + dx] = true;
                    }
                }
            }
        }
        Self {
            width: w,
            height: h,
            painted,
        }
    }

    /// How much of what either paints both paint, the two boxes' top left
    /// corners together.
    fn overlap(&self, other: &Self) -> f64 {
        let (w, h) = (self.width.max(other.width), self.height.max(other.height));
        let at = |m: &Self, x: usize, y: usize| {
            x < m.width && y < m.height && m.painted[y * m.width + x]
        };
        let (mut both, mut either) = (0, 0);
        for y in 0..h {
            for x in 0..w {
                let (a, b) = (at(self, x, y), at(other, x, y));
                both += usize::from(a && b);
                either += usize::from(a || b);
            }
        }
        both as f64 / either.max(1) as f64
    }
}

/// The painted pixels of a recorded PNG: those mostly covered.
fn qt_mask(name: &str) -> Mask {
    let decoder = png::Decoder::new(std::io::BufReader::new(
        std::fs::File::open(fixture(name)).unwrap(),
    ));
    let mut reader = decoder.read_info().unwrap();
    let mut pixels = vec![0; reader.output_buffer_size().unwrap()];
    let info = reader.next_frame(&mut pixels).unwrap();
    assert_eq!(info.color_type, png::ColorType::Rgba);
    let grid: Vec<bool> = pixels[..info.buffer_size()]
        .chunks_exact(4)
        .map(|p| p[3] >= 128)
        .collect();
    Mask::of(info.width as usize, &grid).unwrap()
}

fn hex(rgb: [u8; 3]) -> String {
    format!("#{:02x}{:02x}{:02x}", rgb[0], rgb[1], rgb[2])
}

fn near(pixel: &[u8], colour: [u8; 3]) -> bool {
    pixel[3] == 255
        && pixel[..3]
            .iter()
            .zip(colour)
            .all(|(a, b)| a.abs_diff(b) < 12)
}

/// The first star in a picture of a window, found by its colours: the box
/// from the top left of what is in the border or fill colour, `size` across.
fn star_in(pixels: &[u8], width: usize, size: usize, colours: [[u8; 3]; 2]) -> Option<Mask> {
    let coloured = |i: usize| {
        let p = &pixels[i * 4..i * 4 + 4];
        colours.iter().any(|c| near(p, *c))
    };
    let count = pixels.len() / 4;
    let top = (0..count).find(|i| coloured(*i))? / width;
    let left = (0..count)
        .filter(|i| i / width < top + size && coloured(*i))
        .map(|i| i % width)
        .min()?;
    let grid: Vec<bool> = (0..size)
        .flat_map(|y| (0..size).map(move |x| (x, y)))
        .map(|(x, y)| {
            let (x, y) = (left + x, top + y);
            x < width && coloured(y * width + x)
        })
        .collect();
    Mask::of(size, &grid)
}

fn set_setting<T: hydrus_store::settings::Setting + Send + 'static>(
    store: &hydrus_store::Store,
    change: impl FnOnce(&mut T) + Send + 'static,
) {
    store
        .write(move |ctx| {
            let mut settings: T = hydrus_store::settings::get(ctx.conn())?;
            change(&mut settings);
            hydrus_store::settings::set(ctx.conn(), &settings)
        })
        .unwrap();
}

/// Whether the star `file` records is what the picture draws, found by its
/// colours: how much of what Qt paints ours paints, either way round, over
/// 70% (Qt's rasteriser and its feathered outline are not resvg's).
fn check_star(
    pixels: &[u8],
    width: usize,
    size: usize,
    colours: [[u8; 3]; 2],
    file: &str,
    what: &str,
) {
    let ours = star_in(pixels, width, size, colours)
        .unwrap_or_else(|| panic!("{what}: no star in the border and fill colours was drawn"));
    let theirs = qt_mask(file);
    // (the default star is a thin outline: closed up on both sides before
    // they are compared)
    let hollow = file.contains("gone");
    let similar = if hollow {
        ours.grown().overlap(&theirs.grown())
    } else {
        ours.overlap(&theirs)
    };
    // (over 70%; a star 12 pixels across has too few pixels for its edge to
    // be Qt's, so 65% there; the closed-up outlines over 75%, their edges being anti-aliased)
    assert!(
        similar
            >= if hollow {
                0.75
            } else if size <= 12 {
                0.65
            } else {
                0.7
            },
        "{what}: paints {similar:.2} of what Qt paints ({file})"
    );
    assert!(
        (ours.width as i64 - theirs.width as i64).abs() <= 2
            && (ours.height as i64 - theirs.height as i64).abs() <= 2,
        "{what}: {}x{} against Qt's {}x{}",
        ours.width,
        ours.height,
        theirs.width,
        theirs.height
    );
}

const BORDER: [u8; 3] = [0x11, 0x22, 0x33];
const FILL: [u8; 3] = [0xdd, 0x88, 0x00];

fn unusual() -> PenBrush {
    PenBrush {
        pen: Rgb(BORDER),
        brush: Rgb(FILL),
    }
}

// leaf: audit-media-service-star-appearance
#[test]
fn svg_stars_are_drawn_in_the_editors_examples_where_the_reference_paints_them() {
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
    assert_eq!((hex(BORDER), hex(FILL)), (border.clone(), fill.clone()));
    for size in recorded["sizes"].as_array().unwrap() {
        let size = size.as_u64().unwrap() as u32;
        set_setting::<hydrus_core::thumbnail::ThumbnailRatingSettings>(&store, move |s| {
            s.icon_size = f64::from(size);
        });
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
            let native = native_of(&windows, edit.window());
            headless::render(&native, 640, 1000);
            // the preview is below the fold: one wheel dispatch reaches it
            native.dispatch_event(WindowEvent::PointerScrolled {
                position: slint::LogicalPosition::new(24.0, 500.0),
                delta_x: 0.0,
                delta_y: -10000.0,
            });
            let pixels = render_until(&native, (640, 1000), |pixels| {
                star_in(pixels, 640, size as usize, [BORDER, FILL]).is_some()
            });
            let file = format!("star_svg_{}_{size}.png", name.replace(' ', "_"));
            check_star(
                &pixels,
                640,
                size as usize,
                [BORDER, FILL],
                &file,
                &format!("{name} at {size} in the editor's examples"),
            );
            edit.invoke_cancel_clicked();
            manage.invoke_cancel_clicked();
        }
    }
}

// leaf: audit-media-service-star-appearance
#[test]
fn an_svg_star_is_drawn_by_the_thumbnails_viewer_and_preview() {
    svg_star_is_drawn_everywhere("smile", "star_svg_smile_24.png");
}

// (the default star the reference draws for an SVG it has not got)
// leaf: audit-media-service-star-appearance
#[test]
fn a_missing_svg_is_the_default_star_in_the_thumbnails_viewer_and_preview() {
    svg_star_is_drawn_everywhere("gone", "star_svg_gone_24.png");
}

fn svg_star_is_drawn_everywhere(service_svg: &str, file: &str) {
    let size = 24_u32;
    {
        let (dirs, store) = basic_store();
        configure(
            &store,
            "favourites",
            StarAppearance::Svg(service_svg.into()),
            Some(unusual()),
        );
        set_setting::<hydrus_core::thumbnail::ThumbnailRatingSettings>(&store, move |s| {
            s.icon_size = f64::from(size);
        });
        set_setting::<hydrus_core::media_viewer::MediaViewerSettings>(&store, move |s| {
            s.rating_icon_size = f64::from(size);
        });
        set_setting::<hydrus_store::settings::RatingContextSizes>(&store, move |s| {
            s.preview_icon_size = f64::from(size);
        });
        let client = Client::with(dirs, store);
        client.ui.invoke_search_edited("system:everything".into());
        client.ui.invoke_search_accepted();
        let main = client.native();
        let (width, height) = (1000_u32, 900_u32);
        let colours = [BORDER, FILL];

        // the thumbnail overlay
        for _ in 0..3 {
            headless::render(&main, width, height);
        }
        let pixels = render_until(&main, (width, height), |pixels| {
            star_in(pixels, width as usize, size as usize, colours).is_some()
        });
        check_star(
            &pixels,
            width as usize,
            size as usize,
            colours,
            file,
            &format!("{service_svg} on a thumbnail"),
        );

        // the preview window's top right
        let before = headless::render(&main, width, height);
        client.bound.preview.refresh();
        client.ui.invoke_thumbnail_clicked(0, false, false);
        let started = std::time::Instant::now();
        while client.ui.get_preview_media().size().width == 0 {
            client.bound.preview.refresh();
            assert!(started.elapsed() < std::time::Duration::from_secs(5));
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        let rows: Vec<_> = client.ui.get_preview_ratings().iter().collect();
        assert!(
            rows.iter().any(|r| r.graphic.kind == 0
                && (r.graphic.svg.row_count() == 1) == (service_svg == "smile")),
            "{service_svg}: the preview's rating rows carry the SVG only when there is one"
        );
        let mut after = headless::render(&main, width, height);
        // (the grid's thumbnails carry the star too: what the preview adds)
        let is_coloured =
            |pixels: &[u8], i: usize| colours.iter().any(|c| near(&pixels[i * 4..i * 4 + 4], *c));
        for i in 0..after.len() / 4 {
            if !is_coloured(&after, i) || is_coloured(&before, i) {
                after[i * 4 + 3] = 0;
            }
        }
        check_star(
            &after,
            width as usize,
            size as usize,
            colours,
            file,
            &format!("{service_svg} in the preview window"),
        );

        // the media viewer
        client.ui.invoke_thumbnail_activated(0);
        let viewer = client
            .bound
            .viewer
            .borrow()
            .as_ref()
            .map(slint::ComponentHandle::clone_strong)
            .expect("the viewer");
        let native = native_of(client.windows(), viewer.window());
        for _ in 0..3 {
            headless::render(&native, width, height);
        }
        // (its rating is in the top right corner, away from the title text)
        let corner = |pixels: &[u8]| {
            let mut pixels = pixels.to_vec();
            for (i, pixel) in pixels.chunks_exact_mut(4).enumerate() {
                if i % width as usize + 250 < width as usize || i / (width as usize) > 70 {
                    pixel[3] = 0;
                }
            }
            pixels
        };
        let pixels = render_until(&native, (width, height), |pixels| {
            star_in(&corner(pixels), width as usize, size as usize, colours).is_some()
        });
        let pixels = corner(&pixels);
        check_star(
            &pixels,
            width as usize,
            size as usize,
            colours,
            file,
            &format!("{service_svg} in the media viewer"),
        );
    }
}

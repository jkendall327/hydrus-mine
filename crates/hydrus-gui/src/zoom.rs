//! Zooming and panning a file in the media viewer, as the reference's
//! media container does: a file opens at its default zoom (its type's
//! rules), centred; zooming in and out steps through the regular zooms
//! and canvas fit, keeping the point zoomed on (the pointer, by default)
//! still; switching goes between 100% and canvas fit; panning moves it a
//! twelfth of the smaller of it and the canvas, and dragging moves it with
//! the pointer. A file that isn't zoomable fills the canvas. Positions and
//! sizes are whole logical pixels, as Qt's are. Plain Rust, tested
//! directly.

use std::cell::{Cell, RefCell};
use std::collections::BTreeMap;
use std::rc::Rc;
use std::sync::Arc;

use hydrus_core::Mime;
use hydrus_core::media_viewer::{
    MediaViewerSettings, ShowAction, ZoomCentre, ZoomType, canvas_zooms, media_size, next_zoom,
};

/// A logical pixel position or size.
pub type Point = (i32, i32);

#[derive(Debug, Clone)]
pub struct Zoom {
    settings: MediaViewerSettings,
    mime: Mime,
    resolution: Option<(u32, u32)>,
    show: ShowAction,
    /// The canvas, in logical pixels, and its device pixel ratio.
    canvas: Point,
    ratio: f64,
    zooms: BTreeMap<ZoomType, f64>,
    current: f64,
    current_type: ZoomType,
    /// The file's top left.
    position: Point,
}

impl Zoom {
    /// A file of type `mime` and `resolution` in a canvas of `canvas`
    /// logical pixels at device pixel ratio `ratio`: at its default zoom,
    /// centred.
    pub fn new(
        settings: MediaViewerSettings,
        mime: Mime,
        resolution: Option<(u32, u32)>,
        canvas: Point,
        ratio: f64,
    ) -> Self {
        let show = settings.view(mime).media_show_action;
        let mut zoom = Self {
            settings,
            mime,
            resolution,
            show,
            canvas,
            ratio,
            zooms: BTreeMap::new(),
            current: 1.0,
            current_type: ZoomType::Full,
            position: (0, 0),
        };
        zoom.reinit();
        zoom
    }

    /// Whether the file is shown, so zooms (`IsZoomable`).
    pub fn zoomable(&self) -> bool {
        self.show.zoomable()
    }

    pub fn zoom(&self) -> f64 {
        self.current
    }

    /// The zoom of a zoom type in this canvas.
    pub fn zoom_of(&self, zoom_type: ZoomType) -> f64 {
        self.zooms[&zoom_type]
    }

    pub fn position(&self) -> Point {
        self.position
    }

    /// The file's size at the zoom (`CalculateMediaContainerSize`).
    pub fn size(&self) -> Point {
        self.size_at(self.current)
    }

    fn size_at(&self, zoom: f64) -> Point {
        let (width, height) = media_size(self.mime, self.resolution, zoom);
        let logical = |raw: u32| (f64::from(raw) / self.ratio) as i32;
        (logical(width), logical(height))
    }

    /// Where the file is drawn, as (x, y, width, height): its zoomed box,
    /// or, not zoomable, the canvas.
    pub fn rect(&self) -> Rect {
        if !self.zoomable() {
            return (0, 0, self.canvas.0, self.canvas.1);
        }
        let (width, height) = self.size();
        (self.position.0, self.position.1, width, height)
    }

    /// The size to render video frames at: the file's on screen, in device
    /// pixels, but no bigger than twice the canvas (frames are scaled up
    /// past that).
    pub fn render_size(&self) -> (u32, u32) {
        let (_, _, width, height) = self.rect();
        let device = |v: i32| (f64::from(v.max(1)) * self.ratio).round();
        let (width, height) = (device(width), device(height));
        let limit = (
            f64::from(self.canvas.0.max(1)) * self.ratio * 2.0,
            f64::from(self.canvas.1.max(1)) * self.ratio * 2.0,
        );
        let scale = (limit.0 / width).min(limit.1 / height).min(1.0);
        (
            ((width * scale).round() as u32).max(1),
            ((height * scale).round() as u32).max(1),
        )
    }

    /// The default zoom, centred (`ZoomReinit`, `ResetCenterPosition`).
    fn reinit(&mut self) {
        self.zooms = canvas_zooms(
            &self.settings,
            self.mime,
            self.resolution,
            (
                u32::try_from(self.canvas.0.max(0)).unwrap_or(0),
                u32::try_from(self.canvas.1.max(0)).unwrap_or(0),
            ),
            self.ratio,
        );
        self.current_type = self.settings.default_zoom_type;
        self.current = self.zooms[&self.current_type];
        self.recentre();
    }

    fn recentre(&mut self) {
        let (width, height) = self.size();
        self.position = (
            (self.canvas.0 - width).div_euclid(2),
            (self.canvas.1 - height).div_euclid(2),
        );
    }

    /// Show another file in its place, keeping the zoom and position, as
    /// the duplicate filter does going between a pair's files
    /// (`ZoomMaintainingZoom`): the new file as tall as the old one was
    /// (both landscape) or as wide (both portrait; otherwise whichever
    /// side differs less), unless at the default zoom that would spill a
    /// little over the canvas's edge. A file without a resolution, or one
    /// not shown, takes its default zoom.
    #[allow(clippy::float_cmp)]
    pub fn switch_to(&mut self, mime: Mime, resolution: Option<(u32, u32)>) {
        let useful = |r: Option<(u32, u32)>| r.filter(|&(w, h)| w > 0 && h > 0);
        let previous = (
            self.mime,
            useful(self.resolution),
            self.current,
            self.size(),
        );
        let previous_zooms = self.zooms.clone();
        self.mime = mime;
        self.resolution = resolution;
        self.show = self.settings.view(mime).media_show_action;
        let position = self.position;
        let (Some(old), Some(new)) = (previous.1, useful(resolution)) else {
            self.reinit();
            self.position = position;
            return;
        };
        if !self.zoomable() {
            self.reinit();
            self.position = position;
            return;
        }
        self.zooms = canvas_zooms(
            &self.settings,
            mime,
            resolution,
            (
                u32::try_from(self.canvas.0.max(0)).unwrap_or(0),
                u32::try_from(self.canvas.1.max(0)).unwrap_or(0),
            ),
            self.ratio,
        );
        let (_, _, previous_zoom, (shown_width, shown_height)) = previous;
        let (old_width, old_height) = media_size(previous.0, Some(old), previous_zoom);
        let (old_w, old_h) = (f64::from(old.0), f64::from(old.1));
        let (new_w, new_h) = (f64::from(new.0), f64::from(new.1));
        let width_locked = f64::from(old_width) / new_w;
        let height_locked = f64::from(old_height) / new_h;
        let width_locked_size = self.size_at(width_locked);
        let height_locked_size = self.size_at(height_locked);
        let mut lock_height = if old_w > old_h && new_w > new_h {
            true
        } else if old_w < old_h && new_w < new_h {
            false
        } else {
            let width_difference = old_w.max(new_w) / old_w.min(new_w);
            let height_difference = old_h.max(new_h) / old_h.min(new_h);
            height_difference <= width_difference
        };
        // at the default zoom, near the canvas's edges, don't spill a little
        // over them
        if previous_zoom == previous_zooms[&ZoomType::DefaultForFiletype]
            && previous_zoom <= previous_zooms[&ZoomType::Canvas] * 1.05
        {
            let (canvas_width, canvas_height) =
                (f64::from(self.canvas.0), f64::from(self.canvas.1));
            let (width, height) = (f64::from(shown_width), f64::from(shown_height));
            let near = |side: f64, canvas: f64| canvas * 0.95 <= side && side <= canvas * 1.05;
            let spills = |new: i32, side: f64| side < f64::from(new) && f64::from(new) < side * 1.1;
            if near(height, canvas_height) && spills(width_locked_size.1, height) {
                lock_height = true;
            }
            if near(width, canvas_width) && spills(height_locked_size.0, width) {
                lock_height = false;
            }
        }
        self.current = if lock_height {
            height_locked
        } else {
            width_locked
        };
    }

    /// The canvas changed size: the default zoom again, centred, as the
    /// reference's defaults have it.
    pub fn resize(&mut self, canvas: Point, ratio: f64) {
        self.resize_with_policy(canvas, ratio, true);
    }

    /// Resize without losing a detail zoom/pan when recentering is disabled.
    pub fn resize_with_policy(&mut self, canvas: Point, ratio: f64, recenter: bool) {
        if canvas != self.canvas || (ratio - self.ratio).abs() > f64::EPSILON {
            let (current, position) = (self.current, self.position);
            self.canvas = canvas;
            self.ratio = ratio;
            self.reinit();
            if !recenter {
                self.current = current;
                self.position = position;
            }
        }
    }

    /// Zoom in a step, about `pointer` if the zoom centres on it and it is
    /// over the canvas.
    pub fn zoom_in(&mut self, pointer: Option<Point>) {
        self.step(true, pointer);
    }

    pub fn zoom_out(&mut self, pointer: Option<Point>) {
        self.step(false, pointer);
    }

    fn step(&mut self, zoom_in: bool, pointer: Option<Point>) {
        if !self.zoomable() {
            return;
        }
        let rules = self.settings.view(self.mime).zoom;
        if let Some(zoom) = next_zoom(
            &self.settings,
            &rules,
            self.current,
            self.zoom_of(ZoomType::Canvas),
            zoom_in,
        ) {
            self.change_zoom(zoom, pointer);
        }
    }

    /// Between 100% and canvas fit (`ZoomSwitch`): recentred unless
    /// bigger than the canvas.
    // (zooms are compared exactly, as the reference compares them)
    #[allow(clippy::float_cmp)]
    pub fn switch(&mut self, pointer: Option<Point>) {
        if !self.zoomable() {
            return;
        }
        let canvas = self.zoom_of(ZoomType::Canvas);
        let zoom = if self.current == 1.0 {
            if canvas == 1.0 {
                return;
            }
            canvas
        } else {
            1.0
        };
        self.current_type = if zoom == 1.0 {
            ZoomType::Full
        } else {
            ZoomType::Canvas
        };
        self.change_zoom(zoom, pointer);
        if zoom <= canvas {
            self.recentre();
        }
    }

    /// The configurable top-hover switch: two or three levels, optionally
    /// overriding the chosen zoom centre with the viewer centre.
    #[allow(clippy::float_cmp)]
    pub fn switch_with_policy(&mut self, choice: usize, pointer: Option<Point>) {
        if !self.zoomable() {
            return;
        }
        let centre = self.settings.zoom_centre;
        if choice % 2 == 1 {
            self.settings.zoom_centre = ZoomCentre::ViewerCentre;
        }
        if choice < 2 {
            self.switch(pointer);
        } else {
            self.current_type = if self.current == 1.0 {
                ZoomType::Canvas
            } else if self.current_type == ZoomType::Canvas {
                ZoomType::FillAuto
            } else {
                ZoomType::Full
            };
            let zoom = self.zoom_of(self.current_type);
            self.change_zoom(zoom, pointer);
            self.recentre();
        }
        self.settings.zoom_centre = centre;
    }

    /// To the largest zoom (`ZoomMax`): the largest of the zoom steps, or
    /// with exact zooms only, the largest doubling under it.
    #[allow(clippy::float_cmp)]
    pub fn zoom_max(&mut self) {
        if !self.zoomable() {
            return;
        }
        let mut max = self.max_step();
        if self.settings.view(self.mime).zoom.exact_zooms_only {
            let mut exact = 1.0;
            while exact * 2.0 <= max {
                exact *= 2.0;
            }
            max = exact;
        }
        if self.current != max {
            self.change_zoom(max, None);
        }
    }

    /// Whether the file is as big as it may be (`IsAtMaxZoom`): at the
    /// largest zoom step, or as wide or high as the largest size allows.
    #[allow(clippy::float_cmp)]
    pub fn at_max(&self) -> bool {
        let (width, height) = self.size();
        let max = self.max_dimension();
        self.current == self.max_step() || width == max || height == max
    }

    /// The largest zoom step.
    fn max_step(&self) -> f64 {
        self.settings
            .media_zooms
            .iter()
            .copied()
            .fold(f64::MIN, f64::max)
    }

    /// The largest the file may be, on either side: less for what plays.
    fn max_dimension(&self) -> i32 {
        let animated = matches!(
            self.mime.general_class(),
            Some(Mime::GeneralAnimation | Mime::GeneralVideo | Mime::GeneralAudio)
        );
        if animated || matches!(self.show, ShowAction::Mpv | ShowAction::QtMediaPlayer) {
            8000
        } else {
            32000
        }
    }

    /// `_TryToChangeZoom`: the zoom (no bigger than the largest size
    /// allows), moved so the centre point stays put, rescued if it left
    /// the canvas.
    #[allow(clippy::float_cmp)]
    fn change_zoom(&mut self, mut zoom: f64, pointer: Option<Point>) {
        let (width, height) = self.size();
        let mut new_size = self.size_at(zoom);
        let max = self.max_dimension();
        if new_size.0 > max || new_size.1 > max {
            let max = u32::try_from(max).unwrap_or(u32::MAX);
            zoom = canvas_zooms(
                &self.settings,
                self.mime,
                self.resolution,
                (max, max),
                self.ratio,
            )[&ZoomType::Canvas];
            new_size = self.size_at(zoom);
        }
        if zoom == self.current {
            return;
        }
        if width > 0 && height > 0 {
            let viewer_centre = (self.canvas.0 / 2, self.canvas.1 / 2);
            let centre = match self.settings.zoom_centre {
                ZoomCentre::ViewerCentre => viewer_centre,
                ZoomCentre::MediaCentre => {
                    (self.position.0 + width / 2, self.position.1 + height / 2)
                }
                ZoomCentre::MediaTopLeft => self.position,
                ZoomCentre::Mouse => pointer
                    .filter(|&(x, y)| {
                        (0..self.canvas.0).contains(&x) && (0..self.canvas.1).contains(&y)
                    })
                    .unwrap_or(viewer_centre),
            };
            let along_x = f64::from(centre.0 - self.position.0) / f64::from(width);
            let along_y = f64::from(centre.1 - self.position.1) / f64::from(height);
            self.position.0 += (f64::from(width - new_size.0) * along_x) as i32;
            self.position.1 += (f64::from(height - new_size.1) * along_y) as i32;
        }
        self.current = zoom;
        self.rescue();
    }

    /// Bring the file back if it is wholly off the canvas
    /// (`RescueIfOffScreen`), a fifth of it (at most) showing.
    fn rescue(&mut self) {
        let (width, height) = self.size();
        let (x, y) = self.position;
        let (right, bottom) = (x + width - 1, y + height - 1);
        let (canvas_right, canvas_bottom) = (self.canvas.0 - 1, self.canvas.1 - 1);
        let intersects = x <= canvas_right && right >= 0 && y <= canvas_bottom && bottom >= 0;
        if intersects {
            return;
        }
        let height_buffer = height.min(height / 5);
        if bottom < 0 {
            self.position.1 = height_buffer - height + 1;
        } else if y > canvas_bottom {
            self.position.1 = canvas_bottom - height_buffer;
        }
        let width_buffer = width.min(width / 5);
        if right < 0 {
            self.position.0 = width_buffer - width + 1;
        } else if x > canvas_right {
            self.position.0 = canvas_right - width_buffer;
        }
    }

    /// Pan by steps (`DoManualPan`): each a twelfth of the smaller of the
    /// file and the canvas.
    pub fn pan(&mut self, x_steps: i32, y_steps: i32) {
        let (width, height) = self.size();
        self.position.0 += x_steps * (self.canvas.0.min(width) / 12);
        self.position.1 += y_steps * (self.canvas.1.min(height) / 12);
    }

    /// Dragged by `delta`.
    pub fn drag(&mut self, delta: Point) {
        if self.zoomable() {
            self.position.0 += delta.0;
            self.position.1 += delta.1;
        }
    }
}

/// Where a file is drawn: (x, y, width, height).
pub type Rect = (i32, i32, i32, i32);

/// A still drawn sharply over the window's quickly scaled one.
#[derive(Debug, Clone)]
pub(crate) struct Overlay {
    pub image: slint::Image,
    /// Where, in logical pixels.
    pub rect: (f32, f32, f32, f32),
    /// Whether it hides what is under it (else that is hidden).
    pub opaque: bool,
}

type Setter<T> = Rc<dyn Fn(T)>;

/// The still drawn sharply ([`crate::still`]), its renderer, and what was
/// asked of it and is showing.
#[derive(Default)]
struct Sharp {
    still: Option<Arc<hydrus_media::Raster>>,
    renderer: Option<crate::still::Renderer>,
    polling: slint::Timer,
    /// The newest render asked for.
    asked: u64,
    plan: Option<crate::still::Plan>,
    /// The overlay showing: its plan, the file's box then, and its image.
    shown: Option<(crate::still::Plan, Rect, slint::Image)>,
}

/// What a window is told of its file's zoom (none, if it doesn't zoom).
type Watch = Rc<dyn Fn(Option<f64>)>;
type ResizePolicy = Rc<dyn Fn() -> bool>;

/// A window's zoomed file, kept as the file, the window and the user
/// change it, and drawn by the window's own setters: the file's box, and
/// a still's sharp overlay.
#[derive(Clone)]
pub(crate) struct Zoomed {
    settings: MediaViewerSettings,
    resize_policy: Rc<RefCell<ResizePolicy>>,
    origin: Rc<dyn Fn() -> Option<Point>>,
    last_origin: Rc<Cell<Option<Point>>>,
    zoom: Rc<RefCell<Option<Zoom>>>,
    sharp: Rc<RefCell<Sharp>>,
    /// The window's canvas and device pixel ratio, while it is open.
    canvas: Rc<dyn Fn() -> Option<(Point, f64)>>,
    /// Draw the file at (x, y, width, height).
    draw: Setter<Rect>,
    overlay: Setter<Option<Overlay>>,
    /// Told the zoom each time the file is drawn.
    watch: Rc<RefCell<Option<Watch>>>,
}

impl std::fmt::Debug for Zoomed {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Zoomed")
            .field("zoom", &self.zoom.borrow())
            .finish_non_exhaustive()
    }
}

impl Zoomed {
    /// For `window`, whose canvas (in logical pixels) `canvas` gives.
    pub fn within<W: slint::ComponentHandle + 'static>(
        window: &W,
        settings: MediaViewerSettings,
        canvas: impl Fn(&W) -> Point + 'static,
        draw: impl Fn(&W, Rect) + 'static,
        overlay: impl Fn(&W, Option<Overlay>) + 'static,
    ) -> Self {
        let (sized, drawn, overlaid, positioned) = (
            window.as_weak(),
            window.as_weak(),
            window.as_weak(),
            window.as_weak(),
        );
        Self {
            settings,
            resize_policy: Rc::new(RefCell::new(Rc::new(|| true))),
            origin: Rc::new(move || {
                let window = positioned.upgrade()?;
                let origin = window
                    .window()
                    .position()
                    .to_logical(window.window().scale_factor());
                Some((origin.x as i32, origin.y as i32))
            }),
            last_origin: Rc::default(),
            zoom: Rc::default(),
            sharp: Rc::default(),
            canvas: Rc::new(move || {
                let window = sized.upgrade()?;
                let ratio = f64::from(window.window().scale_factor());
                Some((canvas(&window), ratio))
            }),
            draw: Rc::new(move |rect| {
                if let Some(window) = drawn.upgrade() {
                    draw(&window, rect);
                }
            }),
            overlay: Rc::new(move |shown| {
                if let Some(window) = overlaid.upgrade() {
                    overlay(&window, shown);
                }
            }),
            watch: Rc::default(),
        }
    }

    /// Read resize policy live; other canvases keep the default recentering.
    pub fn set_resize_policy(&self, policy: impl Fn() -> bool + 'static) {
        *self.resize_policy.borrow_mut() = Rc::new(policy);
    }

    /// Tell `watch` the zoom (none, for a file that doesn't zoom) each time
    /// the file is drawn, as the reference tells its top hover frame.
    pub fn watch(&self, watch: impl Fn(Option<f64>) + 'static) {
        *self.watch.borrow_mut() = Some(Rc::new(watch));
    }

    /// The file shown next is this still (its file decoded whole), to be
    /// drawn sharply; or isn't one.
    pub fn set_still(&self, still: Option<Arc<hydrus_media::Raster>>) {
        let mut sharp = self.sharp.borrow_mut();
        sharp.still = still;
        sharp.asked += 1;
        sharp.plan = None;
        sharp.shown = None;
        sharp.polling.stop();
        drop(sharp);
        (self.overlay)(None);
    }

    /// Release still rendering when its owner closes, even if callers retain
    /// the window handle. Dropping the channels lets the worker finish and exit.
    pub fn close(&self) {
        let mut sharp = self.sharp.borrow_mut();
        sharp.polling.stop();
        sharp.renderer = None;
        sharp.still = None;
        sharp.plan = None;
        sharp.shown = None;
        sharp.asked += 1;
        drop(sharp);
        *self.zoom.borrow_mut() = None;
        (self.overlay)(None);
    }

    /// Show a file of this type and resolution (none: of unknown type) at
    /// its default zoom.
    pub fn show(&self, shape: Option<(Mime, Option<(u32, u32)>)>) {
        self.last_origin.set((self.origin)());
        let Some((canvas, ratio)) = (self.canvas)() else {
            return;
        };
        *self.zoom.borrow_mut() = shape.map(|(mime, resolution)| {
            Zoom::new(self.settings.clone(), mime, resolution, canvas, ratio)
        });
        self.draw();
    }

    fn draw(&self) {
        let rect = if let Some(zoom) = self.zoom.borrow().as_ref() {
            zoom.rect()
        } else {
            let ((width, height), _) = (self.canvas)().unwrap_or(((0, 0), 1.0));
            (0, 0, width, height)
        };
        (self.draw)(rect);
        self.sharpen(rect);
        let zoom = self
            .zoom
            .borrow()
            .as_ref()
            .filter(|z| z.zoomable())
            .map(Zoom::zoom);
        let watch = self.watch.borrow().clone();
        if let Some(watch) = watch {
            watch(zoom);
        }
    }

    /// Draw the still sharply at `rect`: what was drawn moved along with
    /// it, if just moved, while the part now showing is rendered.
    fn sharpen(&self, rect: Rect) {
        self.sharpen_with(rect, crate::still::Renderer::new);
    }

    fn sharpen_with(
        &self,
        rect: Rect,
        make_renderer: impl FnOnce() -> std::io::Result<crate::still::Renderer>,
    ) {
        let plan = {
            let sharp = self.sharp.borrow();
            let zoom = self.zoom.borrow();
            match (sharp.still.as_ref(), zoom.as_ref(), (self.canvas)()) {
                (Some(still), Some(zoom), Some((canvas, ratio))) if zoom.zoomable() => {
                    let rules = self.settings.view(zoom.mime).zoom;
                    crate::still::plan(rect, canvas, ratio, (still.width(), still.height()), &rules)
                }
                _ => None,
            }
        };
        let mut sharp = self.sharp.borrow_mut();
        let Some(plan) = plan else {
            if sharp.shown.take().is_some() || sharp.plan.take().is_some() {
                sharp.asked += 1;
                sharp.polling.stop();
                drop(sharp);
                (self.overlay)(None);
            }
            return;
        };
        let opaque = sharp
            .still
            .as_ref()
            .is_some_and(|still| !still.has_alpha_channel());
        // what shows already, at the same zoom, moves with the file
        let moved = sharp.shown.as_ref().and_then(|(shown, then, image)| {
            (then.2 == rect.2 && then.3 == rect.3).then(|| {
                let (dx, dy) = ((rect.0 - then.0) as f32, (rect.1 - then.1) as f32);
                Overlay {
                    image: image.clone(),
                    rect: (
                        shown.rect.0 + dx,
                        shown.rect.1 + dy,
                        shown.rect.2,
                        shown.rect.3,
                    ),
                    opaque,
                }
            })
        });
        let unchanged = sharp
            .shown
            .as_ref()
            .is_some_and(|(shown, _, _)| shown.clip == plan.clip && shown.target == plan.target);
        if moved.is_none() {
            sharp.shown = None;
        }
        if unchanged {
            // (just moved: nothing to render)
            if let Some((shown, then, _)) = sharp.shown.as_mut() {
                *shown = plan;
                *then = rect;
            }
            sharp.plan = Some(plan);
            sharp.asked += 1;
            sharp.polling.stop();
        } else if sharp.plan != Some(plan) {
            sharp.asked += 1;
            sharp.plan = Some(plan);
            let (id, still) = (
                sharp.asked,
                sharp.still.clone().expect("planned for a still"),
            );
            if sharp.renderer.is_none() {
                match make_renderer() {
                    Ok(renderer) => sharp.renderer = Some(renderer),
                    Err(error) => {
                        // The original image is already drawn by Slint. Keep
                        // that nearest-pixel fallback without a poller waiting
                        // forever for a worker the OS could not start. Retain
                        // the failed plan to avoid retrying on every paint;
                        // another zoom, file or clipping plan can retry.
                        eprintln!("could not start still rendering: {error}");
                        sharp.shown = None;
                        sharp.polling.stop();
                        drop(sharp);
                        (self.overlay)(None);
                        return;
                    }
                }
            }
            sharp
                .renderer
                .as_ref()
                .expect("renderer was started")
                .request(id, still, plan);
            let this = Rc::downgrade(&self.sharp);
            let overlay = self.overlay.clone();
            sharp.polling.start(
                slint::TimerMode::Repeated,
                std::time::Duration::from_millis(8),
                move || {
                    let Some(this) = this.upgrade() else { return };
                    let Ok(mut sharp) = this.try_borrow_mut() else {
                        return;
                    };
                    let newest = sharp
                        .renderer
                        .as_ref()
                        .and_then(crate::still::Renderer::newest);
                    let Some((id, pixels)) = newest else { return };
                    if id != sharp.asked {
                        return;
                    }
                    sharp.polling.stop();
                    let Some(plan) = sharp.plan else { return };
                    let image = pixels.image();
                    let opaque = sharp
                        .still
                        .as_ref()
                        .is_some_and(|still| !still.has_alpha_channel());
                    sharp.shown = Some((plan, rect, image.clone()));
                    drop(sharp);
                    overlay(Some(Overlay {
                        image,
                        rect: plan.rect,
                        opaque,
                    }));
                },
            );
        }
        drop(sharp);
        (self.overlay)(moved);
    }
    /// Apply a change to the zoom, then draw.
    fn change(&self, change: impl FnOnce(&mut Zoom)) {
        if let Some(zoom) = self.zoom.borrow_mut().as_mut() {
            change(zoom);
        }
        self.draw();
    }

    /// Show another file in the old one's place, at the same zoom and
    /// position (see [`Zoom::switch_to`]).
    pub fn switch_to(&self, shape: Option<(Mime, Option<(u32, u32)>)>) {
        let switched = match (self.zoom.borrow_mut().as_mut(), shape) {
            (Some(zoom), Some((mime, resolution))) => {
                zoom.switch_to(mime, resolution);
                true
            }
            _ => false,
        };
        if switched {
            self.draw();
        } else {
            self.show(shape);
        }
    }

    /// The window changed size.
    pub fn resized(&self) {
        if let Some((canvas, ratio)) = (self.canvas)() {
            let recenter = (self.resize_policy.borrow())();
            let origin = (self.origin)();
            let previous = self.last_origin.replace(origin);
            let delta = match (previous, origin) {
                (Some(previous), Some(current)) => (previous.0 - current.0, previous.1 - current.1),
                _ => (0, 0),
            };
            self.change(|zoom| {
                zoom.resize_with_policy(canvas, ratio, recenter);
                if !recenter {
                    zoom.drag(delta);
                }
            });
        }
    }

    /// Zoom in (`direction` 1), out (-1) or switch (0), about `pointer`.
    pub fn zoom(&self, direction: i32, pointer: Option<Point>) {
        self.change(|zoom| match direction.signum() {
            1 => zoom.zoom_in(pointer),
            -1 => zoom.zoom_out(pointer),
            _ => zoom.switch(pointer),
        });
    }

    pub fn switch_with_policy(&self, choice: usize, pointer: Option<Point>) {
        self.change(|zoom| zoom.switch_with_policy(choice, pointer));
    }

    pub fn pan(&self, x_steps: i32, y_steps: i32) {
        self.change(|zoom| zoom.pan(x_steps, y_steps));
    }

    pub fn drag(&self, delta: Point) {
        self.change(|zoom| zoom.drag(delta));
    }

    /// To the largest zoom.
    pub fn zoom_max(&self) {
        self.change(Zoom::zoom_max);
    }

    /// The zoom now, the zoom fitting the canvas, and whether it is as big
    /// as it may be, if the file zooms (for the viewer's menu).
    pub fn state(&self) -> Option<crate::viewer_menu::ZoomState> {
        let zoom = self.zoom.borrow();
        let zoom = zoom.as_ref().filter(|z| z.zoomable())?;
        Some(crate::viewer_menu::ZoomState {
            current: zoom.zoom(),
            canvas: zoom.zoom_of(ZoomType::Canvas),
            at_max: zoom.at_max(),
        })
    }

    /// The size to render video at, if the file's zoom is known.
    pub fn render_size(&self) -> Option<(u32, u32)> {
        self.zoom.borrow().as_ref().map(Zoom::render_size)
    }
}

#[cfg(test)]
mod resource_tests {
    use super::*;
    use slint::ComponentHandle as _;

    #[test]
    fn an_unavailable_worker_keeps_the_original_image_and_stops_polling() {
        let windows = crate::headless::init();
        let window = crate::MediaViewerWindow::new().unwrap();
        let raster = Arc::new(
            hydrus_media::Raster::new(
                2,
                2,
                3,
                vec![255, 0, 0, 0, 255, 0, 0, 0, 255, 255, 255, 255],
            )
            .unwrap(),
        );
        window.set_media(crate::image(&raster));
        window.set_media_width(8.0);
        window.set_media_height(8.0);
        window.show().unwrap();
        let drawn = windows.get(0).unwrap();
        let before = crate::headless::render(&drawn, 32, 32);
        let zoomed = crate::zoom_window!(window, MediaViewerSettings::default(), |_| (8, 8));
        zoomed.set_still(Some(raster));
        *zoomed.zoom.borrow_mut() = Some(Zoom::new(
            MediaViewerSettings::default(),
            Mime::ImageJpeg,
            Some((2, 2)),
            (8, 8),
            1.0,
        ));
        zoomed.sharp.borrow().polling.start(
            slint::TimerMode::Repeated,
            std::time::Duration::from_secs(60),
            || {},
        );
        let attempts = Cell::new(0);
        let unavailable = || {
            attempts.set(attempts.get() + 1);
            Err(std::io::Error::new(
                std::io::ErrorKind::WouldBlock,
                "worker unavailable",
            ))
        };
        zoomed.sharpen_with((0, 0, 8, 8), unavailable);
        assert_eq!(attempts.get(), 1);
        assert!(!window.get_sharp_shown());
        let source_size = window.get_media().size();
        assert_eq!((source_size.width, source_size.height), (2, 2));
        assert_eq!(crate::headless::render_snapshot(&drawn, 32, 32), before);
        assert!(zoomed.sharp.borrow().renderer.is_none());
        assert!(!zoomed.sharp.borrow().polling.running());
        zoomed.sharpen_with((0, 0, 8, 8), || {
            panic!("unchanged failed plans must not repeatedly spawn workers")
        });
        zoomed.sharpen_with((0, 0, 16, 16), unavailable);
        assert_eq!(attempts.get(), 2, "a changed plan can retry");
        zoomed.close();
        assert!(zoomed.sharp.borrow().still.is_none());
        assert!(zoomed.sharp.borrow().plan.is_none());
        assert!(zoomed.zoom.borrow().is_none());
        zoomed.sharpen_with((0, 0, 8, 8), || {
            panic!("retained closed canvases must not restart workers")
        });
        window.hide().unwrap();
    }
}

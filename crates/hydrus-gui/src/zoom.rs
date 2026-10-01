//! Zooming and panning a file in the media viewer, as the reference's
//! media container does: a file opens at its default zoom (its type's
//! rules), centred; zooming in and out steps through the regular zooms
//! and canvas fit, keeping the point zoomed on (the pointer, by default)
//! still; switching goes between 100% and canvas fit; panning moves it a
//! twelfth of the smaller of it and the canvas, and dragging moves it with
//! the pointer. A file that isn't zoomable fills the canvas. Positions and
//! sizes are whole logical pixels, as Qt's are. Plain Rust, tested
//! directly.

use std::collections::BTreeMap;

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
        self.current = self.zooms[&self.settings.default_zoom_type];
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
        if canvas != self.canvas || (ratio - self.ratio).abs() > f64::EPSILON {
            self.canvas = canvas;
            self.ratio = ratio;
            self.reinit();
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
        self.change_zoom(zoom, pointer);
        if zoom <= canvas {
            self.recentre();
        }
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

/// A window's zoomed file, kept as the file, the window and the user
/// change it, and drawn by the window's own setter.
#[derive(Clone)]
pub(crate) struct Zoomed {
    settings: MediaViewerSettings,
    zoom: std::rc::Rc<std::cell::RefCell<Option<Zoom>>>,
    /// The window's canvas and device pixel ratio, while it is open.
    canvas: std::rc::Rc<dyn Fn() -> Option<(Point, f64)>>,
    /// Draw the file at (x, y, width, height).
    draw: std::rc::Rc<dyn Fn(Rect)>,
}

impl std::fmt::Debug for Zoomed {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Zoomed")
            .field("zoom", &self.zoom.borrow())
            .finish_non_exhaustive()
    }
}

impl Zoomed {
    /// For `window`, whose whole is the canvas, drawing with `draw`.
    pub fn of<W: slint::ComponentHandle + 'static>(
        window: &W,
        settings: MediaViewerSettings,
        draw: impl Fn(&W, Rect) + 'static,
    ) -> Self {
        Self::within(
            window,
            settings,
            |window| {
                let window = window.window();
                let size = window.size().to_logical(window.scale_factor());
                (size.width as i32, size.height as i32)
            },
            draw,
        )
    }

    /// For `window`, whose canvas (in logical pixels) `canvas` gives.
    pub fn within<W: slint::ComponentHandle + 'static>(
        window: &W,
        settings: MediaViewerSettings,
        canvas: impl Fn(&W) -> Point + 'static,
        draw: impl Fn(&W, Rect) + 'static,
    ) -> Self {
        let (sized, drawn) = (window.as_weak(), window.as_weak());
        Self {
            settings,
            zoom: std::rc::Rc::default(),
            canvas: std::rc::Rc::new(move || {
                let window = sized.upgrade()?;
                let ratio = f64::from(window.window().scale_factor());
                Some((canvas(&window), ratio))
            }),
            draw: std::rc::Rc::new(move |rect| {
                if let Some(window) = drawn.upgrade() {
                    draw(&window, rect);
                }
            }),
        }
    }

    /// Show a file of this type and resolution (none: of unknown type) at
    /// its default zoom.
    pub fn show(&self, shape: Option<(Mime, Option<(u32, u32)>)>) {
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
            self.change(|zoom| zoom.resize(canvas, ratio));
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

    pub fn pan(&self, x_steps: i32, y_steps: i32) {
        self.change(|zoom| zoom.pan(x_steps, y_steps));
    }

    pub fn drag(&self, delta: Point) {
        self.change(|zoom| zoom.drag(delta));
    }

    /// The size to render video at, if the file's zoom is known.
    pub fn render_size(&self) -> Option<(u32, u32)> {
        self.zoom.borrow().as_ref().map(Zoom::render_size)
    }
}

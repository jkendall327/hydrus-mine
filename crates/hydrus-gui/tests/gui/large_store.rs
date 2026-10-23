//! The main window over a large store with awkward data: thousands of files
//! in the inbox, very long tags, emoji, right-to-left, combining and control
//! characters, files with no or enormous dimensions, very long URLs, big
//! collections and many tag services. Every page, sort, collect, scroll,
//! the media viewer and manage tags are drawn through the software renderer
//! at several scale factors and window sizes, and each must stay quick.
//!
//! The fast variant runs in the normal suite; `HYDRUS_STRESS=1` runs the
//! large one (tens of thousands of files; run it in release mode too).

use std::sync::Arc;
use std::time::{Duration, Instant};

use hydrus_core::time::TimestampMs;
use hydrus_core::{HashId, Mime, ServiceId, Sha256, Tag};
use hydrus_gui::{Bound, MainWindow, Pages, SearchPage, bind, headless};
use hydrus_store::Store;
use hydrus_store::content::MappingAction;
use hydrus_store::media::{FileFlags, FileInfo};
use hydrus_store::services::{Service, ServiceKind};
use hydrus_store::synth::{SynthSpec, populate};
use slint::platform::WindowAdapter as _;
use slint::{ComponentHandle as _, Model as _};

const STUB_THUMBNAIL: &[u8] = include_bytes!("../../../../static/hydrus.png");

/// What the store holds beyond the synthetic library.
struct Odd {
    /// The awkward files, in the inbox.
    files: Vec<HashId>,
}

/// Tags no real library should have, but some do.
fn awkward_tags() -> Vec<String> {
    let zalgo: String = "zalgo"
        .chars()
        .flat_map(|c| std::iter::once(c).chain("\u{0301}\u{0336}\u{0489}".chars().cycle().take(60)))
        .collect();
    vec![
        format!("title:{}", "a very long title ".repeat(300)),
        format!("creator:{}", "x".repeat(4_000)),
        "series:🦊🦊🦊 fox 🏳️‍🌈 👨‍👩‍👧‍👦".repeat(40),
        "character:مرحبا بالعالم שלום עולם".into(),
        "meta:\u{202e}reversed\u{202c} text".into(),
        "meta:zero\u{200b}width\u{200d}joiners\u{feff}".into(),
        format!("meta:{zalgo}"),
        "meta:control\u{7}bell\u{1b}escape".into(),
        "漢字かなカナ한국어".repeat(100),
        "ｆｕｌｌｗｉｄｔｈ　ｔｅｘｔ".into(),
    ]
}

/// A store of `files` synthetic files, `inbox` of them in the inbox, plus
/// the awkward ones.
fn large_store(files: u32, inbox: u32) -> (tempfile::TempDir, Arc<Store>, Odd) {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path()).unwrap();
    // many tag services
    let services: Vec<Service> = store
        .snapshot()
        .services
        .all()
        .map(|s| (**s).clone())
        .collect();
    let mut desired = services.clone();
    for i in 0..12u8 {
        desired.push(Service {
            id: ServiceId(0),
            key: hydrus_core::ServiceKey::new([[0xA0, i].as_slice(), &[7; 30]].concat()),
            name: format!(
                "tag service {i} with a long name to measure {}",
                "…".repeat(i.into())
            ),
            kind: ServiceKind::LocalTags,
        });
    }
    hydrus_store::services_management::apply(&store, services, desired).unwrap();
    let library = populate(
        &store,
        SynthSpec {
            files,
            tags: 3_000,
            tags_per_file: 8,
            siblings: 100,
            parents: 100,
            batch: 2_000,
            ..SynthSpec::default()
        },
    )
    .unwrap();
    let snapshot = store.snapshot();
    let my_files = snapshot
        .services
        .builtin(hydrus_core::service::builtin_keys::MY_FILES)
        .unwrap()
        .id;
    let tag_services: Vec<ServiceId> = snapshot.services.tag_services().map(|s| s.id).collect();
    drop(snapshot);
    let synthetic: Vec<HashId> = library.hashes.iter().map(|(id, _)| *id).collect();
    let synthetic_all = synthetic.clone();
    let in_inbox = synthetic[..inbox as usize].to_vec();
    let odd = store
        .write_content(move |w| {
            let mut odd = Vec::new();
            // (width, height, mime)
            let shapes: [(Option<u32>, Option<u32>, Mime); 8] = [
                (Some(0), Some(0), Mime::ImagePng),
                (None, None, Mime::ImageJpeg),
                (Some(u32::MAX), Some(u32::MAX), Mime::ImagePng),
                (Some(1_000_000), Some(1), Mime::ImagePng),
                (Some(1), Some(1_000_000), Mime::ImagePng),
                (Some(1), Some(1), Mime::ImageGif),
                (Some(70_000), Some(70_000), Mime::ImageJpeg),
                (Some(640), Some(480), Mime::VideoMp4),
            ];
            for (i, (width, height, mime)) in shapes.into_iter().enumerate() {
                let mut bytes = [0xEEu8; 32];
                bytes[0] = u8::try_from(i).unwrap();
                let id = hydrus_store::master::intern_hash(w.conn(), &Sha256(bytes))?;
                let info = FileInfo {
                    size: if i == 2 { u64::MAX / 2 } else { 1_000 },
                    mime,
                    original_mime: None,
                    width,
                    height,
                    duration_ms: (mime == Mime::VideoMp4).then_some(u64::MAX / 4),
                    num_frames: (mime == Mime::VideoMp4).then_some(u64::MAX / 4),
                    has_audio: false,
                    num_words: None,
                    file_modified: Some(TimestampMs::from_millis(0)),
                    pixel_hash: None,
                    blurhash: None,
                    flags: FileFlags(0),
                };
                w.add_file_info(id, &info, false)?;
                odd.push(id);
            }
            let now = w.now_ms();
            let rows: Vec<(HashId, Option<i64>)> = odd.iter().map(|&id| (id, Some(now))).collect();
            w.add_files(my_files, &rows)?;
            w.inbox(&odd)?;
            w.inbox(&in_inbox)?;
            let tag = |w: &mut hydrus_store::content::ContentWriter<'_>,
                       service: ServiceId,
                       text: &str,
                       files: &[HashId]|
             -> hydrus_store::Result<()> {
                let Some(tag) = Tag::new(text) else {
                    return Ok(());
                };
                let tag = hydrus_store::master::intern_tag(w.conn(), &tag)?;
                w.update_mappings(service, &MappingAction::Add, tag, files)?;
                Ok(())
            };
            // the awkward tags on every awkward file, on every tag service
            for &service in &tag_services {
                for text in awkward_tags() {
                    tag(w, service, &text, &odd)?;
                }
            }
            // thousands of page tags, a banner of tens of thousands of
            // characters
            for page in 1..=3_000 {
                tag(
                    w,
                    tag_services[0],
                    &format!("page:chapter {page} part"),
                    &odd,
                )?;
            }
            // a big collection: one series over a few thousand files
            let big: Vec<HashId> = synthetic.iter().step_by(3).take(3_000).copied().collect();
            tag(w, tag_services[0], "series:the big one", &big)?;
            for (i, chunk) in big.chunks(7).enumerate() {
                tag(w, tag_services[0], &format!("page:{i}"), chunk)?;
            }
            // very long URLs, and many of them
            let mut urls: Vec<String> = (0..300)
                .map(|i| {
                    format!(
                        "https://example.com/post/{i}?q={}",
                        "%F0%9F%A6%8A".repeat(i)
                    )
                })
                .collect();
            urls.push(format!("https://example.com/{}", "a".repeat(20_000)));
            w.add_urls(&odd, &urls)?;
            w.set_note(odd[0], "a long note", &"note text ".repeat(10_000))?;
            Ok(odd)
        })
        .unwrap();
    let all: Vec<HashId> = synthetic_all.iter().chain(&odd).copied().collect();
    // a thumbnail for every file
    let snapshot = store.snapshot();
    let hashes = store
        .read(move |conn| hydrus_store::master::hashes(conn, &all))
        .unwrap();
    for hash in hashes.values() {
        let path = snapshot.storage.thumbnail_path(hash).unwrap();
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, STUB_THUMBNAIL).unwrap();
    }
    drop(snapshot);
    (dir, store, Odd { files: odd })
}

/// How long one step may take.
const STEP: Duration = Duration::from_secs(10);

/// Times a step, failing a slow one.
fn step<R>(what: &str, f: impl FnOnce() -> R) -> R {
    let start = Instant::now();
    let out = f();
    let took = start.elapsed();
    eprintln!("{what}: {took:?}");
    assert!(took < STEP, "{what} took {took:?}");
    out
}

/// Draws every window shown at each size and scale.
fn draw_all(windows: &headless::Windows, sizes: &[(u32, u32, f32)]) {
    for &(width, height, scale_factor) in sizes {
        for n in 0..windows.count() {
            let Some(native) = windows.get(n).filter(|w| w.window().is_visible()) else {
                continue;
            };
            native
                .window()
                .dispatch_event(slint::platform::WindowEvent::ScaleFactorChanged { scale_factor });
            headless::render(&native, width, height);
        }
    }
}

/// Lets the grid load what it shows.
fn settle(ui: &MainWindow, bound: &Bound) {
    let first = usize::try_from(ui.get_grid_first_row()).unwrap_or(0);
    let count = usize::try_from(ui.get_grid_visible_rows()).unwrap_or(1);
    bound.rows.paint_tick(true, first, count);
    bound.rows.wait();
    slint::platform::update_timers_and_animations();
}

/// How much of the window to drive.
struct Run<'a> {
    files: u32,
    inbox: u32,
    searches: &'a [&'a str],
    /// Window sizes and scale factors everything is drawn at (sorts and
    /// collects only at the first).
    sizes: &'a [(u32, u32, f32)],
    /// Every `n`th sort and collect.
    every: usize,
    /// How many of the sizes the media viewer is drawn at.
    viewer_sizes: usize,
}

fn drive(run: &Run<'_>) {
    let sizes = run.sizes;
    // (generating is not the GUI's work, so it has no limit)
    let start = Instant::now();
    let (_dir, store, odd) = large_store(run.files, run.inbox);
    eprintln!("generating the store: {:?}", start.elapsed());
    let windows = headless::init();
    for &search in run.searches {
        let ui = MainWindow::new().unwrap();
        let bound = bind(&ui, Pages::single(SearchPage::new(store.clone())));
        ui.show().unwrap();
        ui.window().set_size(slint::LogicalSize::new(1100.0, 700.0));
        step(&format!("{search}: opening"), || {
            ui.invoke_search_edited(search.into());
            ui.invoke_search_accepted();
            settle(&ui, &bound);
        });
        let page = bound.current.borrow().clone();
        let results = page.borrow().results().len();
        eprintln!("{search}: {results} items");
        assert!(results > 0, "{search}");
        step(&format!("{search}: drawing"), || draw_all(&windows, sizes));

        // scrolled to the middle and to the end
        for fraction in [0.5, 1.0] {
            step(&format!("{search}: scrolled to {fraction}"), || {
                let end = (ui.get_grid_content_height() - ui.get_grid_visible_height()).max(0.0);
                ui.set_grid_scroll(-end * fraction);
                settle(&ui, &bound);
                draw_all(&windows, sizes);
            });
        }
        step(&format!("{search}: select all"), || {
            ui.invoke_select_all();
            draw_all(&windows, &sizes[..1]);
        });
        let sorts = ui.get_sort_names().row_count();
        for sort in (0..sorts).step_by(run.every) {
            let name = ui.get_sort_names().row_data(sort).unwrap();
            step(&format!("{search}: sort {name}"), || {
                ui.invoke_sort_chosen(i32::try_from(sort).unwrap());
                settle(&ui, &bound);
                draw_all(&windows, &sizes[..1]);
            });
        }
        let collects = ui.get_collect_choices().row_count();
        for collect in (0..collects).step_by(run.every) {
            let name = ui.get_collect_choices().row_data(collect).unwrap().name;
            let at = i32::try_from(collect).unwrap();
            step(&format!("{search}: collect by {name}"), || {
                ui.invoke_collect_toggled(at, true);
                ui.set_grid_scroll(0.0);
                settle(&ui, &bound);
                draw_all(&windows, &sizes[..1]);
                ui.invoke_select_all();
                draw_all(&windows, &sizes[..1]);
                ui.invoke_collect_toggled(at, false);
            });
        }
        // the awkward files in the media viewer, and manage tags on them
        let indices: Vec<usize> = {
            let page = page.borrow();
            page.results()
                .iter()
                .enumerate()
                .filter(|(_, item)| odd.files.contains(item))
                .map(|(i, _)| i)
                .collect()
        };
        for (n, &index) in indices.iter().enumerate() {
            let index = i32::try_from(index).unwrap();
            step(&format!("{search}: viewing item {index}"), || {
                ui.invoke_thumbnail_activated(index);
                draw_all(&windows, &sizes[..run.viewer_sizes]);
                let viewer = bound
                    .viewer
                    .borrow()
                    .as_ref()
                    .map(slint::ComponentHandle::clone_strong);
                if let Some(viewer) = viewer {
                    viewer.invoke_close_requested();
                }
            });
            // (the awkward files all have the same tags)
            if n > 0 {
                continue;
            }
            step(&format!("{search}: manage tags on item {index}"), || {
                ui.invoke_thumbnail_clicked(index, false, false);
                ui.invoke_manage_tags_selected();
                draw_all(&windows, sizes);
                let tags = bound
                    .manage_tags
                    .borrow()
                    .as_ref()
                    .map(slint::ComponentHandle::clone_strong);
                if let Some(tags) = tags {
                    tags.invoke_cancel();
                }
            });
        }
        ui.hide().unwrap();
        drop(bound);
    }
}

#[test]
fn a_realistic_store_is_browsed_quickly_and_drawn_without_failing() {
    drive(&Run {
        files: 1_000,
        inbox: 400,
        searches: &["system:inbox"],
        sizes: &[(1100, 700, 1.0), (1920, 1080, 2.0)],
        every: 5,
        viewer_sizes: 1,
    });
}

/// `HYDRUS_STRESS=1`: tens of thousands of files, an inbox of thousands.
#[test]
fn a_large_store_is_browsed_quickly_and_drawn_without_failing() {
    if std::env::var_os("HYDRUS_STRESS").is_none() {
        eprintln!("skipped: set HYDRUS_STRESS=1 to run");
        return;
    }
    drive(&Run {
        files: 40_000,
        inbox: 4_000,
        searches: &["system:inbox", "system:archive", "system:everything"],
        sizes: &[
            (1100, 700, 1.0),
            (3840, 2160, 1.0),
            (1920, 1080, 2.0),
            (2560, 1440, 3.0),
            (1280, 800, 1.25),
        ],
        every: 1,
        viewer_sizes: 5,
    });
}

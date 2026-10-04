//! The actual page menu callbacks, retained errors, rules drafts and a live engine.
use hydrus_core::{
    bandwidth::{BandwidthType, Rule, Rules, Tracker},
    network::NetworkContext,
    time::TimestampMs,
};
use hydrus_gui::{Clip, MainWindow, headless, network_data_window, network_job_control as gui};
use hydrus_gui_model::network_job_control::Target;
use hydrus_store::{
    Store,
    bandwidth::BandwidthSettings,
    network_runtime::{self, JobAction, JobControl, JobError, NetworkJob, Snapshot, WaitReason},
    settings,
};
use slint::{ComponentHandle, Model};
use std::{cell::RefCell, rc::Rc, sync::Arc};
fn now() -> i64 {
    TimestampMs::now().millis() / 1000
}
fn until(mut condition: impl FnMut() -> bool) {
    let start = std::time::Instant::now();
    while !condition() {
        assert!(start.elapsed() < std::time::Duration::from_secs(8));
        slint::platform::update_timers_and_animations();
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
}
fn snapshot() -> Snapshot {
    Snapshot {
        epoch: "test daemon".into(),
        at: now(),
        jobs: vec![NetworkJob {
            id: 42,
            url: "https://example.com/a%20b".into(),
            status: "waiting".into(),
            wait: WaitReason::Connection,
            bytes_read: 0,
            bytes_total: None,
            speed: 0,
            contexts: vec![
                NetworkContext::global(),
                NetworkContext::domain("example.com"),
                NetworkContext::downloader_page("0000000000000001"),
            ],
            obeys_bandwidth: true,
        }],
        controls: vec![JobControl {
            id: 42,
            created: now(),
            gallery: false,
            domain_ok: false,
            tokens_ok: false,
            auto_override: false,
        }],
        ..Snapshot::default()
    }
}
#[test]
fn page_cog_rules_cancel_apply_and_error_owner_boundary() {
    let rendered = headless::init();
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path()).unwrap();
    let queue = store
        .write(|ctx| {
            hydrus_store::queues::create_queue(
                ctx.conn(),
                hydrus_store::queues::QueueKind::Urls,
                "control test",
                None,
                &hydrus_core::import_options::ImportOptionsSlice::default(),
                now(),
            )
        })
        .unwrap();
    assert_eq!(queue, 1);
    store
        .write(|ctx| settings::set(ctx.conn(), &snapshot()))
        .unwrap();
    let window = MainWindow::new().unwrap();
    let page = hydrus_gui::SearchPage::url_downloader(store.clone(), queue, None, vec![]);
    let bound = hydrus_gui::bind(&window, hydrus_gui::Pages::single(page));
    let slots = bound.network_data.clone();
    let owner = bound.network_controls.clone();
    until(|| {
        window.invoke_control_menu(false);
        !window.get_file_cog().url.is_empty()
    });
    let menu = window.get_file_cog();
    assert_eq!(menu.url, "https://example.com/a b");
    assert_eq!(menu.rules.row_count(), 4); // global, default domain, domain instance, page default
    assert_eq!(menu.actions.row_count(), 4);
    assert!(menu.actions.iter().any(|r| r.id == 2));
    let copies = Rc::new(RefCell::new(Vec::new()));
    hydrus_gui::set_clipper({
        let copies = copies.clone();
        move |clip| copies.borrow_mut().push(clip.clone())
    });
    window.invoke_control_action(false, 1);
    assert_eq!(
        copies.borrow()[0],
        Clip::Text("https://example.com/a%20b".into())
    );
    window.invoke_control_action(false, 2);
    window.invoke_control_action(false, 3);
    window.invoke_control_action(false, 5);
    window.invoke_control_action(false, 6);
    window.invoke_control_action(false, 999);
    let mut commands = Vec::new();
    until(|| {
        commands.extend(
            store
                .write(|ctx| network_runtime::take_commands(ctx.conn()))
                .unwrap(),
        );
        commands
            .iter()
            .filter(|c| {
                !matches!(
                    c.action,
                    JobAction::AutoOverrideBandwidth(_)
                        | JobAction::AutoOverrideBandwidthFor { .. }
                )
            })
            .count()
            == 4
    });
    assert!(
        commands
            .iter()
            .all(|c| c.epoch == "test daemon" && c.job == 42)
    );
    let actions = commands.iter().map(|c| c.action).collect::<Vec<_>>();
    for action in [
        JobAction::OverrideConnectionWait,
        JobAction::ScrubDomainErrors,
        JobAction::OverrideBandwidth,
        JobAction::OverrideGalleryWait,
    ] {
        assert!(actions.contains(&action));
    }

    let before = store.read(settings::get::<BandwidthSettings>).unwrap();
    window.invoke_control_action(false, 100);
    until(|| network_data_window::last_rules().is_some_and(|w| w.window().is_visible()));
    let edit = network_data_window::last_rules().unwrap();
    edit.set_amount("0".into());
    edit.invoke_add_rule();
    assert!(edit.get_error().contains("positive"));
    edit.set_requests(true);
    edit.set_monthly(false);
    edit.set_amount("23".into());
    edit.set_seconds("60".into());
    edit.invoke_add_rule();
    edit.invoke_cancel_clicked();
    assert_eq!(
        store.read(settings::get::<BandwidthSettings>).unwrap(),
        before
    );
    // A stale draft callback is harmless after its owner closes.
    edit.invoke_apply_clicked();
    assert_eq!(
        store.read(settings::get::<BandwidthSettings>).unwrap(),
        before
    );
    window.invoke_control_action(false, 100);
    until(|| network_data_window::last_rules().is_some_and(|w| w.window().is_visible()));
    let edit = network_data_window::last_rules().unwrap();
    edit.set_requests(true);
    edit.set_monthly(false);
    edit.set_amount("23".into());
    edit.set_seconds("60".into());
    edit.invoke_add_rule();
    edit.invoke_apply_clicked();
    until(|| !edit.window().is_visible());
    let after = store.read(settings::get::<BandwidthSettings>).unwrap();
    assert!(
        after
            .rules
            .iter()
            .find(|(c, _)| c == &NetworkContext::global())
            .unwrap()
            .1
            .rules()
            .contains(&Rule::new(BandwidthType::Requests, Some(60), 23))
    );
    window.invoke_control_action(false, 100);
    until(|| network_data_window::last_rules().is_some_and(|w| w.window().is_visible()));
    let edit = network_data_window::last_rules().unwrap();
    assert!(
        edit.get_rows()
            .iter()
            .any(|r| r.cells.row_data(0).unwrap() == "23 requests")
    );
    edit.invoke_cancel_clicked();

    store
        .write(|ctx| {
            let mut s = snapshot();
            s.errors.push(JobError {
                id: 42,
                url: s.jobs[0].url.clone(),
                contexts: s.jobs[0].contexts.clone(),
                gallery: false,
                text: "synthetic failure\nserver detail".into(),
            });
            s.jobs.clear();
            s.controls.clear();
            settings::set(ctx.conn(), &s)
        })
        .unwrap();
    until(|| window.get_file_cog().has_error);
    window.invoke_control_action(false, 8);
    let error = gui::last_error().unwrap();
    assert_eq!(error.get_error_text(), "synthetic failure\nserver detail");
    window.invoke_control_action(false, 9);
    assert_eq!(
        copies.borrow().last().unwrap(),
        &Clip::Text(error.get_error_text().to_string())
    );
    let width = 660;
    let height = 370;
    let pixels = headless::render(&rendered.get(rendered.count() - 1).unwrap(), width, height);
    assert!(pixels.chunks_exact(4).any(|p| p != &pixels[..4]));
    let path = std::env::var_os("HYDRUS_NETWORK_SCREENSHOTS").map_or_else(
        || std::path::PathBuf::from(env!("CARGO_TARGET_TMPDIR")),
        std::path::PathBuf::from,
    );
    std::fs::create_dir_all(&path).unwrap();
    headless::save_png(
        &path.join("network-control-error.png"),
        &pixels,
        width,
        height,
    )
    .unwrap();
    error.invoke_close_clicked();
    owner.clear_error(Target {
        queue: 1,
        gallery: false,
    });
    assert!(!window.get_file_cog().has_error);
    let count = copies.borrow().len();
    window.invoke_control_action(false, 9);
    assert_eq!(copies.borrow().len(), count);
    assert!(!window.get_search_cog().has_error);
    drop(owner);
    drop(bound);
    window.invoke_control_action(false, 7); // callbacks cannot outlive the control's owner
    slots.close();
}

#[test]
fn page_override_reaches_live_engine_and_auto_policy_reaches_next_request() {
    use hydrus_net::{BandwidthScope, Job, NetEngine, NetOptions, Request};
    use std::io::{Read, Write};
    headless::init();
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path()).unwrap();
    let mut used = Tracker::new(now());
    used.report_requests(1, now());
    used.report_data(1, now());
    store
        .write(move |ctx| {
            settings::set(
                ctx.conn(),
                &BandwidthSettings {
                    rules: vec![(
                        NetworkContext::global(),
                        Rules::new([
                            Rule::new(BandwidthType::Requests, Some(3600), 1),
                            Rule::new(BandwidthType::Data, Some(15), 1),
                        ]),
                    )],
                    ..BandwidthSettings::default()
                },
            )?;
            hydrus_store::bandwidth::save_usage(ctx.conn(), &[(NetworkContext::global(), used)])
        })
        .unwrap();
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}/file", listener.local_addr().unwrap());
    let server = std::thread::spawn(move || {
        for _ in 0..2 {
            let (mut stream, _) = listener.accept().unwrap();
            let mut request = [0; 4096];
            let size = stream.read(&mut request).unwrap();
            assert!(size > 0);
            stream
                .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\nok")
                .unwrap();
        }
    });
    let engine = Arc::new(NetEngine::new(store.clone(), NetOptions::default()).unwrap());
    let (done, completed) = std::sync::mpsc::channel();
    let worker = std::thread::spawn({
        let engine = engine.clone();
        move || {
            let runtime = tokio::runtime::Runtime::new().unwrap();
            runtime.block_on(async move {
                for _ in 0..2 {
                    let job = Job::scoped(BandwidthScope {
                        contexts: vec![NetworkContext::downloader_page("0000000000000001")],
                        ..BandwidthScope::default()
                    });
                    let engine_fetch = engine.clone();
                    let request = Request::get(url.clone());
                    let fetch =
                        tokio::spawn(async move { engine_fetch.fetch(&request, &job).await });
                    while !fetch.is_finished() {
                        engine.publish_runtime().unwrap();
                        tokio::time::sleep(std::time::Duration::from_millis(25)).await;
                    }
                    fetch.await.unwrap().unwrap();
                    engine.publish_runtime().unwrap();
                    done.send(()).unwrap();
                }
            });
        }
    });
    let window = MainWindow::new().unwrap();
    let slots = network_data_window::Slots::default();
    let _owner = gui::bind(
        &window,
        store.clone(),
        Rc::new(|gallery| Some(Target { queue: 1, gallery })),
        slots,
    );
    until(|| {
        window.invoke_control_menu(false);
        window.get_file_cog().actions.iter().any(|r| r.id == 5)
    });
    let first = engine.runtime_snapshot().jobs[0].id;
    window.invoke_control_action(false, 5);
    until(|| completed.try_recv().is_ok());
    until(|| {
        engine
            .runtime_snapshot()
            .jobs
            .first()
            .is_some_and(|j| j.id != first && j.wait == WaitReason::Bandwidth)
    });
    let created = engine.runtime_snapshot().controls[0].created;
    window.invoke_control_action(false, 7);
    assert!(window.get_file_cog().auto_override);
    until(|| completed.try_recv().is_ok());
    assert!(
        now() > created + 5,
        "auto override must wait past the reference's five-second boundary"
    );
    assert!(engine.runtime_snapshot().jobs.is_empty());
    assert!(engine.runtime_snapshot().errors.is_empty());
    worker.join().unwrap();
    server.join().unwrap();
}

#[test]
fn auto_policy_belongs_to_page_control_and_retires_when_page_closes() {
    use std::{cell::Cell, collections::HashSet};
    headless::init();
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path()).unwrap();
    let mut s = snapshot();
    for (id, queue) in [(43, 2), (44, 3)] {
        let mut job = s.jobs[0].clone();
        job.id = id;
        job.contexts[2] = NetworkContext::downloader_page(format!("{queue:016x}"));
        job.url = format!("https://example.com/{queue}");
        s.jobs.push(job);
        let mut meta = s.controls[0].clone();
        meta.id = id;
        s.controls.push(meta);
    }
    store
        .write(move |ctx| settings::set(ctx.conn(), &s))
        .unwrap();
    let window = MainWindow::new().unwrap();
    let queue = Rc::new(Cell::new(1));
    let page = Rc::new(RefCell::new("page A".to_owned()));
    let alive = Rc::new(RefCell::new(HashSet::from([
        "page A".to_owned(),
        "page B".to_owned(),
    ])));
    let owner = gui::bind_owned(
        &window,
        store.clone(),
        Rc::new({
            let queue = queue.clone();
            move |gallery| {
                Some(Target {
                    queue: queue.get(),
                    gallery,
                })
            }
        }),
        Rc::new({
            let page = page.clone();
            move || page.borrow().clone()
        }),
        network_data_window::Slots::default(),
    );
    owner.set_owner_alive(Rc::new({
        let alive = alive.clone();
        move |key| alive.borrow().contains(key)
    }));
    until(|| {
        window.invoke_control_menu(false);
        !window.get_file_cog().url.is_empty()
    });
    window.invoke_control_action(false, 7);
    assert!(window.get_file_cog().auto_override);
    queue.set(2);
    window.invoke_control_menu(false);
    assert_eq!(window.get_file_cog().url, "https://example.com/2");
    assert!(
        window.get_file_cog().auto_override,
        "highlight changes reuse this widget's policy"
    );
    *page.borrow_mut() = "page B".into();
    queue.set(3);
    window.invoke_control_menu(false);
    assert!(
        !window.get_file_cog().auto_override,
        "another page owns another widget"
    );
    *page.borrow_mut() = "page A".into();
    queue.set(2);
    window.invoke_control_menu(false);
    assert!(window.get_file_cog().auto_override);
    alive.borrow_mut().remove("page A");
    until(|| !window.get_file_cog().auto_override);
    let mut commands = Vec::new();
    until(|| {
        commands.extend(
            store
                .write(|ctx| network_runtime::take_commands(ctx.conn()))
                .unwrap(),
        );
        commands.iter().any(|c| {
            c.job == 43
                && matches!(
                    c.action,
                    JobAction::AutoOverrideBandwidthFor { enabled: false, .. }
                )
        })
    });
    assert!(commands.iter().any(|c| c.job == 43
        && matches!(
            c.action,
            JobAction::AutoOverrideBandwidthFor { enabled: true, .. }
        )));
    window.invoke_control_action(false, 7);
    assert!(!window.get_file_cog().auto_override);
}

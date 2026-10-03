//! The manage subscriptions dialog's "duplicate": a copy of the selected,
//! named "name (1)", saying "1 objects added!", and "apply" writing it
//! with a copy of each query's file log. Tested against the reference's
//! in hydrus-gui-model.

use hydrus_core::subscriptions::{QueryState, SubscriptionSettings};
use hydrus_gui::{MainWindow, Pages, bind, headless};
use hydrus_store::queues::{self, FileSeedMeta, NewFileSeed, SeedStatus, SeedType};
use hydrus_store::subscriptions;

use crate::subscriptions::{asked, now, open_dialog, rows, store};

#[test]
fn a_duplicate_is_written_with_its_file_logs() {
    let (_dirs, store) = store();
    let now = now();
    store
        .write(move |ctx| {
            let conn = ctx.conn();
            let settings = SubscriptionSettings {
                gug_name: "example tag search".into(),
                ..SubscriptionSettings::default()
            };
            let id = subscriptions::create_subscription(conn, "artist", &settings)?.unwrap();
            let queue = subscriptions::add_query(conn, id, &QueryState::new("blue"), now)?;
            let seeds: Vec<NewFileSeed> = (0..3)
                .map(|i| {
                    let url = format!("https://booru.example/post/{i}");
                    NewFileSeed {
                        seed_type: SeedType::Url,
                        data: url.clone(),
                        data_for_comparison: url,
                        source_time: None,
                        referral_url: None,
                        meta: FileSeedMeta::default(),
                    }
                })
                .collect();
            queues::add_file_seeds(conn, queue, &seeds, false, now)?;
            let mut first = queues::file_seeds(conn, queue)?.remove(0);
            first.status = SeedStatus::SuccessfulAndNew;
            queues::update_file_seed(conn, &first)
        })
        .unwrap();
    let _windows = headless::init();
    let ui = MainWindow::new().unwrap();
    let bound = bind(&ui, Pages::open(store.clone()).unwrap());
    let dialog = open_dialog(&ui, &bound);

    dialog.invoke_row_clicked(0, false, false);
    dialog.invoke_duplicate();
    let (title, message, _) = asked(&dialog);
    assert_eq!(
        (title.as_str(), message.as_str()),
        ("Information", "1 objects added!")
    );
    dialog.invoke_chosen(0);
    let shown = rows(&dialog);
    let names: Vec<(&str, bool)> = shown.iter().map(|r| (r.0[0].as_str(), r.1)).collect();
    assert_eq!(names, [("artist", false), ("artist (1)", true)]);

    dialog.invoke_apply();
    let logs: Vec<(String, Vec<(String, SeedStatus)>)> = store
        .read(|c| {
            let mut logs = Vec::new();
            for s in subscriptions::subscriptions(c)? {
                for q in subscriptions::queries(c, s.id)? {
                    let seeds = queues::file_seeds(c, q.queue_id)?
                        .into_iter()
                        .map(|f| (f.data, f.status))
                        .collect();
                    logs.push((format!("{}: {}", s.name, q.state.query_text), seeds));
                }
            }
            Ok(logs)
        })
        .unwrap();
    assert_eq!(logs.len(), 2);
    assert_eq!(logs[0].1, logs[1].1);
    assert_eq!(logs[1].0, "artist (1): blue");
    assert_eq!(logs[1].1.len(), 3);
    assert_eq!(logs[1].1[0].1, SeedStatus::SuccessfulAndNew);
}

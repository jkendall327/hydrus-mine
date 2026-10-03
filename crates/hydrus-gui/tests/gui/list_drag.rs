//! Dragging across a list's rows selects them, from the row pressed to the
//! row under the pointer, as the reference's lists (Qt's extended
//! selection) do: pressed on the manage subscriptions dialog's list, with
//! the pointer moved down and back up.

use slint::platform::{PointerEventButton, WindowEvent};
use slint::{LogicalPosition, Model as _};

use hydrus_core::subscriptions::SubscriptionSettings;
use hydrus_gui::{MainWindow, Pages, bind, headless};
use hydrus_store::subscriptions;

use crate::subscriptions::{open_dialog, rows, store};

fn selected(dialog: &hydrus_gui::SubscriptionsWindow) -> Vec<String> {
    rows(dialog)
        .into_iter()
        .filter(|(_, on)| *on)
        .map(|(cells, _)| cells[0].clone())
        .collect()
}

#[test]
fn dragging_across_rows_selects_them() {
    let (_dirs, store) = store();
    store
        .write(|ctx| {
            for name in ["a", "b", "c", "d", "e"] {
                subscriptions::create_subscription(
                    ctx.conn(),
                    name,
                    &SubscriptionSettings::default(),
                )?;
            }
            Ok(())
        })
        .unwrap();
    let windows = headless::init();
    let ui = MainWindow::new().unwrap();
    let bound = bind(&ui, Pages::open(store.clone()).unwrap());
    let dialog = open_dialog(&ui, &bound);
    let last = (0..100)
        .take_while(|&n| windows.get(n).is_some())
        .last()
        .unwrap();
    let window = windows.get(last).unwrap();
    // (laid out at a size)
    headless::render(&window, 900, 700);
    let at = |y: f32| LogicalPosition::new(60.0, y);
    let press = |y: f32| {
        window.dispatch_event(WindowEvent::PointerPressed {
            position: at(y),
            button: PointerEventButton::Left,
        });
    };
    let release = |y: f32| {
        window.dispatch_event(WindowEvent::PointerReleased {
            position: at(y),
            button: PointerEventButton::Left,
        });
    };
    let to = |y: f32| window.dispatch_event(WindowEvent::PointerMoved { position: at(y) });
    // the first row's place
    let first = (0..300)
        .map(|y| y as f32)
        .find(|&y| {
            press(y);
            release(y);
            selected(&dialog) == ["a"]
        })
        .expect("a row to press");
    let y = first + 5.0;
    press(y);
    to(y + 22.0);
    to(y + 2.0 * 22.0);
    assert_eq!(selected(&dialog), ["a", "b", "c"]);
    // back up: from the row pressed
    to(y + 22.0);
    assert_eq!(selected(&dialog), ["a", "b"]);
    release(y + 22.0);
    // a move with nothing pressed selects nothing more
    to(y + 4.0 * 22.0);
    assert_eq!(selected(&dialog), ["a", "b"]);
    assert_eq!(dialog.get_rows().row_count(), 5);
}

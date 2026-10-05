#!/usr/bin/env python3
"""Real Help QAction and genuine five-second scheduler → JobStatus → toaster.

Two overlapping launches retain actual background jobs, delays and publication
times. Main hiding leaves existing scheduled work alive. No scheduler or clock
is replaced; observers forward actual CallLater and pub unchanged.
"""
import json
import sqlite3
import sys
import tempfile
import time
from pathlib import Path

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
MESSAGE = 'This is a delayed popup message.'


def record(session):
    def drive():
        from qtpy import QtTest as T, QtWidgets as W
        controller = session.controller
        gui = controller.gui
        scheduled, published, jobs, trace = [], [], [], []
        original_later, original_pub = controller.CallLater, controller.pub
        menu = None
        started = time.monotonic()

        def elapsed():
            return round((time.monotonic() - started) * 1000, 3)

        def later(delay, callback, *args, **kwargs):
            job = original_later(delay, callback, *args, **kwargs)
            if args == (MESSAGE,):
                scheduled.append(dict(at_ms=elapsed(), delay_seconds=delay,
                                      callback=callback.__name__, arguments=list(args),
                                      remaining_seconds=job.GetTimeDeltaUntilDue(), job=job))
            return job

        def pub(topic, *args, **kwargs):
            if topic == 'message' and args[0].GetStatusText() == MESSAGE:
                job = args[0]
                jobs.append(job)
                published.append(dict(at_ms=elapsed(), text=job.GetStatusText(),
                                      title=job.GetStatusTitle(), done=job.IsDone(),
                                      pausable=job.IsPausable(), cancellable=job.IsCancellable()))
            return original_pub(topic, *args, **kwargs)

        def find(menu, labels):
            for action in menu.actions():
                if action.text() == labels[0]:
                    return action if len(labels) == 1 else find(action.menu(), labels[1:])
            raise ValueError(labels)

        def wait_to(target):
            while elapsed() < target:
                T.QTest.qWait(max(1, min(50, round(target - elapsed()))))
            trace.append(dict(at_ms=elapsed(), requested_ms=target,
                              main_visible=gui.isVisible(), published=len(published)))

        try:
            controller.CallLater, controller.pub = later, pub
            menu, label = gui._InitialiseMenuInfoHelp()
            action = find(menu, ['debug', 'gui actions', 'make a popup in five seconds'])
            started = time.monotonic()
            action.trigger()
            wait_to(1000)
            action.trigger()
            gui.hide()
            for target in [4900, 5300, 5900, 6400]:
                wait_to(target)
            assert len(scheduled) == len(published) == 2
            assert [item['published'] for item in trace] == [0, 0, 1, 1, 2]
            assert all(item['job'].IsWorkComplete() for item in scheduled)
            gui.show()
            manager = gui._message_manager
            manager._CheckPending()
            manager._Update()
            T.QTest.qWait(100)
            cards = []
            for index in range(manager._message_vbox.count()):
                card = manager._message_vbox.itemAt(index).widget()
                if card.GetJobStatus() in jobs:
                    cards.append(dict(text=card._text_1.text(), title_hidden=card._title.isHidden(),
                                      cancel_hidden=card._cancel_button.isHidden(),
                                      width=card.width(), height=card.height()))
            assert len(cards) == 2
            manager.grab().save(str(HERE / 'fixtures/debug_delayed_popup.png'))
            return dict(menu_path=[label.replace('&', ''), 'debug', 'gui actions', action.text()],
                        schedule=[{k: v for k, v in item.items() if k != 'job'} for item in scheduled],
                        published=published, trace=trace, cards=cards,
                        scheduler='actual controller CallLater/background SingleJob',
                        clock='unmodified real monotonic observer')
        finally:
            controller.CallLater, controller.pub = original_later, original_pub
            for item in scheduled:
                if not item['job'].IsWorkComplete():
                    item['job'].Cancel()
            for job in jobs:
                job.FinishAndDismiss()
            gui.show()
            if menu is not None:
                menu.deleteLater()
            W.QApplication.processEvents()

    return session.controller.CallBlockingToQt(session.controller.gui, drive)


def main():
    import hydrus_driver
    import record_api
    if len(sys.argv) > 1 and sys.argv[1] == '--child':
        database = record_api.unpack_fixture('basic')
        conn = sqlite3.connect(str(Path(database) / 'client.db'))
        raw = json.loads(conn.execute('SELECT dictionary_string FROM services WHERE service_type=18').fetchone()[0])
        for key, value in raw[2]:
            if key == [0, 'port']:
                value[1] = None
        conn.execute('UPDATE services SET dictionary_string=? WHERE service_type=18', (json.dumps(raw),))
        conn.commit()
        conn.close()
        Path(sys.argv[2]).write_text(json.dumps(hydrus_driver.run_client(database, record)))
        return
    with tempfile.TemporaryDirectory() as directory:
        output = Path(directory) / 'result.json'
        hydrus_driver.run_in_subprocess(str(Path(__file__).resolve()), '--child', str(output))
        result = json.loads(output.read_text())
    (HERE / 'fixtures/debug_delayed_popup.json').write_text(json.dumps(result, indent=2) + '\n')
    print('wrote debug_delayed_popup.json')


if __name__ == '__main__':
    main()

#!/usr/bin/env python3
"""Actual Help/debug QAction, private JobStatus publication and scheduled text/title.

The genuine _DebugLongTextPopup schedules every update through CallLater. We
record those delays, then drain the captured JobStatus setters in schedule order
under an owned deterministic word sequence. Actual PopupMessage widgets present
each accepted state. No network request or live delayed task escapes this probe.
"""
import json
import sqlite3
import sys
import tempfile
from pathlib import Path

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))


def record(session):
    def drive():
        from qtpy import QtWidgets as W
        from hydrus.client.gui import ClientGUI, ClientGUIPopupMessages as P

        controller = session.controller
        gui = controller.gui
        jobs, scheduled, publications, panels, trace = [], [], [], [], []
        choice_count = [0]
        original_pub = controller.pub
        original_later = controller.CallLater
        original_choice = ClientGUI.random.choice
        words = ['test', 'a', 'longish', 'statictext', 'm8']

        def choice(candidates):
            assert list(candidates) == words
            word = words[choice_count[0] % len(words)]
            choice_count[0] += 1
            return word

        def publish(topic, *args, **kwargs):
            if topic == 'message':
                job = args[0]
                jobs.append(job)
                publications.append(dict(index=len(jobs) - 1, text=job.GetStatusText(), title=job.GetStatusTitle(), done=job.IsDone(), cancellable=job.IsCancellable(), pausable=job.IsPausable()))
                return
            return original_pub(topic, *args, **kwargs)

        def later(delay, callback, value):
            job = callback.__self__
            scheduled.append(dict(delay=delay, at_ms=round(delay * 1000), index=jobs.index(job), method=callback.__name__, value=value, callback=callback))

        def find(menu, labels):
            for action in menu.actions():
                if action.text() == labels[0]:
                    return action if len(labels) == 1 else find(action.menu(), labels[1:])
            raise ValueError(labels)

        def state(at_ms):
            for panel in panels:
                panel.UpdateMessage()
            W.QApplication.processEvents()
            trace.append(dict(at_ms=at_ms, jobs=[dict(text=job.GetStatusText(), title=job.GetStatusTitle(), done=job.IsDone(), dismissed=job.IsDismissed()) for job in jobs], cards=[dict(text=panel._text_1.text(), title=panel._title.text(), title_hidden=panel._title.isHidden(), width=panel.sizeHint().width(), height=panel.sizeHint().height()) for panel in panels]))

        container = W.QWidget(gui)
        menu = None
        try:
            controller.pub = publish
            controller.CallLater = later
            ClientGUI.random.choice = choice
            menu, label = gui._InitialiseMenuInfoHelp()
            action = find(menu, ['debug', 'gui actions', 'make a long text popup'])
            action.trigger()
            assert len(jobs) == 2 and len(scheduled) == 124 and choice_count[0] == 126
            layout = W.QVBoxLayout(container)
            for job in jobs:
                panel = P.PopupMessage(container, job)
                panels.append(panel)
                layout.addWidget(panel)
            container.show()
            state(0)
            for item in scheduled:
                item['callback'](item['value'])
                if item['at_ms'] in [200, 12400, 12600, 24800]:
                    state(item['at_ms'])
            container.adjustSize()
            W.QApplication.processEvents()
            container.grab().save(str(HERE / 'fixtures/debug_long_popup.png'))
            jobs[0].FinishAndDismiss()
            state(25000)
            return dict(menu_path=[label.replace('&', ''), 'debug', 'gui actions', action.text()], words=words, word_choice='cycle from index0 across both real producer loops', published=publications, schedule=[{k: v for k, v in item.items() if k != 'callback'} for item in scheduled], trace=trace)
        finally:
            controller.pub = original_pub
            controller.CallLater = original_later
            ClientGUI.random.choice = original_choice
            container.close()
            container.deleteLater()
            if menu is not None:
                menu.deleteLater()
            W.QApplication.processEvents()
    return session.controller.CallBlockingToQt(session.controller.gui, drive)


def main():
    import hydrus_driver
    import record_api
    if len(sys.argv) > 1 and sys.argv[1] == '--child':
        database = record_api.unpack_fixture('basic')
        # This GUI-only probe needs no listener; disable it in the private copy.
        with sqlite3.connect(str(Path(database) / 'client.db')) as connection:
            raw = json.loads(connection.execute('SELECT dictionary_string FROM services WHERE service_type=18').fetchone()[0])
            for key, value in raw[2]:
                if key == [0, 'port']:
                    value[1] = None
            connection.execute('UPDATE services SET dictionary_string=? WHERE service_type=18', (json.dumps(raw),))
        connection.close()
        Path(sys.argv[2]).write_text(json.dumps(hydrus_driver.run_client(database, record)))
        return
    with tempfile.TemporaryDirectory() as directory:
        result = Path(directory) / 'record.json'
        hydrus_driver.run_in_subprocess(str(Path(__file__).resolve()), '--child', str(result))
        data = json.loads(result.read_text())
    (HERE / 'fixtures/debug_long_popup.json').write_text(json.dumps(data, indent=2) + '\n')
    print('wrote debug_long_popup.json')


if __name__ == '__main__':
    main()

#!/usr/bin/env python3
"""Real narrow/fixed Qt question cards: wrapped text and all action bounds.

Synthetic private JobStatus only; actual PopupMessage layout and Cancel handler.
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
        from qtpy import QtCore as C, QtTest as T, QtWidgets as W
        from hydrus.client import ClientThreading
        from hydrus.client.gui import ClientGUIPopupMessages
        from hydrus.core import HydrusData

        controller = session.controller
        saved = controller.new_options.Duplicate()
        hosts = []
        cases = []
        question = 'Continue this synthetic worker with its current settings? ' * 4

        def bounds(widget, popup):
            point = widget.mapTo(popup, C.QPoint(0, 0))
            return dict(x=point.x(), y=point.y(), width=widget.width(), height=widget.height())

        try:
            for characters, fixed in [(32, True), (16, False), (16, True)]:
                controller.new_options.SetInteger('popup_message_character_width', characters)
                controller.new_options.SetBoolean('popup_message_force_min_width', fixed)
                job = ClientThreading.JobStatus(cancellable=True)
                job.SetStatusTitle('question and actions')
                job.SetVariable('popup_yes_no_question', question)
                job.SetVariable('popup_clipboard', ('copy payload', 'full synthetic payload'))
                call = HydrusData.Call(lambda: None)
                call.SetLabel('run command')
                job.SetUserCallable(call)
                host = W.QDialog(controller.gui)
                hosts.append(host)
                layout = W.QVBoxLayout(host)
                layout.setSizeConstraint(W.QLayout.SizeConstraint.SetFixedSize)
                popup = ClientGUIPopupMessages.PopupMessage(host, job)
                layout.addWidget(popup)
                popup.UpdateMessage()
                host.show()
                T.QTest.qWait(10)
                controls = {
                    name: bounds(widget, popup)
                    for name, widget in [('yes', popup._yes), ('no', popup._no),
                                         ('clipboard', popup._copy_to_clipboard_button),
                                         ('callable', popup._user_callable_button),
                                         ('cancel', popup._cancel_button)]
                }
                assert all(value['y'] + value['height'] <= popup.height() for value in controls.values())
                if characters == 16 and not fixed:
                    host.grab().save(str(HERE / 'fixtures/popup_question_layout.png'))
                popup._cancel_button.click()
                cases.append(dict(characters=characters, fixed=fixed, question=question,
                                  width=popup.width(), height=popup.height(), controls=controls,
                                  question_word_wrap=popup._text_yes_no.wordWrap(),
                                  question_bounds=bounds(popup._text_yes_no, popup),
                                  cancelled=job.IsCancelled()))
                host.hide()
            return dict(cases=cases)
        finally:
            controller.new_options = saved
            for host in hosts:
                host.hide()
                host.deleteLater()
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
    (HERE / 'fixtures/popup_question_layout.json').write_text(json.dumps(result, indent=2) + '\n')
    print('wrote popup_question_layout.json')


if __name__ == '__main__':
    main()

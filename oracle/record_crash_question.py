#!/usr/bin/env python3
"""Record the reference's Help > debug > tests > "induce a program crash".

`CrashTheProgram` asks a `GetYesYesNo` question: a dialog titled "Are you
sure?" with the message "u wot mate I'll hook u in the gabber", two accept
buttons ("nice crash", "not a nice crash") and a cancel button ("forget it").
"nice crash" calls `os.abort`; "not a nice crash" starts a repaint loop on a
thread (`CallToThread(crashtime_not_nice)`); "forget it" does neither.

This runs the real function in the running client on the `basic` fixture
with the dialog's `exec` replaced: it records the dialog's title, its text
and its buttons in layout order (label, object name), then presses each
button in turn with `os.abort` and `CallToThread` replaced by recorders, so
the process survives, and writes what each press did.

Usage: QT_QPA_PLATFORM=offscreen ~/pyenv/bin/python oracle/record_crash_question.py
       (writes fixtures/crash_question.json)
"""
import json
import os
import sys
import tempfile
from pathlib import Path

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))


def record(session):
    from qtpy import QtWidgets as QW
    from hydrus.client import ClientGlobals as CG
    from hydrus.client.gui import ClientGUI
    from hydrus.client.gui.panels import ClientGUIScrolledPanelsButtonQuestions as Q
    from hydrus.client.gui import ClientGUITopLevelWindowsPanels as TL

    c = session.controller

    def drive():
        cases = []
        for press in ('nice crash', 'not a nice crash', 'forget it', 'closed'):
            seen = {}
            calls = []
            real_exec = TL.DialogCustomButtonQuestion.exec
            real_abort = os.abort
            real_call = CG.client_controller.CallToThread

            def fake_exec(dialog):
                panel = dialog.findChild(Q.QuestionYesYesNoPanel)
                buttons = panel.findChildren(QW.QPushButton)
                labels = [w for w in panel.findChildren(QW.QLabel)]
                seen['title'] = dialog.windowTitle()
                seen['text'] = [w.text() for w in labels]
                seen['buttons'] = [dict(label=b.text(), object_name=b.objectName()) for b in buttons]
                if press == 'closed':
                    dialog.reject()
                else:
                    [b for b in buttons if b.text() == press][0].click()
                return dialog.result()

            TL.DialogCustomButtonQuestion.exec = fake_exec
            os.abort = lambda: calls.append('os.abort')
            CG.client_controller.CallToThread = lambda f, *a, **k: calls.append('CallToThread:' + f.__name__)
            try:
                ClientGUI.CrashTheProgram(c.gui)
            finally:
                TL.DialogCustomButtonQuestion.exec = real_exec
                os.abort = real_abort
                CG.client_controller.CallToThread = real_call
            cases.append(dict(press=press, dialog=seen, calls=calls))
        return dict(cases=cases)

    return c.CallBlockingToQt(c.gui, drive)


def child(path):
    import hydrus_driver, record_api
    value = hydrus_driver.run_client(record_api.unpack_fixture('basic'), record)
    Path(path).write_text(json.dumps(value))


def main():
    if len(sys.argv) > 1 and sys.argv[1] == '--child':
        child(sys.argv[2])
        return
    import hydrus_driver
    with tempfile.TemporaryDirectory() as directory:
        out = Path(directory) / 'record.json'
        hydrus_driver.run_in_subprocess(str(Path(__file__).resolve()), '--child', str(out))
        value = json.loads(out.read_text())
    path = HERE / 'fixtures' / 'crash_question.json'
    path.write_text(json.dumps(value, indent=2) + '\n')
    print(f'wrote {path}')


if __name__ == '__main__':
    main()

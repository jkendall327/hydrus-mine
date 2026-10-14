#!/usr/bin/env python3
"""Help > debug > profiling > what is this? and gui actions > make a QMessageBox.

The real Help menu's QActions are triggered with `QMessageBox.information` and
`QMessageBox.warning` replaced by recorders; the title and text each would have
shown are written out.
"""
import json
import sys
import tempfile
from pathlib import Path

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))


def record(session):
    def drive():
        from qtpy import QtWidgets as W
        controller = session.controller
        gui = controller.gui
        shown = []

        def stub(kind):
            def call(win, title, message):
                shown.append(dict(kind=kind, title=title, text=message))
            return call

        def find(menu, labels):
            for action in menu.actions():
                if action.text() == labels[0]:
                    return action if len(labels) == 1 else find(action.menu(), labels[1:])
            raise ValueError(labels)

        original = (W.QMessageBox.information, W.QMessageBox.warning)
        menu = None
        try:
            W.QMessageBox.information = stub('information')
            W.QMessageBox.warning = stub('warning')
            menu, label = gui._InitialiseMenuInfoHelp()
            out = {}
            for name, path in (('what is this?', ['debug', 'profiling', 'what is this?']),
                               ('make a QMessageBox', ['debug', 'gui actions', 'make a QMessageBox'])):
                shown.clear()
                find(menu, path).trigger()
                out[name] = list(shown)
            return out
        finally:
            W.QMessageBox.information, W.QMessageBox.warning = original
            if menu is not None:
                menu.deleteLater()
            W.QApplication.processEvents()

    return session.controller.CallBlockingToQt(session.controller.gui, drive)


def main():
    import hydrus_driver
    import record_api
    if len(sys.argv) > 1 and sys.argv[1] == '--child':
        database = record_api.unpack_fixture('basic')
        Path(sys.argv[2]).write_text(json.dumps(hydrus_driver.run_client(database, record)))
        return
    with tempfile.TemporaryDirectory() as directory:
        output = Path(directory) / 'result.json'
        hydrus_driver.run_in_subprocess(str(Path(__file__).resolve()), '--child', str(output))
        result = json.loads(output.read_text())
    (HERE / 'fixtures/debug_message_boxes.json').write_text(json.dumps(result, indent=2) + '\n')
    print('wrote debug_message_boxes.json')


if __name__ == '__main__':
    main()

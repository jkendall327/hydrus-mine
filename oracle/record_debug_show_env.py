#!/usr/bin/env python3
"""Help > debug > data actions > show env, on environments built here.

The real Help QAction is triggered with `os.environ` replaced by each case's
variables; the popup text (`HydrusEnvironment.DumpEnv`) is read back.
"""
import json
import os
import sys
import tempfile
from pathlib import Path

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))

CASES = {
    'plain': {'B': '2', 'A': '1', 'a': 'lower', 'Z': 'upper'},
    'paths split': {'PATH': '/usr/bin:/bin:/opt/x', 'XDG_DATA_DIRS': '/usr/share:/usr/local/share',
                    'HOME': '/home/me', 'LD_LIBRARY_PATH': '/one'},
    'only names with PATH or DIRS split': {'PATHEXT': '.A:.B', 'MY_DIRS': 'a:b', 'MYPATHS': 'c', 'other': 'x:y',
                                           'path': 'lower:case', 'EMPTY': '', 'PATH_EMPTY': ''},
    'unicode and spaces': {'NAME': 'caf\u00e9 au lait', 'SPACE KEY': 'a b', 'PATH': 'a b:\u00fc'},
    'empty': {},
}


def record(session):
    def drive():
        from qtpy import QtTest as T, QtWidgets as W
        controller = session.controller
        gui = controller.gui
        shown = []
        original_pub = controller.pub

        def pub(topic, *args, **kwargs):
            if topic == 'message':
                shown.append(args[0].GetStatusText())
            return original_pub(topic, *args, **kwargs)

        def find(menu, labels):
            for action in menu.actions():
                if action.text() == labels[0]:
                    return action if len(labels) == 1 else find(action.menu(), labels[1:])
            raise ValueError(labels)

        menu = None
        saved = os.environ.copy()
        try:
            controller.pub = pub
            menu, label = gui._InitialiseMenuInfoHelp()
            action = find(menu, ['debug', 'data actions', 'show env'])
            out = dict(pathsep=os.pathsep, cases={})
            for name, env in CASES.items():
                os.environ.clear()
                os.environ.update(env)
                shown.clear()
                action.trigger()
                T.QTest.qWait(50)
                out['cases'][name] = dict(env=env, messages=list(shown))
            return out
        finally:
            os.environ.clear()
            os.environ.update(saved)
            controller.pub = original_pub
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
    (HERE / 'fixtures/debug_show_env.json').write_text(json.dumps(result, indent=2) + '\n')
    print('wrote debug_show_env.json')


if __name__ == '__main__':
    main()

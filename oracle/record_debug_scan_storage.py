#!/usr/bin/env python3
"""Help > debug > data actions > scan file storage folders.

The real Help QAction is triggered with the directory picker replaced by
scripted answers (recording the title and the directory it opens in), on
directory trees built here, at each storage granularity (the client's
`client_files_subfolders` answer is replaced for the one it does not have).
The popup text ("It found N subfolders and took T" / "It cancelled: why") is
read back; the time is the real one.
"""
import json
import os
import sys
import tempfile
from pathlib import Path

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))


def build(root, shape, granularity):
    root = Path(root)
    if shape == 'three prefixes':
        for p in ('fa0', 'fa1', 'fa2'):
            (root / p / (f'{p}0' if granularity == 3 else '')).mkdir(parents=True)
    elif shape == 'empty':
        pass
    elif shape == 'seventeen files':
        for i in range(17):
            (root / f'f{i}').write_text('x')
    elif shape == 'sixteen files':
        for i in range(16):
            (root / f'f{i}').write_text('x')
    elif shape == 'six hundred and one dirs':
        for i in range(601):
            (root / f'd{i:03}').mkdir()
    elif shape == 'six hundred dirs':
        for i in range(600):
            (root / f'd{i:03}').mkdir()
    elif shape == 'twenty-one second level dirs':
        for i in range(21):
            (root / 'aa' / f'b{i:02}').mkdir(parents=True)
    elif shape == 'twenty second level dirs':
        for i in range(20):
            (root / 'aa' / f'b{i:02}').mkdir(parents=True)
    elif shape == 'seventeen second level files':
        (root / 'aa').mkdir()
        for i in range(17):
            (root / 'aa' / f'f{i}').write_text('x')


SHAPES = ['three prefixes', 'empty', 'sixteen files', 'seventeen files', 'six hundred dirs',
          'six hundred and one dirs', 'twenty second level dirs', 'twenty-one second level dirs',
          'seventeen second level files']


def record(session):
    def drive():
        from qtpy import QtTest as T, QtWidgets as W
        from hydrus.client.gui import ClientGUIDialogsQuick
        controller = session.controller
        gui = controller.gui
        shown, picks = [], []
        original_pub, original_read = controller.pub, controller.Read
        original_pick = ClientGUIDialogsQuick.PickDirectory
        fake = {'granularity': None}
        current = {'path': None}

        def pub(topic, *args, **kwargs):
            if topic == 'message':
                shown.append(args[0].GetStatusText())
            return original_pub(topic, *args, **kwargs)

        def read(action, *args, **kwargs):
            result = original_read(action, *args, **kwargs)
            if action == 'client_files_subfolders' and fake['granularity'] is not None:
                return (fake['granularity'], result[1])
            return result

        def pick(win, message, default_path=None, **kwargs):
            picks.append(dict(message=message, default_path=default_path))
            return current['path']

        def find(menu, labels):
            for action in menu.actions():
                if action.text() == labels[0]:
                    return action if len(labels) == 1 else find(action.menu(), labels[1:])
            raise ValueError(labels)

        menu = None
        try:
            controller.pub, controller.Read = pub, read
            ClientGUIDialogsQuick.PickDirectory = pick
            menu, label = gui._InitialiseMenuInfoHelp()
            action = find(menu, ['debug', 'data actions', 'scan file storage folders'])
            out = dict(db_dir=controller.GetDBDir(), real_granularity=original_read('client_files_subfolders')[0], cases=[])
            for granularity in (2, 3):
                fake['granularity'] = granularity
                for shape in SHAPES:
                    with tempfile.TemporaryDirectory() as root:
                        build(root, shape, granularity)
                        current['path'] = root
                        shown.clear()
                        picks.clear()
                        action.trigger()
                        T.QTest.qWait(50)
                        out['cases'].append(dict(granularity=granularity, shape=shape, messages=list(shown),
                                                 picks=[dict(message=p['message'], default_path_is_db_dir=p['default_path'] == controller.GetDBDir()) for p in picks]))
            # cancelling the picker
            from hydrus.core import HydrusExceptions

            def cancel(win, message, default_path=None, **kwargs):
                raise HydrusExceptions.CancelledException()
            ClientGUIDialogsQuick.PickDirectory = cancel
            shown.clear()
            action.trigger()
            T.QTest.qWait(50)
            out['cancelled_messages'] = list(shown)
            return out
        finally:
            controller.pub, controller.Read = original_pub, original_read
            ClientGUIDialogsQuick.PickDirectory = original_pick
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
    result['db_dir'] = '<db dir>'
    (HERE / 'fixtures/debug_scan_storage.json').write_text(json.dumps(result, indent=2) + '\n')
    print('wrote debug_scan_storage.json')


if __name__ == '__main__':
    main()

#!/usr/bin/env python3
"""Record manage > viewing stats > clear: `DoClearFileViewingStats` on two
files and on one, each answered no, then yes. The yes/no question is
captured (its message and the title and button labels `GetYesNo` gives
it) and so is the write the yes makes (content type, action, hashes).

Usage: QT_QPA_PLATFORM=offscreen ~/pyenv/bin/python oracle/record_clear_viewing_stats.py
       (writes fixtures/clear_viewing_stats.json)
"""
import inspect
import json
import sys
import tempfile
from pathlib import Path
HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))


def record(session):
    from qtpy import QtWidgets as QW
    from hydrus.client import ClientConstants as CC
    from hydrus.client.gui import ClientGUIDialogsQuick
    from hydrus.client.gui.media import ClientGUIMediaModalActions as M
    from hydrus.client.media import ClientMediaResult  # noqa: F401
    from hydrus.client.media import ClientMediaSingle
    controller = session.controller
    gui = controller.gui
    manifest = json.loads((HERE / 'fixtures/legacy_db/basic.manifest.json').read_text())
    hashes = [bytes.fromhex(f['hash']) for f in manifest['files'][:3]]
    defaults = inspect.signature(ClientGUIDialogsQuick.GetYesNo).parameters

    def drive():
        medias = [ClientMediaSingle.MediaSingle(m) for m in controller.Read('media_results', hashes)]
        questions, writes = [], []
        answer = [False]
        old_yesno, old_write = ClientGUIDialogsQuick.GetYesNo, controller.Write

        def yesno(win, message, **kwargs):
            questions.append({'message': message,
                'title': kwargs.get('title', defaults['title'].default),
                'yes': kwargs.get('yes_label', defaults['yes_label'].default),
                'no': kwargs.get('no_label', defaults['no_label'].default)})
            return QW.QDialog.DialogCode.Accepted if answer[0] else QW.QDialog.DialogCode.Rejected

        def write(action, *args, **kwargs):
            if action == 'content_updates':
                for service_key, updates in args[0].IterateContentUpdates():
                    for u in updates:
                        writes.append({'data_type': u.GetDataType(), 'action': u.GetAction(),
                            'hashes': sorted(h.hex() for h in u.GetHashes()), 'service': service_key.hex()})
                return None
            return old_write(action, *args, **kwargs)

        ClientGUIDialogsQuick.GetYesNo = yesno
        controller.Write = write
        steps = []
        try:
            for count in (2, 1):
                for accept in (False, True):
                    answer[0] = accept
                    questions.clear()
                    writes.clear()
                    M.DoClearFileViewingStats(gui, medias[:count] if count == 2 else medias[2:3])
                    steps.append({'files': count, 'answer': accept, 'questions': list(questions), 'writes': list(writes),
                        'hashes': [m.GetHash().hex() for m in (medias[:count] if count == 2 else medias[2:3])]})
            questions.clear()
            writes.clear()
            M.DoClearFileViewingStats(gui, [])
            steps.append({'files': 0, 'answer': None, 'questions': list(questions), 'writes': list(writes), 'hashes': []})
        finally:
            ClientGUIDialogsQuick.GetYesNo = old_yesno
            controller.Write = old_write
        return {'hashes': [h.hex() for h in hashes], 'steps': steps,
            'local_files_service': CC.HYDRUS_LOCAL_FILE_STORAGE_SERVICE_KEY.hex()}
    return controller.CallBlockingToQt(gui, drive)


def main():
    import hydrus_driver, record_api
    if len(sys.argv) > 1 and sys.argv[1] == '--child':
        Path(sys.argv[2]).write_text(json.dumps(hydrus_driver.run_client(record_api.unpack_fixture('basic'), record)))
        return
    with tempfile.TemporaryDirectory() as work:
        out = Path(work) / 'stats.json'
        hydrus_driver.run_in_subprocess(str(Path(__file__).resolve()), '--child', str(out))
        result = json.loads(out.read_text())
    (HERE / 'fixtures' / 'clear_viewing_stats.json').write_text(json.dumps(result, indent=2) + '\n')


if __name__ == '__main__':
    main()

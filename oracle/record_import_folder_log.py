#!/usr/bin/env python3
"""Record the actual import-folder cache child's ownership and cancellation.

Opens the real folder-manager Edit handler and FileSeedCacheButton dialog.
The file-log child's real row status and deletion handlers mutate its duplicate;
scripted modal answers then record child Apply/Cancel, folder-fields Cancel/Apply,
manager output and saved/reopened cache. No workers/imports are started.
"""
import json
import sys
import tempfile
from pathlib import Path
HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
NOW = 1_800_000_000


def record(session):
    from qtpy import QtWidgets as QW
    from hydrus.client import ClientConstants as CC
    from hydrus.client.gui import ClientGUIDialogsQuick, ClientGUITopLevelWindowsPanels
    from hydrus.client.gui.importing import ClientGUIImportFolders, ClientGUIFileSeedCache
    from hydrus.client.importing import ClientImportLocal, ClientImportFileSeeds
    from hydrus.core import HydrusSerialisable, HydrusTime
    controller, gui = session.controller, session.controller.gui
    old_exec, old_yesno, old_now = ClientGUITopLevelWindowsPanels.DialogEdit.exec, ClientGUIDialogsQuick.GetYesNo, HydrusTime.GetNow
    watched = tempfile.TemporaryDirectory(prefix='hydrus-folder-log-')
    def drive():
        HydrusTime.GetNow = lambda: NOW
        original = ClientImportLocal.ImportFolder('folder log recording')
        folder_fields = list(original.ToTuple())
        folder_fields[1] = watched.name
        original.SetTuple(*folder_fields)
        cache = original.GetFileSeedCache()
        seeds = []
        for i, status in enumerate([CC.STATUS_ERROR, CC.STATUS_SUCCESSFUL_BUT_REDUNDANT, CC.STATUS_UNKNOWN]):
            seed = ClientImportFileSeeds.FileSeed(ClientImportFileSeeds.FILE_SEED_TYPE_HDD, f'/srv/import/file-{i}.jpg')
            seed.created = NOW - 100 + i
            seed.modified = NOW - 50 + i
            seed.source_time = NOW - 200 + i
            seed.status, seed.note = status, f'detail {i}'
            seed.SetHash(bytes.fromhex('12' * 32))
            seeds.append(seed)
        cache.AddFileSeeds(seeds)
        def rows(cache):
            return [{'data': s.file_seed_data, 'created': s.created, 'modified': s.modified, 'source_time': s.source_time, 'status': s.status, 'note': s.note, 'hashes': [[kind, hash.hex()] for kind, hash in sorted(s.GetHashTypesToHashes().items())]} for s in cache.GetFileSeeds()]
        cases = []
        for child_ok, folder_ok in [(False, False), (True, False), (True, True)]:
            managed = original.Duplicate()
            owner = ClientGUIImportFolders.EditImportFoldersPanel(gui, [managed])
            owner._import_folders.SelectDatas([managed], deselect_others=True)
            calls, child_states = [], []
            def yesno(parent, text, *args, **kwargs):
                calls.append({'kind': 'question', 'text': text, 'answer': True})
                return QW.QDialog.DialogCode.Accepted
            ClientGUIDialogsQuick.GetYesNo = yesno
            def dialog_exec(dialog):
                title = dialog.windowTitle()
                calls.append({'kind': 'dialog', 'title': title})
                if title == 'file log':
                    child = dialog._panel
                    child_states.append({'phase': 'initial', 'rows': rows(child.GetValue())})
                    data = child._list_ctrl.GetData()
                    child._list_ctrl.SelectDatas([data[0]], deselect_others=True)
                    child._SetSelected(CC.STATUS_UNKNOWN)
                    child_states.append({'phase': 'retry', 'rows': rows(child.GetValue())})
                    child._list_ctrl.SelectDatas([data[1]], deselect_others=True)
                    child._DeleteSelected()
                    child_states.append({'phase': 'delete', 'rows': rows(child.GetValue())})
                    child_states.append({'phase': 'owner_before_child_close', 'rows': rows(managed.GetFileSeedCache())})
                    return QW.QDialog.DialogCode.Accepted if child_ok else QW.QDialog.DialogCode.Rejected
                assert title == 'edit import folder'
                panel = dialog._panel
                panel._file_seed_cache_button._ShowFileSeedCacheFrame()
                child_states.append({'phase': 'owner_after_child_close', 'rows': rows(managed.GetFileSeedCache())})
                return QW.QDialog.DialogCode.Accepted if folder_ok else QW.QDialog.DialogCode.Rejected
            ClientGUITopLevelWindowsPanels.DialogEdit.exec = dialog_exec
            owner._Edit()
            result = owner.GetValue()[0]
            reopened = HydrusSerialisable.CreateFromString(result.DumpToString())
            cases.append({'child_applied': child_ok, 'folder_applied': folder_ok, 'calls': calls, 'child_states': child_states,
                          'manager_rows': rows(result.GetFileSeedCache()), 'reopened_rows': rows(reopened.GetFileSeedCache()),
                          'original_rows': rows(original.GetFileSeedCache())})
        return {'now': NOW, 'initial': rows(original.GetFileSeedCache()), 'cases': cases}
    try: return controller.CallBlockingToQt(gui, drive)
    finally:
        ClientGUITopLevelWindowsPanels.DialogEdit.exec, ClientGUIDialogsQuick.GetYesNo, HydrusTime.GetNow = old_exec, old_yesno, old_now
        watched.cleanup()


def main():
    import hydrus_driver, record_api
    if len(sys.argv) > 1 and sys.argv[1] == '--child':
        destination = Path(sys.argv[2])
        result = hydrus_driver.run_client(record_api.unpack_fixture('basic'), record)
        destination.write_text(json.dumps(result)); return
    with tempfile.TemporaryDirectory() as work:
        destination = Path(work) / 'folder-log.json'
        hydrus_driver.run_in_subprocess(str(Path(__file__).resolve()), '--child', str(destination))
        result = json.loads(destination.read_text())
    (HERE / 'fixtures/import_folder_log.json').write_text(json.dumps(result, indent=2) + '\n')
if __name__ == '__main__': main()

#!/usr/bin/env python3
"""Actual FilesAndTrash Qt drafts and local-domain migration questions/writes.

Each transfer drives MoveOrDuplicateLocalFiles and its real migration worker on
an unpacked basic database. Only authored selections and local domains are used;
no network requests or external files are involved. Explicit sources match the
reference thumbnail menu's add/strict/merge commands.
"""
import json, sys, tempfile
from pathlib import Path
HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
KEYS = ['confirm_multiple_local_file_services_copy', 'confirm_multiple_local_file_services_move']

def record(session):
    c = session.controller
    def drive():
        from qtpy import QtWidgets as QW
        from hydrus.client import ClientOptions, ClientConstants as CC
        from hydrus.client.gui import ClientGUIDialogsQuick as Q
        from hydrus.client.gui.panels.options.FilesAndTrashPanel import FilesAndTrashPanel
        from hydrus.client.gui.media import ClientGUIMediaModalActions as A, ClientGUIMediaSimpleActions as S, ClientGUIMediaMenus as Menus
        from hydrus.client.media import ClientMediaSingle as M, ClientMediaResultCache
        from hydrus.client.metadata import ClientContentUpdates as U, ClientFileMigration as F
        from hydrus.core import HydrusConstants as HC, HydrusSerialisable, HydrusTime
        original = c.new_options
        c.new_options = original.Duplicate()
        old_question, old_thread, old_now = Q.GetYesNo, c.CallToThread, HydrusTime.GetNowMS
        panels = []
        def prefs(o): return [o.GetBoolean(key) for key in KEYS]
        def controls(p): return [p._confirm_multiple_local_file_services_copy.isChecked(), p._confirm_multiple_local_file_services_move.isChecked()]
        domains = sorted(c.services_manager.GetServices((HC.LOCAL_FILE_DOMAIN,)), key=lambda s:s.GetName())
        source, dest = domains[:2]
        source_key, dest_key = source.GetServiceKey(), dest.GetServiceKey()
        results = c.Read('media_results_from_ids', [1, 2, 3])
        hashes = [m.GetHash() for m in results]
        def update(key, action, row):
            c.WriteSynchronous('content_updates', U.ContentUpdatePackage.STATICCreateFromContentUpdate(key, U.ContentUpdate(HC.CONTENT_TYPE_FILES, action, row)))
        def reset():
            for domain in domains: update(domain.GetServiceKey(), HC.CONTENT_UPDATE_DELETE, hashes)
            for result in results: update(source_key, HC.CONTENT_UPDATE_ADD, (result.GetFileInfoManager(), 1234567890000))
            update(CC.HYDRUS_LOCAL_FILE_STORAGE_SERVICE_KEY, HC.CONTENT_UPDATE_CLEAR_DELETE_RECORD, hashes)
            update(dest_key, HC.CONTENT_UPDATE_ADD, (results[1].GetFileInfoManager(), 1234567890100))
            # The third selection is trash-only, never a transfer candidate.
            update(source_key, HC.CONTENT_UPDATE_DELETE, [hashes[2]])
        def fresh():
            cache = ClientMediaResultCache.MediaResultCache.instance()
            for result in results: cache.DropMediaResult(result.GetHashId(), result.GetHash())
            return c.Read('media_results', hashes)
        def state():
            current = fresh()
            return [{'hash':m.GetHash().hex(), 'domains':sorted(c.services_manager.GetNameSafe(k) for k in m.GetLocationsManager().GetCurrent() if k in (source_key,dest_key)), 'imports':{s.GetName():m.GetTimesManager().GetImportedTimestampMS(s.GetServiceKey()) for s in (source,dest)}, 'inbox':m.GetLocationsManager().inbox} for m in current]
        asked, jobs, answer = [], [], [False]
        def question(parent, text, **kwargs):
            asked.append(text)
            return QW.QDialog.DialogCode.Accepted if answer[0] else QW.QDialog.DialogCode.Rejected
        def thread(fn, *args, **kwargs):
            if fn is F.DoMoveOrDuplicateLocalFiles:
                jobs.append([m.GetHash().hex() for m in args[2]])
                return fn(*args, **kwargs)
            return old_thread(fn, *args, **kwargs)
        try:
            Q.GetYesNo, c.CallToThread, HydrusTime.GetNowMS = question, thread, lambda:1700000000000
            out = dict(defaults=prefs(ClientOptions.ClientOptions()), loaded=prefs(c.new_options), source=source.GetName(), destination=dest.GetName(), options=[], transfers=[])
            for values in ([False,True], [True,False], [True,True]):
                panel = FilesAndTrashPanel(c.gui); panels.append(panel)
                before = prefs(c.new_options)
                panel._confirm_multiple_local_file_services_copy.setChecked(values[0]); panel._confirm_multiple_local_file_services_move.setChecked(values[1])
                staged = prefs(c.new_options)
                panel.UpdateOptions()
                reopened = FilesAndTrashPanel(c.gui); panels.append(reopened)
                encoded = HydrusSerialisable.CreateFromSerialisableTuple(c.new_options.GetSerialisableTuple())
                out['options'].append(dict(input=values,before=before,staged=staged,saved=prefs(c.new_options),reopened=controls(reopened),round_trip=prefs(encoded)))
            panel.resize(1050,720); panel.show(); QW.QApplication.processEvents()
            assert panel.grab().save(str(HERE/'fixtures/local_transfer_confirmations.png')); panel.hide()
            before = prefs(c.new_options)
            cancelled = FilesAndTrashPanel(c.gui)
            cancelled._confirm_multiple_local_file_services_copy.setChecked(False); cancelled._confirm_multiple_local_file_services_move.setChecked(False)
            cancelled.deleteLater(); out['cancel'] = dict(before=before,after=prefs(c.new_options))
            reset()
            media = [M.MediaSingle(m) for m in fresh()]
            counts = S.GetLocalFileActionServiceKeys(media)
            current = {key:sum(key in m.GetLocationsManager().GetCurrent() for m in media) for key in (source_key,dest_key)}
            menu = QW.QMenu(c.gui)
            Menus.AddLocalFilesMoveAddToMenu(c.gui,menu,current,*counts,True,lambda command:None)
            def labels(menu):
                return [{'label':a.text(),'children':labels(a.menu()) if a.menu() is not None else []} for a in menu.actions()]
            out['menu'] = labels(menu)
            menu.deleteLater()
            for kind, action in [('copy',HC.CONTENT_UPDATE_ADD),('move',HC.CONTENT_UPDATE_MOVE),('merge',HC.CONTENT_UPDATE_MOVE_MERGE)]:
                for confirm in (False,True):
                    for yes in (False,True):
                        reset(); before=state(); asked.clear();jobs.clear();answer[0]=yes
                        for key in KEYS:c.new_options.SetBoolean(key,confirm)
                        media=[M.MediaSingle(m) for m in fresh()]
                        A.MoveOrDuplicateLocalFiles(c.gui,dest_key,action,media,source_key if kind!='copy' else None)
                        out['transfers'].append(dict(kind=kind,confirm=confirm,accepted=yes,before=before,questions=list(asked),jobs=list(jobs),after=state()))
            # Restore a previously deleted destination using its original import timestamp.
            reset();update(dest_key, HC.CONTENT_UPDATE_ADD,(results[0].GetFileInfoManager(),1234567890200));update(dest_key, HC.CONTENT_UPDATE_DELETE,[hashes[0]])
            before=state();asked.clear();jobs.clear();answer[0]=True
            A.MoveOrDuplicateLocalFiles(c.gui,dest_key,HC.CONTENT_UPDATE_MOVE,[M.MediaSingle(c.Read('media_results',[hashes[0]])[0])],source_key)
            out['undelete']=dict(before=before,after=state(),questions=list(asked),jobs=list(jobs))
            return out
        finally:
            c.new_options=original;Q.GetYesNo=old_question;c.CallToThread=old_thread;HydrusTime.GetNowMS=old_now
            for panel in panels:panel.deleteLater()
    return c.CallBlockingToQt(c.gui,drive)

def main():
    import hydrus_driver,record_api
    if len(sys.argv)>1 and sys.argv[1]=='--child':
        Path(sys.argv[2]).write_text(json.dumps(hydrus_driver.run_client(record_api.unpack_fixture('basic'),record)));return
    with tempfile.TemporaryDirectory() as directory:
        path=Path(directory)/'result.json'
        hydrus_driver.run_in_subprocess(str(Path(__file__).resolve()),'--child',str(path));result=json.loads(path.read_text())
    (HERE/'fixtures/local_transfer_confirmations.json').write_text(json.dumps(result,indent=2)+'\n')
    print('wrote local_transfer_confirmations.json')
if __name__=='__main__':main()

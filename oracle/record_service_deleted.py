#!/usr/bin/env python3
"""Record actual physical-storage review's double-confirmed deleted-record clear.

A copied basic DB contains one fixture hash deleted permanently and one kept in
trash. Real UI questions, accepted content writes, domain counts and import hash
statuses are captured; only async scheduling and explicit yes/no decisions are
controlled. No remote services or non-fixture files are modified.
"""
import json
import os
import sys
import tempfile
HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
OUT = os.path.join(HERE, 'fixtures', 'service_deleted.json')

def record(session):
    from qtpy import QtWidgets as QW
    from hydrus.client import ClientConstants as CC
    from hydrus.client.gui import ClientGUIDialogsQuick as Q
    from hydrus.client.gui.services import ClientGUIClientsideServices as S
    from hydrus.client.metadata import ClientContentUpdates as U
    from hydrus.core import HydrusConstants as HC
    c = session.controller
    manifest = json.load(open(os.path.join(HERE, 'fixtures', 'legacy_db', 'basic.manifest.json')))
    hashes = [bytes.fromhex(row['hash']) for row in manifest['files'][:2]]
    c.new_options.SetBoolean('delete_lock_for_archived_files', False)
    def update(key, action, row):
        c.WriteSynchronous('content_updates', U.ContentUpdatePackage.STATICCreateFromContentUpdate(key, U.ContentUpdate(HC.CONTENT_TYPE_FILES, action, row)))
    update(CC.COMBINED_LOCAL_FILE_DOMAINS_SERVICE_KEY, HC.CONTENT_UPDATE_DELETE, hashes)
    update(CC.HYDRUS_LOCAL_FILE_STORAGE_SERVICE_KEY, HC.CONTENT_UPDATE_DELETE, hashes[:1])
    def work():
        old_thread, old_after, old_question, old_write = c.CallToThread, c.CallAfterQtSafe, Q.GetYesNo, c.Write
        c.CallToThread = lambda fn, *a, **kw: fn(*a, **kw)
        c.CallAfterQtSafe = lambda widget, fn, *a: fn(*a)
        c.Write = lambda action, *a, **kw: c.WriteSynchronous(action, *a, **kw)
        storage = c.services_manager.GetService(CC.HYDRUS_LOCAL_FILE_STORAGE_SERVICE_KEY)
        def state():
            domains = []
            for service in c.services_manager.GetServices((HC.LOCAL_FILE_DOMAIN, HC.COMBINED_LOCAL_FILE_DOMAINS, HC.HYDRUS_LOCAL_FILE_STORAGE)):
                info = c.Read('service_info', service.GetServiceKey())
                domains.append(dict(key=service.GetServiceKey().hex(), name=service.GetName(), deleted=info[HC.SERVICE_INFO_NUM_DELETED_FILES]))
            return dict(domains=domains, import_status=[c.Read('hash_status', 'sha256', h).status for h in hashes], trash=sorted(h.hex() for h in c.Read('trash_hashes')))
        events = []
        try:
            for decisions in [[False], [True, False], [True, True]]:
                asked = []
                choices = iter(decisions)
                def question(parent, message, **kw):
                    asked.append(dict(message=message, **kw))
                    return QW.QDialog.DialogCode.Accepted if next(choices) else QW.QDialog.DialogCode.Rejected
                Q.GetYesNo = question
                panel = S.ReviewServiceHydrusLocalFileStorageSubPanel(c.gui, storage)
                before = state()
                panel._ClearDeletedFilesRecord()
                panel.resize(700, 200)
                panel.grab().save(OUT.replace('.json', '.png'))
                events.append(dict(decisions=decisions, asked=asked, before=before, after=state()))
                panel.deleteLater()
            return dict(corpus=[h.hex() for h in hashes], events=events)
        finally:
            c.CallToThread, c.CallAfterQtSafe, Q.GetYesNo, c.Write = old_thread, old_after, old_question, old_write
    return c.CallBlockingToQt(c.gui, work)

def main():
    import hydrus_driver
    if len(sys.argv) > 1 and sys.argv[1] == '--child':
        import record_api
        out = sys.argv[2]
        result = hydrus_driver.run_client(record_api.unpack_fixture('basic'), record)
        with open(out, 'w') as f:
            json.dump(result, f)
        return
    with tempfile.TemporaryDirectory() as folder:
        path = os.path.join(folder, 'result.json')
        hydrus_driver.run_in_subprocess(os.path.abspath(__file__), '--child', path)
        with open(path) as f:
            result = json.load(f)
    with open(OUT, 'w') as f:
        json.dump(result, f, indent=2)
        f.write('\n')
    print('wrote', OUT)
if __name__ == '__main__':
    main()

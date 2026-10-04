#!/usr/bin/env python3
"""Drive actual local trash and all three rating review panels on a copied basic DB.

Only fixture files are modified. Synchronous dispatch replaces the panel's async
Write/CallToThread scheduling so accepted database counts are observed immediately.
Questions, button enablement, selection scopes and reopened panels remain actual
Qt consumers. No remote service is opened. Physical deletion stays within the
freshly unpacked temporary fixture; native parity uses its deferred-delete queue.
"""
import json
import os
import sys
import tempfile
HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
OUT = os.path.join(HERE, 'fixtures', 'service_bulk.json')

def record(session):
    from qtpy import QtWidgets as QW
    from hydrus.client import ClientConstants as CC
    from hydrus.client.gui import ClientGUIDialogsQuick as Q
    from hydrus.client.gui.services import ClientGUIClientsideServices as S
    from hydrus.client.gui.media import ClientGUIMediaSimpleActions as M
    from hydrus.client.metadata import ClientContentUpdates as U
    from hydrus.core import HydrusConstants as HC
    c = session.controller
    manifest = json.load(open(os.path.join(HERE, 'fixtures', 'legacy_db', 'basic.manifest.json')))
    hashes = [bytes.fromhex(row['hash']) for row in manifest['files'][:4]]
    unknown = bytes([219]) * 32
    ratings = [c.services_manager.GetService(s.GetServiceKey()) for s in c.services_manager.GetServices(HC.RATINGS_SERVICES)]
    c.new_options.SetBoolean('delete_lock_for_archived_files', False)
    def update(key, kind, action, row):
        c.WriteSynchronous('content_updates', U.ContentUpdatePackage.STATICCreateFromContentUpdate(key, U.ContentUpdate(kind, action, row)))
    def work():
        old_thread, old_after, old_question, old_write = c.CallToThread, c.CallAfterQtSafe, Q.GetYesNo, c.Write
        c.CallToThread = lambda fn, *a, **kw: fn(*a, **kw)
        c.CallAfterQtSafe = lambda widget, fn, *a: fn(*a)
        c.Write = lambda action, *a, **kw: c.WriteSynchronous(action, *a, **kw)
        events = []
        asked = []
        answer = False
        def question(parent, message, **kw):
            asked.append(dict(message=message, **kw))
            return QW.QDialog.DialogCode.Accepted if answer else QW.QDialog.DialogCode.Rejected
        Q.GetYesNo = question
        trash = c.services_manager.GetService(CC.TRASH_SERVICE_KEY)
        def trash_state():
            panel = S.ReviewServiceTrashSubPanel(c.gui, trash)
            result = dict(hashes=sorted(h.hex() for h in c.Read('trash_hashes')), clear_enabled=panel._clear_trash.isEnabled(), undelete_enabled=panel._undelete_all.isEnabled())
            panel.deleteLater()
            return result
        try:
            M.UndeleteFiles(c.Read('trash_hashes'))
            update(CC.COMBINED_LOCAL_FILE_DOMAINS_SERVICE_KEY, HC.CONTENT_TYPE_FILES, HC.CONTENT_UPDATE_DELETE, hashes[:2])
            for action in ['undelete', 'clear']:
                if action == 'clear':
                    update(CC.COMBINED_LOCAL_FILE_DOMAINS_SERVICE_KEY, HC.CONTENT_TYPE_FILES, HC.CONTENT_UPDATE_DELETE, hashes[:2])
                for accepted in [False, True]:
                    answer = accepted
                    asked.clear()
                    panel = S.ReviewServiceTrashSubPanel(c.gui, trash)
                    before = trash_state()
                    if action == 'clear' and accepted:
                        panel.resize(700, 240)
                        panel.grab().save(OUT.replace('.json', '-trash.png'))
                    (panel._UndeleteAll if action == 'undelete' else panel._ClearTrash)()
                    events.append(dict(kind='trash', action=action, accepted=accepted, asked=list(asked), before=before, after=trash_state()))
                    panel.deleteLater()
            update(CC.COMBINED_LOCAL_FILE_DOMAINS_SERVICE_KEY, HC.CONTENT_TYPE_FILES, HC.CONTENT_UPDATE_DELETE, [hashes[2]])
            corpus = [hashes[3], hashes[2], hashes[0], unknown]
            def seed(service):
                key = service.GetServiceKey()
                update(key, HC.CONTENT_TYPE_RATINGS, HC.CONTENT_UPDATE_ADVANCED, 'delete_for_all_files')
                update(key, HC.CONTENT_TYPE_RATINGS, HC.CONTENT_UPDATE_ADD, (7 if service.GetServiceType() == HC.LOCAL_RATING_INCDEC else 1.0, corpus))
            for service in ratings:
                seed(service)
            def rating_state(service):
                panel = S.ReviewServiceRatingSubPanel(c.gui, service)
                values = [media.GetRatingsManager().GetRating(service.GetServiceKey()) for media in c.Read('media_results', corpus)]
                result = dict(label=panel._rating_info_st.text(), cached_values=values)
                panel.deleteLater()
                return result
            for service in ratings:
                for action, description, label in [('delete_for_deleted_files', 'deleted files', 'for deleted files'), ('delete_for_non_local_files', 'non-local files', 'for all non-local files'), ('delete_for_all_files', 'ALL FILES', 'for all files')]:
                    seed(service)
                    for accepted in [False, True]:
                        answer = accepted
                        asked.clear()
                        before = rating_state(service)
                        panel = S.ReviewServiceRatingSubPanel(c.gui, service)
                        if service.GetName() == 'counter' and accepted and action == 'delete_for_non_local_files':
                            panel.resize(700, 240)
                            panel.grab().save(OUT.replace('.json', '-rating.png'))
                        panel._ClearRatings(action, description)
                        events.append(dict(kind='rating', service=service.GetName(), service_type=service.GetServiceType(), action=action, menu_label=label, accepted=accepted, asked=list(asked), before=before, after=rating_state(service)))
                        panel.deleteLater()
            return dict(trash_seed=[h.hex() for h in hashes[:2]], rating_corpus=[h.hex() for h in corpus], events=events)
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

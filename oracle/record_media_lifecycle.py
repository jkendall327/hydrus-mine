#!/usr/bin/env python3
"""Record the reference's thumbnail panel archiving, inboxing, deleting, undeleting and
physically deleting files, with the real database writes.

On the basic fixture, the real media results panel (a page of its first 40 files) has its
`_Archive`, `_Inbox`, `_Delete` and `_Undelete` run on chosen files, as the menu entries
call them (`_Delete` with the domain the entry names). The questions asked are recorded
(yes/no, the delete files dialog's description, the undelete chooser), answered by script;
nothing about the writes is intercepted. After each step the files' state is read back from
the database: in the inbox, current and deleted local domains (and the trash), and the reason
the deletion kept.

Usage: QT_QPA_PLATFORM=offscreen ~/pyenv/bin/python oracle/record_media_lifecycle.py
       (writes fixtures/media_lifecycle.json)
"""
import json
import os
import sys
import tempfile
import time
HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)


def record(session):
    c = session.controller

    def build():
        from hydrus.client import ClientConstants as CC, ClientLocation
        results = c.Read('media_results_from_ids', list(range(1, 41)))
        hashes = [m.GetHash() for m in results]
        page = c.gui._notebook.NewPageQuery(ClientLocation.LocationContext.STATICCreateSimple(CC.LOCAL_FILE_SERVICE_KEY), page_name='lifecycle', initial_hashes=hashes)
        return page, results
    page, results = c.CallBlockingToQt(c.gui, build)
    for _ in range(400):
        if c.CallBlockingToQt(c.gui, lambda: page._initialised): break
        time.sleep(.025)
    else: raise RuntimeError('real thumbnail owner did not initialize')

    def drive():
        from qtpy import QtWidgets as QW
        from hydrus.core import HydrusConstants as HC, HydrusExceptions
        from hydrus.client import ClientConstants as CC
        from hydrus.client.gui import ClientGUIDialogsQuick as Q
        from hydrus.client.gui import ClientGUITopLevelWindowsPanels as T
        owner = page.GetMediaResultsPanel()
        names = {s.GetServiceKey(): s.GetName() for s in c.services_manager.GetServices((HC.LOCAL_FILE_DOMAIN,))}
        names[CC.TRASH_SERVICE_KEY] = 'trash'
        log = []
        script = dict(yes=[], delete=[], choose=[])
        old = (Q.GetYesNo, T.DialogEdit.exec, Q.SelectFromListButtons, HC.options['confirm_archive'], HC.options['confirm_trash'])
        HC.options['confirm_archive'] = True
        HC.options['confirm_trash'] = True

        def yesno(parent, message, **kwargs):
            answer = script['yes'].pop(0)
            log.append(['question', message, answer])
            return QW.QDialog.DialogCode.Accepted if answer else QW.QDialog.DialogCode.Rejected
        def execute(dlg):
            panel = dlg._panel
            answer = script['delete'].pop(0)
            log.append(['delete files dialog', panel._simple_description.text(), answer])
            return QW.QDialog.DialogCode.Accepted if answer else QW.QDialog.DialogCode.Rejected
        def choose(win, title, choice_tuples, *a, **k):
            answer = script['choose'].pop(0)
            log.append(['chooser', title, [t[0] for t in choice_tuples], answer])
            if answer is None:
                raise HydrusExceptions.CancelledException('Dialog cancelled.')
            for (label, value, tooltip) in choice_tuples:
                if label == answer:
                    return value
            raise Exception('no choice ' + answer)
        Q.GetYesNo, T.DialogEdit.exec, Q.SelectFromListButtons = yesno, execute, choose

        def state(h):
            r = c.Read('media_results', [h])[0]
            lm = r.GetLocationsManager()
            return dict(hash=h.hex(), inbox=lm.GetInbox(),
                        current=sorted(names[k] for k in lm.GetCurrent() if k in names),
                        deleted=sorted(names[k] for k in lm.GetDeleted() if k in names),
                        reason=lm.GetLocalFileDeletionReason())

        def select(hashes):
            media = [m for m in owner.GetSortedMedia() if m.GetHash() in hashes]
            owner._HitMedia(None, False, False)
            for i, m in enumerate(media):
                owner._HitMedia(m, i > 0, False)

        def settle():
            time.sleep(0.6)
            QW.QApplication.processEvents()

        steps = []

        def step(name, hashes, action, yes=(), delete=(), choose_=()):
            log.clear()
            script['yes'][:] = list(yes); script['delete'][:] = list(delete); script['choose'][:] = list(choose_)
            select(set(hashes))
            action()
            QW.QApplication.processEvents()
            settle()
            steps.append(dict(step=name, files=[h.hex() for h in hashes], dialogs=list(log), states=[state(h) for h in hashes]))

        try:
            flat = owner.GetSortedMedia()
            inbox = next(m for m in flat if m.HasInbox() and len(m.GetLocationsManager().GetCurrent() & set(names)) == 1)
            archived = next(m for m in flat if not m.HasInbox())
            one_domain = [m for m in flat if len([k for k in m.GetLocationsManager().GetCurrent() if k in names]) == 1 and m.HasInbox()]
            two_domain = [m for m in flat if len([k for k in m.GetLocationsManager().GetCurrent() if k in names and k != CC.TRASH_SERVICE_KEY]) == 2]
            a, b = inbox.GetHash(), archived.GetHash()
            single = [m for m in flat if len([k for k in m.GetLocationsManager().GetCurrent() if k in names]) == 1 and m.GetHash() not in (a, b)][0]
            double = two_domain[0]
            s, d = single.GetHash(), double.GetHash()
            local = lambda m: [k for k in m.GetLocationsManager().GetCurrent() if k in names]
            domain_of = lambda m: local(m)[0]
            step('archive an inbox file and an archived file', [a, b], owner._Archive)
            step('re-inbox both, declined', [a, b], owner._Inbox, yes=[False])
            step('re-inbox both', [a, b], owner._Inbox, yes=[True])
            step('re-inbox one', [a], owner._Inbox)
            step('delete a file from its only domain, declined', [s], lambda: owner._Delete(domain_of(single)), delete=[False])
            step('delete a file from its only domain', [s], lambda: owner._Delete(domain_of(single)), delete=[True])
            step('undelete it, declined', [s], owner._Undelete, yes=[False])
            step('undelete it, accepted', [s], owner._Undelete, yes=[True])
            first, second = sorted(local(double))
            step('delete a two-domain file from one domain', [d], lambda: owner._Delete(first), delete=[True])
            step('delete it from the other', [d], lambda: owner._Delete(second), delete=[True])
            step('undelete, chooser cancelled', [d], owner._Undelete, choose_=[None])
            step('undelete, back to one domain', [d], owner._Undelete, choose_=[names[first]])
            step('delete it from the domain it is back in', [d], lambda: owner._Delete(first), delete=[True])
            step('undelete, back to all the above', [d], owner._Undelete, choose_=['all the above'])
            step('delete the single file again', [s], lambda: owner._Delete(domain_of(single)), delete=[True])
            step('delete it physically, declined', [s], lambda: owner._Delete(CC.HYDRUS_LOCAL_FILE_STORAGE_SERVICE_KEY), delete=[False])
            step('delete it physically', [s], lambda: owner._Delete(CC.HYDRUS_LOCAL_FILE_STORAGE_SERVICE_KEY), delete=[True])
            # "delete trash physically now": with a trashed file and one that is not, only the
            # trashed one is deleted for good (`only_those_in_file_service_key`)
            spare = [m for m in flat if m.GetHash() not in (a, b, s, d) and len([k for k in local(m)]) == 1 and CC.TRASH_SERVICE_KEY not in m.GetLocationsManager().GetCurrent()][0]
            t, u = spare.GetHash(), archived.GetHash()
            step('delete another file to the trash', [t], lambda: owner._Delete(domain_of(spare)), delete=[True])
            step('delete trash physically now, with a mixed selection', [t, a], lambda: owner._Delete(CC.HYDRUS_LOCAL_FILE_STORAGE_SERVICE_KEY, only_those_in_file_service_key=CC.TRASH_SERVICE_KEY), delete=[True])
        finally:
            Q.GetYesNo, T.DialogEdit.exec, Q.SelectFromListButtons, HC.options['confirm_archive'], HC.options['confirm_trash'] = old
        return dict(names={k.hex(): v for k, v in names.items()}, steps=steps)
    return c.CallBlockingToQt(c.gui, drive)


def main():
    import hydrus_driver
    if len(sys.argv) > 1 and sys.argv[1] == '--child':
        import record_api
        output = sys.argv[2]
        value = hydrus_driver.run_client(record_api.unpack_fixture('basic'), record)
        with open(output, 'w') as f:
            json.dump(value, f)
        return
    with tempfile.TemporaryDirectory() as tmp:
        path = os.path.join(tmp, 'result.json')
        hydrus_driver.run_in_subprocess(os.path.abspath(__file__), '--child', path)
        with open(path) as f:
            value = json.load(f)
    with open(os.path.join(HERE, 'fixtures', 'media_lifecycle.json'), 'w') as f:
        json.dump(value, f, indent=1, ensure_ascii=False)
        f.write('\n')
    print('recorded the media lifecycle')


if __name__ == '__main__':
    main()

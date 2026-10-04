#!/usr/bin/env python3
"""Record real FilesAndTrash options, thumbnail archive/inbox questions and
EditDeleteFilesPanel auto-resolution on authored selections from the basic fixture.
Writes are intercepted; panel UpdateOptions and native action construction run.
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
        page = c.gui._notebook.NewPageQuery(ClientLocation.LocationContext.STATICCreateSimple(CC.LOCAL_FILE_SERVICE_KEY), page_name='synthetic confirmation examples', initial_hashes=hashes)
        return page, results
    page, results = c.CallBlockingToQt(c.gui, build)
    for _ in range(400):
        if c.CallBlockingToQt(c.gui, lambda: page._initialised): break
        time.sleep(.025)
    else: raise RuntimeError('real thumbnail owner did not initialize')
    def drive():
        from qtpy import QtWidgets as QW
        from hydrus.core import HydrusConstants as HC
        from hydrus.client import ClientConstants as CC
        from hydrus.client.gui import ClientGUIDialogsQuick as Q
        from hydrus.client.gui.panels.options import FilesAndTrashPanel as O
        from hydrus.client.gui.panels import ClientGUIScrolledPanelsEdit as E
        from hydrus.client.media import ClientMediaSingle as M
        old = {k: HC.options[k] for k in ('confirm_archive', 'confirm_trash')}
        old_write, old_yes = c.Write, Q.GetYesNo
        owner = page.GetMediaResultsPanel()
        out = {'initial': old.copy(), 'options': [], 'archive': [], 'deletion': []}
        panels = []
        questions, writes = [], []
        accepted = [False]
        def answer(parent, message, **kwargs):
            questions.append(message)
            return QW.QDialog.DialogCode.Accepted if accepted[0] else QW.QDialog.DialogCode.Rejected
        def write(name, *args, **kwargs):
            writes.append(name)
        c.Write, Q.GetYesNo = write, answer
        try:
            for value in (False, True):
                panel = O.FilesAndTrashPanel(c.gui); panels.append(panel)
                panel._confirm_archive.setChecked(value); panel._confirm_trash.setChecked(value)
                out['options'].append({'entered': value, 'staged': [HC.options['confirm_archive'],HC.options['confirm_trash']]})
                panel.UpdateOptions()
                reopened = O.FilesAndTrashPanel(c.gui); panels.append(reopened)
                out['options'][-1]['reopened'] = [reopened._confirm_archive.isChecked(), reopened._confirm_trash.isChecked()]
                for action, inbox in (('archive', True), ('inbox', False)):
                    choices = [m for m in owner.GetSortedMedia() if m.HasInbox() == inbox][:2]
                    assert len(choices) == 2
                    for count in (1, 2):
                        for yes in (False, True):
                            owner._HitMedia(None, False, False)
                            for i, media in enumerate(choices[:count]): owner._HitMedia(media, i > 0, False)
                            accepted[0] = yes; questions.clear(); writes.clear()
                            (owner._Archive if inbox else owner._Inbox)()
                            out['archive'].append({'confirm': value, 'action': action, 'count': count, 'accepted': yes, 'questions': list(questions), 'writes': list(writes)})
                by_domains = {}
                for result in results:
                    domains = [key for key in result.GetLocationsManager().GetCurrent() if c.services_manager.GetServiceType(key) == HC.LOCAL_FILE_DOMAIN]
                    if domains: by_domains.setdefault(len(domains), result)
                for domains, result in sorted(by_domains.items()):
                    deletion = E.EditDeleteFilesPanel(c.gui, [M.MediaSingle(result)], 'Deleted from Preview or Media Viewer.')
                    panels.append(deletion)
                    out['deletion'].append({'confirm': value, 'domains': domains, 'resolved': deletion.QuestionIsAlreadyResolved(), 'description': deletion._simple_description.text()})
            panel.resize(1150,850); panel.show(); QW.QApplication.processEvents()
            panel.grab().save(os.path.join(HERE,'fixtures/files_trash_options.png'))
            return out
        finally:
            HC.options.update(old); c.Write, Q.GetYesNo = old_write, old_yes
            for panel in panels: panel.deleteLater()
    return c.CallBlockingToQt(c.gui, drive)


def main():
    import hydrus_driver
    if len(sys.argv) > 1 and sys.argv[1] == '--child':
        output = sys.argv[2]
        import record_api
        result = hydrus_driver.run_client(record_api.unpack_fixture('basic'), record)
        with open(output,'w') as f: json.dump(result,f,ensure_ascii=False,indent=2)
        return
    with tempfile.TemporaryDirectory() as work:
        output = os.path.join(work,'result.json')
        hydrus_driver.run_in_subprocess(os.path.abspath(__file__),'--child',output)
        with open(output) as f: result=json.load(f)
        with open(os.path.join(HERE,'fixtures/files_trash.json'),'w') as f: json.dump(result,f,ensure_ascii=False,indent=2)

if __name__ == '__main__': main()

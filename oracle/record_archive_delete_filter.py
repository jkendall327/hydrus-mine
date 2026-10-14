#!/usr/bin/env python3
"""Record the archive/delete filter canvas (`CanvasMediaListFilterArchiveDelete`)
on five files: keep, delete, skip and back (and back on the first file, and
a decision on the last file closing the window), each step's current file
and kept/deleted/skipped sets; then the finish question for several mixes
of decisions on a page of all local files and a page of "my files", answered
by each button (commit, forget with yes and no, back to filtering): the
question's labels and buttons, what the canvas returns, which files are
kept, deleted and skipped afterwards, the commit it starts, and the files it
tells the page to remove.

Usage: QT_QPA_PLATFORM=offscreen ~/pyenv/bin/python oracle/record_archive_delete_filter.py
       (writes fixtures/archive_delete_filter.json)
"""
import json
import os
import sys
import tempfile
from pathlib import Path
HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))


def record(session):
    from qtpy import QtWidgets as QW
    from hydrus.client import ClientConstants as CC, ClientLocation
    from hydrus.client.gui import ClientGUIDialogsQuick, ClientGUITopLevelWindowsPanels
    from hydrus.client.gui.canvas import ClientGUICanvas as Canvas, ClientGUICanvasFrame
    from hydrus.client.gui.panels import ClientGUIScrolledPanelsCommitFiltering as Finish
    c = session.controller
    manifest = json.loads((HERE / 'fixtures/legacy_db/basic.manifest.json').read_text())
    # (still images, so that no player starts)
    hashes = [bytes.fromhex(f['hash']) for f in manifest['files'] if f['name'].startswith('jpeg_0')][:5]

    def drive():
        results = c.Read('media_results', hashes)
        frames = []
        questions, commits, removed = [], [], []
        choice = {'button': 'commit', 'forget_yes': True, 'pick': 0}
        old_finish, old_thread, old_yesno = Finish.GetFinishArchiveDeleteFilteringAnswer, c.CallToThread, ClientGUIDialogsQuick.GetYesNo

        def finish(win, kept, options):
            dlg = ClientGUITopLevelWindowsPanels.DialogCustomButtonQuestion(win, 'filtering done?')
            panel = Finish.QuestionArchiveDeleteFinishFilteringPanel(dlg, kept, options)
            dlg.SetPanel(panel)
            dlg.show()
            QW.QApplication.processEvents()
            buttons = [b for b in panel.findChildren(QW.QPushButton)]
            question = dict(kept=kept, options=[dict(keys=sorted(k.hex() for k in loc.current_service_keys), label=label) for loc, label in options],
                labels=[x.text() for x in panel.findChildren(QW.QLabel) if x.text()],
                buttons=[b.text() for b in buttons], forget_questions=[])
            if choice['button'] == 'commit':
                commits_ = [b for b in buttons if b.text() == 'commit']
                commits_[min(choice['pick'], len(commits_) - 1)].click()
            elif choice['button'] == 'forget':
                ClientGUIDialogsQuick.GetYesNo = lambda w, m, **k: (question['forget_questions'].append(m), QW.QDialog.DialogCode.Accepted if choice['forget_yes'] else QW.QDialog.DialogCode.Rejected)[1]
                [b for b in buttons if b.text() == 'forget'][0].click()
                ClientGUIDialogsQuick.GetYesNo = old_yesno
            else:
                [b for b in buttons if b.text() == 'back to filtering'][0].click()
            questions.append(question)
            out = (dlg.result() if dlg.result() else QW.QDialog.DialogCode.Rejected, panel.GetLocationContext(), dlg.WasCancelled())
            if choice['button'] == 'commit':
                out = (QW.QDialog.DialogCode.Accepted, panel.GetLocationContext(), False)
            dlg.hide()
            dlg.deleteLater()
            return out

        def thread(fn, *args, **kwargs):
            if fn is Canvas.CommitArchiveDelete:
                commits.append(dict(keys=sorted(k.hex() for k in args[0].current_service_keys), kept=[m.GetHash().hex() for m in args[1]], deleted=[m.GetHash().hex() for m in args[2]]))
                return None
            return old_thread(fn, *args, **kwargs)
        Finish.GetFinishArchiveDeleteFilteringAnswer = finish
        c.CallToThread = thread

        def copies(originals):
            # (the hover's information lines need these times)
            out = []
            for original in originals:
                result = original.Duplicate()
                manager = result.GetLocationsManager()
                times = result.GetTimesManager()
                times.SetFileModifiedTimestampMS(1234567890000)
                for key in manager.GetCurrent():
                    times.SetImportedTimestampMS(key, 1234567890000)
                out.append(result)
            return out

        def make(keys):
            frame = ClientGUICanvasFrame.CanvasFrame(c.gui)
            frames.append(frame)
            canvas = Canvas.CanvasMediaListFilterArchiveDelete(frame, os.urandom(32), ClientLocation.LocationContext(current_service_keys=set(keys), deleted_service_keys=set()), copies(results))
            canvas.userRemovedMedia.connect(lambda hs: removed.append(sorted(h.hex() for h in hs)))
            frame.SetCanvas(canvas)
            frame.showNormal()
            frame.resize(1000, 700)
            QW.QApplication.processEvents()
            canvas.SetMedia(canvas._media_list.GetFirst())
            QW.QApplication.processEvents()
            return frame, canvas

        def names(canvas, group):
            order = [m.GetHash().hex() for m in canvas._media_list.GetSortedMedia()]
            return sorted(order.index(m.GetHash().hex()) for m in group)

        def is_visible(frame):
            try:
                return bool(frame.isVisible())
            except RuntimeError:
                return False  # (closed and deleted)

        def is_gone(frame):
            try:
                frame.isVisible()
                return False
            except RuntimeError:
                return True

        def state(canvas, frame, do):
            order = [m.GetHash().hex() for m in canvas._media_list.GetSortedMedia()]
            cur = canvas._current_media
            return dict(do=do, current=order.index(cur.GetHash().hex()) if cur is not None else None,
                kept=names(canvas, canvas._kept), deleted=names(canvas, canvas._deleted), skipped=names(canvas, canvas._skipped),
                visible=is_visible(frame))
        try:
            out = {'order': None, 'decisions': [], 'closes': []}
            frame, canvas = make([CC.COMBINED_LOCAL_FILE_DOMAINS_SERVICE_KEY])
            out['order'] = [m.GetHash().hex() for m in canvas._media_list.GetSortedMedia()]
            choice['button'] = 'back'
            for do in ('back', 'keep', 'delete', 'back', 'delete', 'skip', 'back', 'back', 'back', 'back', 'keep', 'keep', 'delete', 'skip', 'keep'):
                getattr(canvas, '_' + do.capitalize())()
                QW.QApplication.processEvents()
                out['decisions'].append(state(canvas, frame, do))
                if not is_visible(frame):
                    break
            try:
                canvas.ClearMedia()
                frame.deleteLater()
            except RuntimeError:
                pass
            # the finish question, after a run of decisions (the last file's
            # decision closes the window itself, asking as it does)
            plans = {
                'keep one, delete one, skip one': ['keep', 'delete', 'skip'],
                'keep two': ['keep', 'keep'],
                'delete two': ['delete', 'delete'],
                'keep two, delete two, then the last file': ['keep', 'keep', 'delete', 'delete', 'delete'],
                'keep all five': ['keep', 'keep', 'keep', 'keep', 'keep'],
                'a decision undone': ['keep', 'delete', 'back', 'back', 'keep'],
                'only skips': ['skip', 'skip'],
                'no decisions': [],
            }
            c.new_options.SetBoolean('archive_delete_commit_panel_delays_multiple_delete_choices', False)
            for page, keys in (('all local files', [CC.COMBINED_LOCAL_FILE_DOMAINS_SERVICE_KEY]), ('my files', [CC.LOCAL_FILE_SERVICE_KEY])):
                for name, ops in plans.items():
                    for button, forget_yes, pick in (('commit', None, 0), ('commit', None, 1), ('forget', True, 0), ('back', None, 0)):
                        frame, canvas = make(keys)
                        choice.update(button=button, forget_yes=bool(forget_yes), pick=pick)
                        questions.clear()
                        commits.clear()
                        removed.clear()
                        for do in ops:
                            getattr(canvas, '_' + do.capitalize())()
                            QW.QApplication.processEvents()
                        asked_by_the_window = len(questions) > 0
                        ok = None if asked_by_the_window else bool(canvas.UserOKToClose())
                        out['closes'].append(dict(page=page, plan=name, ops=ops, button=button, forget_yes=forget_yes, pick=pick,
                            asked_by_the_window=asked_by_the_window, returned=ok,
                            questions=list(questions), commits=list(commits), removed=list(removed),
                            after=state(canvas, frame, 'close')))
                        again = None
                        if button == 'back' and not is_gone(frame):
                            # asked again: the decisions that are left
                            questions.clear()
                            canvas.UserOKToClose()
                            again = list(questions)
                        out['closes'][-1]['again'] = again
                        try:
                            canvas._kept = set()
                            canvas._deleted = set()
                            canvas.ClearMedia()
                            frame.deleteLater()
                        except RuntimeError:
                            pass  # (closed and deleted already)
        finally:
            c.new_options.SetBoolean('archive_delete_commit_panel_delays_multiple_delete_choices', True)
            Finish.GetFinishArchiveDeleteFilteringAnswer = old_finish
            c.CallToThread = old_thread
            ClientGUIDialogsQuick.GetYesNo = old_yesno
        return out
    return c.CallBlockingToQt(c.gui, drive)


def main():
    import hydrus_driver, record_api
    if len(sys.argv) > 1 and sys.argv[1] == '--child':
        Path(sys.argv[2]).write_text(json.dumps(hydrus_driver.run_client(record_api.unpack_fixture('basic'), record)))
        return
    with tempfile.TemporaryDirectory() as work:
        out = Path(work) / 'filter.json'
        hydrus_driver.run_in_subprocess(str(Path(__file__).resolve()), '--child', str(out))
        result = json.loads(out.read_text())
    (HERE / 'fixtures' / 'archive_delete_filter.json').write_text(json.dumps(result, indent=2) + '\n')


if __name__ == '__main__':
    main()

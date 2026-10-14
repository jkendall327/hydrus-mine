#!/usr/bin/env python3
"""Record the reference's file maintenance review panel (`ReviewFileMaintenance`).

The real panel is made on the basic fixture and its "add new work" tab driven as a user
would: the explanation and labels, each job type's description, running the default
search, the easy-select buttons, adding a job and the large-job question. Dialogs are
replaced by scripted answers; after every step the status labels, the add button, the
messages and questions shown and the scheduled counts are recorded.
"""
import json
import os
import sys
import tempfile
HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)


def record(session):
    c = session.controller

    def work():
        from qtpy import QtWidgets as QW, QtTest as QT
        from hydrus.client import ClientConstants as CC
        from hydrus.client.gui import ClientGUIDialogsMessage as M, ClientGUIDialogsQuick as Q
        from hydrus.client.gui.panels import ClientGUIScrolledPanelsReview as R
        from hydrus.client.files import ClientFilesMaintenance as F
        log = []
        answers = []
        old = (M.ShowInformation, Q.GetYesNo)

        def info(parent, text, *a, **k): log.append(['information', text])
        def yesno(parent, text, yes_label='yes', no_label='no', *a, **k):
            log.append(['question', text, yes_label, no_label])
            return QW.QDialog.DialogCode.Accepted if answers.pop(0) else QW.QDialog.DialogCode.Rejected
        M.ShowInformation, Q.GetYesNo = info, yesno
        panel = R.ReviewFileMaintenance(c.gui, c)
        steps = []

        def settle(until=None):
            for _ in range(100):
                QT.QTest.qWait(30)
                if until is None or until():
                    break
            QT.QTest.qWait(60)

        def snap(step, *, jobs=False, rows=False):
            for _ in range(40):
                QT.QTest.qWait(50)
            for _ in range(200 if rows else 0):
                if len(panel._jobs_listctrl.GetData()) > 0:
                    break
                QT.QTest.qWait(50)
            out = dict(step=step, selected=panel._selected_files_st.text(), search=panel._run_search_st.text(), can_add=panel._add_new_job.isEnabled(),
                       run_search_enabled=panel._run_search.isEnabled(), job=panel._action_selector.GetValue(), dialogs=list(log))
            if jobs or True:
                out['rows'] = [list(panel._ConvertJobTypeToDisplayTuple(j)) for j in panel._jobs_listctrl.GetData()]
            steps.append(out)
            log.clear()

        try:
            settle()
            texts = sorted({l.text() for l in panel.findChildren(QW.QLabel) if l.text()})
            buttons = sorted({b.text() for b in panel.findChildren(QW.QPushButton) if b.text()})
            labels = [(F.regen_file_enum_to_str_lookup[j]) for j in F.ALL_REGEN_JOBS_IN_HUMAN_ORDER]
            steps.append(dict(step='opened', texts=texts, buttons=buttons, job_labels=labels, tabs=[panel._notebook.tabText(i) for i in range(panel._notebook.count())]))
            snap('scheduled work list at the start')
            descriptions = []
            for job_type in F.ALL_REGEN_JOBS_IN_HUMAN_ORDER:
                panel._action_selector.SetValue(job_type)
                panel._SeeDescription()
                descriptions.append(dict(job=F.regen_file_enum_to_str_lookup[job_type], dialogs=list(log)))
                log.clear()
            steps[0]['descriptions'] = descriptions
            panel._action_selector.SetValue(F.ALL_REGEN_JOBS_IN_HUMAN_ORDER[0])
            panel._SetHashIds(set())
            snap('no files')
            panel._RunSearch()
            settle(lambda: 'files found' in panel._run_search_st.text())
            snap('run the default search')
            panel._SelectAllMediaFiles()
            settle(lambda: panel._selected_files_st.text() != 'no files selected yet' and panel._select_all_media_files.isEnabled())
            snap('all media files')
            panel._SelectRepoUpdateFiles()
            settle(lambda: panel._select_repo_files.isEnabled())
            snap('all repository update files')
            panel._SelectAllMediaFiles()
            settle(lambda: panel._select_all_media_files.isEnabled())
            panel._action_selector.SetValue(F.REGENERATE_FILE_DATA_JOB_FILE_METADATA)
            panel._AddJob()
            settle(lambda: len(log) > 0 and panel._add_new_job.isEnabled())
            settle()
            snap('add the file metadata job on all media', rows=True)
            panel._SetHashIds(set(range(1, 1002)))
            answers[:] = [False]
            panel._AddJob()
            settle()
            snap('add a job on 1001 files, forget it', rows=True)
            panel._SetHashIds(set(range(1, 1001)))
            answers[:] = []
            panel._action_selector.SetValue(F.REGENERATE_FILE_DATA_JOB_FORCE_THUMBNAIL)
            panel._SetHashIds(set())
            snap('no files again', rows=True)
            # a predicate in the search box (the autocomplete cannot be typed into headlessly, so the
            # search context is set as choosing the predicate would): the narrowed searches
            from hydrus.client.search import ClientSearchPredicate as P
            previous = panel._selected_files_st.text()
            for name, predicate_type in (('system:archive', P.PREDICATE_TYPE_SYSTEM_ARCHIVE), ('system:inbox', P.PREDICATE_TYPE_SYSTEM_INBOX)):
                context = panel._tag_autocomplete.GetFileSearchContext().Duplicate()
                context.SetPredicates([P.Predicate(predicate_type)])
                panel._tag_autocomplete.SetFileSearchContext(context)
                panel._RunSearch()
                for _ in range(400):
                    QT.QTest.qWait(50)
                    if 'files found' in panel._run_search_st.text() and panel._run_search.isEnabled() and panel._selected_files_st.text() != previous:
                        break
                previous = panel._selected_files_st.text()
                snap('run the search ' + name)
        finally:
            M.ShowInformation, Q.GetYesNo = old
        return dict(steps=steps)
    return c.CallBlockingToQt(c.gui, work)


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
    with open(os.path.join(HERE, 'fixtures', 'file_maintenance_new.json'), 'w') as f:
        json.dump(value, f, indent=1, ensure_ascii=False)
        f.write('\n')
    print('recorded the file maintenance panel')


if __name__ == '__main__':
    main()

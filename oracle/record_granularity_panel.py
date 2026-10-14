#!/usr/bin/env python3
"""Record the reference's granularity window (`ReviewGranularityPanel`).

The real panel is made on an empty client with one file in its granularity 2 layout, and
the client and offline-folder migrations are run with the pickers and questions
replaced by scripted answers; the jobs themselves are the reference's own. After each
step the messages and questions shown, the panel's label and visible buttons, and where the
file is are recorded, with the directories made written as `<name>`.
"""
import json
import os
import re
import sys
import tempfile
HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)


def record(session):
    c = session.controller

    def work():
        from qtpy import QtWidgets as QW, QtTest as QT
        from hydrus.core import HydrusExceptions
        from hydrus.client.gui import ClientGUIDialogsMessage as M, ClientGUIDialogsQuick as Q
        from hydrus.client.gui import ClientGUITopLevelWindowsPanels as T
        from hydrus.client.gui.panels import ClientGUIFilesPhysicalStoragePanels as P
        root = tempfile.mkdtemp()
        names = {}
        default = next(iter(c.Read('ideal_client_files_locations')[0])).path
        names[default] = 'default'

        def folder(name, files):
            path = os.path.join(root, name)
            os.makedirs(path)
            for f in files:
                os.makedirs(os.path.dirname(os.path.join(path, f)), exist_ok=True)
                open(os.path.join(path, f), 'w').write('x')
            names[path] = name
            return path

        def norm(text):
            for path, name in sorted(names.items(), key=lambda kv: -len(kv[0])):
                text = text.replace(path, '<' + name + '>')
            return re.sub(r'in (\d+(\.\d+)? )?(micro|milli)?seconds?\.', 'in <time>.', text)

        log = []
        script = {'yesno': [], 'pick': []}
        old = (M.ShowInformation, M.ShowCritical, Q.GetYesNo, Q.PickDirectory, T.DialogNullipotent.exec)

        def info(parent, text, *a, **k): log.append(['information', norm(text)])
        def critical(parent, title, text, *a, **k): log.append(['critical', title, norm(text)])
        def yesno(parent, text, title='Are you sure?', yes_label='yes', no_label='no', *a, **k):
            log.append(['question', title, norm(text), yes_label, no_label])
            return QW.QDialog.DialogCode.Accepted if script['yesno'].pop(0) else QW.QDialog.DialogCode.Rejected
        def pick(parent, title, *a, **k):
            log.append(['pick directory', title])
            choice = script['pick'].pop(0)
            if choice is None:
                raise HydrusExceptions.CancelledException()
            return choice
        def execute(dialog):
            # the job's own dialog: wait for it to say how it went
            before = len(log)
            for _ in range(12000):
                QT.QTest.qWait(50)
                if len(log) > before:
                    break
            return QW.QDialog.DialogCode.Accepted
        M.ShowInformation, M.ShowCritical, Q.GetYesNo, Q.PickDirectory, T.DialogNullipotent.exec = info, critical, yesno, pick, execute

        def tree(base):
            out = []
            for dirpath, _, files in os.walk(base):
                for f in files:
                    out.append(norm(os.path.relpath(os.path.join(dirpath, f), base)))
            return sorted(out)

        steps = []
        panel_holder = {}

        def labels(panel):
            return dict(granularity=panel._granularity_st.text(), to3=not panel._migrate_2to3_button.isHidden(), to2=not panel._migrate_3to2_button.isHidden())

        def step(name, fn, yesno_answers=(), picks=(), where=None, panel=None):
            script['yesno'][:] = yesno_answers
            script['pick'][:] = picks
            fn()
            QW.QApplication.processEvents()
            entry = dict(step=name, inputs=dict(yesno=list(yesno_answers), pick=[None if p is None else norm(p) for p in picks]), dialogs=list(log), files=tree(where) if where else None)
            if panel is not None:
                entry.update(labels(panel))
            steps.append(entry)
            log.clear()

        try:
            default_file = os.path.join(default, 'f3a', '3ab4567.png')
            os.makedirs(os.path.dirname(default_file), exist_ok=True)
            open(default_file, 'w').write('x')
            panel = P.ReviewGranularityPanel(c.gui, c, c.Read('client_files_subfolders')[0])
            texts = sorted({l.text() for l in panel.findChildren(QW.QLabel) if l.text()})
            buttons = sorted({b.text() for b in panel.findChildren(QW.QPushButton) if b.text()})
            steps.append(dict(step='opened', texts=[norm(t) for t in texts], buttons=buttons, **labels(panel)))
            step('2 to 3, not ready', panel._Migrate2To3, [False], where=default, panel=panel)
            step('2 to 3, ready', panel._Migrate2To3, [True], where=default, panel=panel)
            step('3 to 2, not ready', panel._Migrate3To2, [False], where=default, panel=panel)
            step('3 to 2, ready', panel._Migrate3To2, [True], where=default, panel=panel)
            offline = folder('offline', ['f3a/3ab4567.png'])
            step('offline 2 to 3, not ready', panel._MigrateFolder2To3, [False], where=offline, panel=panel)
            step('offline 2 to 3, cancel the picker', panel._MigrateFolder2To3, [True], [None], where=offline, panel=panel)
            step('offline 2 to 3, decline the scan', panel._MigrateFolder2To3, [True, False], [offline], where=offline, panel=panel)
            step('offline 2 to 3, ready', panel._MigrateFolder2To3, [True, True], [offline], where=offline, panel=panel)
            step('offline 2 to 3 again (already 3), decline', panel._MigrateFolder2To3, [True, False], [offline], where=offline, panel=panel)
            step('offline 3 to 2, scan says 3, go', panel._MigrateFolder3To2, [True, True], [offline], where=offline, panel=panel)
            empty = folder('empty', [])
            step('offline 2 to 3, unknown folder, decline', panel._MigrateFolder2To3, [True, False], [empty], where=empty, panel=panel)
            step('offline 3 to 2 of an empty folder, go', panel._MigrateFolder3To2, [True, True, True], [empty], where=empty, panel=panel)
        finally:
            M.ShowInformation, M.ShowCritical, Q.GetYesNo, Q.PickDirectory, T.DialogNullipotent.exec = old
        return dict(steps=steps)
    return c.CallBlockingToQt(c.gui, work)


def main():
    import hydrus_driver
    if len(sys.argv) > 1 and sys.argv[1] == '--child':
        output = sys.argv[2]
        value = hydrus_driver.run_client(os.path.join(tempfile.mkdtemp(), 'db'), record)
        with open(output, 'w') as f:
            json.dump(value, f)
        return
    with tempfile.TemporaryDirectory() as tmp:
        path = os.path.join(tmp, 'result.json')
        hydrus_driver.run_in_subprocess(os.path.abspath(__file__), '--child', path)
        with open(path) as f:
            value = json.load(f)
    with open(os.path.join(HERE, 'fixtures', 'granularity_panel.json'), 'w') as f:
        json.dump(value, f, indent=1, ensure_ascii=False)
        f.write('\n')
    print('recorded the granularity panel')


if __name__ == '__main__':
    main()

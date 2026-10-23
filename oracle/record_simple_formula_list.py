#!/usr/bin/env python3
"""Record the reference's saved simple downloader formula list (`_EditFormulae`).

The real `SidebarImporterSimpleDownloader._EditFormulae` runs on the real client: its
dialog, its `AddEditDeleteListBoxUniqueNamedObjects` and the formula editor it opens are
the real ones; only the dialogs' answers are scripted (the name asked for, accepting or
cancelling the formula editor, the yes/no questions, the clipboard). After each step the
list's rows, selection and which buttons are enabled are recorded, with every message
and question shown (in order), the clipboard text the export wrote and the
clipboard text an import would read (`pasted`). The
png menus ("to png", "to pngs", "from pngs") are recorded too: the export
panels' starting fields and the pngs they write (kept beside the fixture as
`simple_formula_list_*.png`, with the text each carries).

Usage: QT_QPA_PLATFORM=offscreen ~/pyenv/bin/python oracle/record_simple_formula_list.py
       (writes fixtures/simple_formula_list.json)
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
        from qtpy import QtCore as QC, QtWidgets as QW
        from hydrus.core import HydrusExceptions, HydrusSerialisable as S
        from hydrus.client import ClientDefaults
        from hydrus.client.gui import ClientGUIDialogsMessage as M, ClientGUIDialogsQuick as Q
        from hydrus.client.gui import ClientGUITopLevelWindowsPanels as T
        from hydrus.client.gui.pages import ClientGUISidebarImporters as I
        from hydrus.client.parsing import ClientParsing as P
        from hydrus.client import ClientStrings

        log = []
        script = dict(names=[], formula_ok=[], yes=[], picks=[], clipboard=None, steps=[])
        clipboard_out = []
        old = (M.ShowInformation, M.ShowWarning, M.ShowCritical, Q.EnterText, Q.GetYesNo, T.DialogEdit.exec, c.pub, c.GetClipboardText, Q.SelectMultipleFromList)

        def info(parent, text, *a, **k): log.append(['information', text])
        def warn(parent, text, *a, **k): log.append(['warning', text])
        def critical(parent, title, text, *a, **k): log.append(['critical', title, text])
        def enter_text(win, message, default='', *a, **k):
            name = script['names'].pop(0)
            log.append(['enter text', message, default, name])
            if name is None:
                raise HydrusExceptions.CancelledException('Dialog cancelled.')
            return name
        def yesno(parent, text, *a, **k):
            answer = script['yes'].pop(0)
            log.append(['question', text, answer])
            return QW.QDialog.DialogCode.Accepted if answer else QW.QDialog.DialogCode.Rejected
        def select_multiple(win, title, choice_tuples, *a, **k):
            answer = script['picks'].pop(0)
            log.append(['choose', title, [label for (label, data, selected) in choice_tuples], [selected for (label, data, selected) in choice_tuples], answer])
            if answer is None:
                raise HydrusExceptions.CancelledException('Dialog cancelled.')
            return [data for (label, data, selected) in choice_tuples if label in answer]
        def pub(topic, *args, **k):
            if topic == 'clipboard':
                clipboard_out.append(args[1])
                return
            return old[6](topic, *args, **k)

        def execute(dlg):
            title = dlg.windowTitle()
            if title == 'edit formula':
                ok = script['formula_ok'].pop(0)
                log.append(['formula editor', ok])
                return QW.QDialog.DialogCode.Accepted if ok else QW.QDialog.DialogCode.Rejected
            if title == 'edit simple downloader formulae':
                control = dlg._panel._control
                script['control'] = control
                script['outer'](control)
                return QW.QDialog.DialogCode.Accepted if script['apply'] else QW.QDialog.DialogCode.Rejected
            raise Exception('unexpected dialog ' + title)

        M.ShowInformation, M.ShowWarning, M.ShowCritical, Q.EnterText, Q.GetYesNo, T.DialogEdit.exec, c.pub, Q.SelectMultipleFromList = info, warn, critical, enter_text, yesno, execute, pub, select_multiple
        c.GetClipboardText = lambda: script['clipboard']

        host = QW.QWidget(c.gui)
        host._RefreshFormulae = lambda: None
        steps = []

        def rows(control):
            return [control._listbox.item(i).text() for i in range(control._listbox.count())]

        def selected(control):
            return [i for i in range(control._listbox.count()) if control._listbox.item(i).isSelected()]

        def buttons(control):
            return {b.text(): b.isEnabled() for b in control.findChildren(QW.QPushButton) if b.text() != ''}

        def select(control, *indices):
            control._listbox.clearSelection()
            for i in indices:
                control._listbox.item(i).setSelected(True)

        def step(name, action, **kw):
            log.clear()
            del clipboard_out[:]
            action(script['control'])
            QW.QApplication.processEvents()
            control = script['control']
            steps.append(dict(step=name, rows=rows(control), selected=selected(control), buttons=buttons(control),
                              dialogs=list(log), clipboard=list(clipboard_out), pasted=script['clipboard']))

        def session_run(outer, apply=False):
            script['outer'] = outer
            script['apply'] = apply
            I.SidebarImporterSimpleDownloader._EditFormulae(host)

        initial = sorted(f.GetName() for f in c.new_options.GetSimpleDownloaderFormulae())
        out = dict(initial=initial)
        first_default = ClientDefaults.GetDefaultSimpleDownloaderFormulae()[0]
        out['default_names'] = [f.GetName() for f in ClientDefaults.GetDefaultSimpleDownloaderFormulae()]

        def outer_one(control):
            steps.append(dict(step='opened', rows=rows(control), selected=selected(control), buttons=buttons(control), dialogs=[], clipboard=[]))
            script['names'][:] = ['my formula']; script['formula_ok'][:] = [True]
            step('add a formula', lambda c_: c_._add_button.click())
            script['names'][:] = ['my formula']; script['formula_ok'][:] = [True]
            step('add one with the same name', lambda c_: c_._add_button.click())
            script['names'][:] = [None]
            step('cancel the name', lambda c_: c_._add_button.click())
            script['names'][:] = ['never kept']; script['formula_ok'][:] = [False]
            step('cancel the formula editor', lambda c_: c_._add_button.click())
            names = rows(control)
            at = names.index('my formula')
            step('select a row', lambda c_: select(c_, at))
            script['names'][:] = ['renamed']; script['formula_ok'][:] = [True]
            step('edit it', lambda c_: c_._edit_button.click())
            script['names'][:] = [None]
            step('edit, cancel the name', lambda c_: c_._edit_button.click())
            script['names'][:] = ['x']; script['formula_ok'][:] = [False]
            step('edit, cancel the formula editor', lambda c_: c_._edit_button.click())
            script['names'][:] = ['renamed']; script['formula_ok'][:] = [True]
            step('edit, keep the name', lambda c_: c_._edit_button.click())
            script['yes'][:] = [False]
            step('delete, say no', lambda c_: c_._delete_button.click())
            script['yes'][:] = [True]
            step('delete, say yes', lambda c_: c_._delete_button.click())
            step('delete with nothing selected', lambda c_: c_._delete_button.click())
            step('add them all', lambda c_: c_._AddAllDefaults(ClientDefaults.GetDefaultSimpleDownloaderFormulae))
            step('add them all again', lambda c_: c_._AddAllDefaults(ClientDefaults.GetDefaultSimpleDownloaderFormulae))
            second = ClientDefaults.GetDefaultSimpleDownloaderFormulae()[1].GetName()
            script['picks'][:] = [None]
            step('select from a list, cancel', lambda c_: c_._AddSomeDefaults(ClientDefaults.GetDefaultSimpleDownloaderFormulae))
            script['picks'][:] = [[]]
            step('select from a list, none ticked', lambda c_: c_._AddSomeDefaults(ClientDefaults.GetDefaultSimpleDownloaderFormulae))
            script['picks'][:] = [[second]]
            step('select from a list, one ticked', lambda c_: c_._AddSomeDefaults(ClientDefaults.GetDefaultSimpleDownloaderFormulae))
            step('select two rows', lambda c_: select(c_, 0, 1))
            script['yes'][:] = [True]
            step('delete two', lambda c_: c_._delete_button.click())

        session_run(outer_one)
        out['actions'] = list(steps)
        del steps[:]

        def outer_two(control):
            steps.append(dict(step='opened', rows=rows(control), selected=selected(control), buttons=buttons(control), dialogs=[], clipboard=[]))
            step('select the first row', lambda c_: select(c_, 0))
            step('export to clipboard', lambda c_: c_._ExportToClipboard())
            exported = steps[-1]['clipboard'][0]
            script['clipboard'] = exported
            step('import it from the clipboard', lambda c_: c_._ImportFromClipboard())
            step('duplicate', lambda c_: c_._Duplicate())
            step('select two rows', lambda c_: select(c_, 0, 1))
            step('export two to clipboard', lambda c_: c_._ExportToClipboard())
            script['clipboard'] = steps[-1]['clipboard'][0]
            step('import both from the clipboard', lambda c_: c_._ImportFromClipboard())
            other = S.SerialisableList([ClientStrings.StringMatch(match_type=ClientStrings.STRING_MATCH_FIXED, match_value='a', example_string='a')])
            script['clipboard'] = other.DumpToString()
            step('import something that is not a formula', lambda c_: c_._ImportFromClipboard())
            script['clipboard'] = 'not json at all'
            step('import text that is not an object', lambda c_: c_._ImportFromClipboard())
            mixed = S.SerialisableList([first_default, ClientStrings.StringMatch(match_type=ClientStrings.STRING_MATCH_FIXED, match_value='a', example_string='a')])
            script['clipboard'] = mixed.DumpToString()
            step('import a formula and something else', lambda c_: c_._ImportFromClipboard())

        session_run(outer_two)
        out['exchange'] = list(steps)
        del steps[:]

        # the png menus: "to png" (`PNGExportPanel`), "to pngs" (`PNGsExportPanel`)
        # and "from pngs"; the pngs written are kept beside the fixture
        import shutil
        from hydrus.core import HydrusCompression
        from hydrus.client import ClientSerialisable
        from hydrus.client.gui import ClientGUIDialogsFiles as DF
        pngs = {}
        work_dir = tempfile.mkdtemp()

        class Nullipotent(QW.QDialog):
            def __init__(self, parent, title, *a, **k):
                super().__init__(parent)
                self.title = title
            def __enter__(self):
                return self
            def __exit__(self, *a):
                return False
            def SetPanel(self, panel):
                self.panel = panel
            def exec(self):
                p = self.panel
                if hasattr(p, '_filepicker'):
                    started = dict(dialog=self.title, title=p._title.text(), payload_description=p._payload_description.text(), width=p._width.value())
                    p._filepicker.SetPath(os.path.join(work_dir, 'single'))
                    p._Update()
                    p._export.click()
                    written = ['single.png']
                else:
                    started = dict(dialog=self.title, width=p._width.value(), directory=p._directory_picker.GetPath())
                    folder = os.path.join(work_dir, 'many')
                    os.makedirs(folder, exist_ok=True)
                    p._directory_picker.SetPath(folder)
                    p._Update()
                    p._export.click()
                    written = sorted(os.listdir(folder))
                    written = [os.path.join('many', w) for w in written]
                files = []
                for w in written:
                    path = os.path.join(work_dir, w)
                    name = 'simple_formula_list_' + os.path.basename(w).replace(' ', '_')
                    shutil.copy(path, os.path.join(HERE, 'fixtures', name))
                    payload = ClientSerialisable.LoadFromPNG(path)
                    files.append(dict(written=os.path.basename(w), fixture=name, text=HydrusCompression.DecompressBytesToString(payload)))
                log.append(['png export', started, files])
                return 0

        class FileDialog:
            def __init__(self, parent, message, *a, **k):
                self.message = message
            def __enter__(self):
                return self
            def __exit__(self, *a):
                return False
            def exec(self):
                log.append(['pick files', self.message])
                return QW.QDialog.DialogCode.Accepted
            def GetPaths(self):
                return script['png_paths']

        old_dialogs = (T.DialogNullipotent, DF.FileDialog)
        T.DialogNullipotent, DF.FileDialog = Nullipotent, FileDialog

        def outer_four(control):
            steps.append(dict(step='opened', rows=rows(control), selected=selected(control), buttons=buttons(control), dialogs=[], clipboard=[]))
            step('select the first row', lambda c_: select(c_, 0))
            step('export to png', lambda c_: c_._ExportToPNG())
            step('select two rows', lambda c_: select(c_, 0, 1))
            step('export to pngs', lambda c_: c_._ExportToPNGs())
            many = [f['written'] for f in steps[-1]['dialogs'][0][2]]
            script['png_paths'] = [os.path.join(work_dir, 'many', m) for m in many]
            step('import both from pngs', lambda c_: c_._ImportFromPNG())
            bad = os.path.join(work_dir, 'bad.png')
            with open(bad, 'wb') as fh:
                fh.write(b'not a png')
            script['png_paths'] = [bad]
            step('import a file that is no png', lambda c_: c_._ImportFromPNG())

        session_run(outer_four)
        out['pngs'] = list(steps)
        del steps[:]
        T.DialogNullipotent, DF.FileDialog = old_dialogs
        shutil.rmtree(work_dir)

        # the dialog's result: cancel leaves the saved formulae, apply replaces them
        def outer_three(control):
            script['names'][:] = ['saved one']; script['formula_ok'][:] = [True]
            control._add_button.click()
        before = sorted(f.GetName() for f in c.new_options.GetSimpleDownloaderFormulae())
        session_run(outer_three, apply=False)
        after_cancel = sorted(f.GetName() for f in c.new_options.GetSimpleDownloaderFormulae())
        session_run(outer_three, apply=True)
        after_apply = sorted(f.GetName() for f in c.new_options.GetSimpleDownloaderFormulae())
        out['commit'] = dict(before=before, after_cancel=after_cancel, after_apply=after_apply)

        M.ShowInformation, M.ShowWarning, M.ShowCritical, Q.EnterText, Q.GetYesNo, T.DialogEdit.exec, c.pub, c.GetClipboardText, Q.SelectMultipleFromList = old
        return out
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
    with open(os.path.join(HERE, 'fixtures', 'simple_formula_list.json'), 'w') as f:
        json.dump(value, f, indent=1, ensure_ascii=False)
        f.write('\n')
    print('recorded the simple formula list')


if __name__ == '__main__':
    main()

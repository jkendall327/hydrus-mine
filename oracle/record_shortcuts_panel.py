#!/usr/bin/env python3
"""Record Options > shortcuts (`ShortcutsPanel`) driving the real reference panel.

Fresh temporary `basic` client. The real `ShortcutsPanel` is built over the
client's shortcut sets. Its dialogs are the real ones (`DialogEdit`,
`EditShortcutSetPanel`, `EditSelectFromListPanel`, the yes/no question and the
information message): only `exec` / the message boxes are replaced, with
scripted answers, and everything the user would see in them is written down
first: titles, messages, list entries in displayed order, the set editor's
name field (text, enabled) and its description labels.

Scenarios: the panel as opened; the set editor opened on every built-in set, on
a custom set and on a new one; edit (built-in) with one shortcut added; restore
defaults (cancelled, answered no, answered yes, set missing); add (cancelled,
vetoed on a reserved name then renamed, a duplicate name); edit/rename and
delete (declined, accepted) of custom sets.
"""
import json
import os
import sys
import tempfile
from pathlib import Path

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
OUT = HERE / 'fixtures' / 'shortcuts_panel.json'


def record(session):
    def qt():
        from qtpy import QtWidgets as W
        from hydrus.core import HydrusExceptions
        from hydrus.client import ClientApplicationCommand as A
        from hydrus.client.gui import ClientGUIDialogsMessage as M
        from hydrus.client.gui import ClientGUIDialogsQuick as Q
        from hydrus.client.gui import ClientGUIShortcuts as S
        from hydrus.client.gui import ClientGUIShortcutControls as E
        from hydrus.client.gui import ClientGUITopLevelWindowsPanels as P
        from hydrus.client.gui.panels import ClientGUIScrolledPanelsButtonQuestions as BQ
        from hydrus.client.gui.panels.options import ShortcutsPanel as SP
        c = session.controller
        log = []
        answers = []  # scripted: callables taking the dialog's panel and dialog

        def labels(widget):
            return [w.text() for w in widget.findChildren(W.QLabel) if w.text()]

        def set_rows(panel, ctrl):
            out = []
            for data in ctrl.GetData():
                out.append(list(panel._GetDisplayTuple(data)))
            return sorted(out, key=lambda r: r[0])

        def state_entry(dialog, panel):
            entry = dict(dialog=dialog.windowTitle(), panel=type(panel).__name__)
            if isinstance(panel, E.EditShortcutSetPanel):
                entry.update(name=panel._name.text(), name_enabled=panel._name.isEnabled(), labels=labels(panel), rows=len(panel._shortcuts.GetData()))
            if isinstance(panel, BQ.QuestionYesNoPanel):
                entry.update(labels=labels(panel))
            if type(panel).__name__ == 'EditSelectFromListPanel':
                entry.update(entries=[panel._list.item(i).text() for i in range(panel._list.count())],
                             selected=[i.text() for i in panel._list.selectedItems()])
            return entry

        def fake_exec(self):
            # a vetoed Apply leaves the dialog open: the next scripted step runs on it
            while True:
                entry = state_entry(self, self._panel)
                log.append(entry)
                if answers.pop(0)(self, self._panel, entry) is not False:
                    return int(self.result())

        P.DialogEdit.exec = fake_exec
        P.DialogCustomButtonQuestion.exec = fake_exec
        messages = []
        M.ShowInformation = lambda win, text, *a, **k: messages.append(('information', text))
        M.ShowWarning = lambda win, text, *a, **k: messages.append(('warning', text))

        def accept(dialog, panel, entry, edit=None):
            if edit:
                edit(panel)
            try:
                panel.CheckValid()
                panel.GetValue()
            except HydrusExceptions.VetoException as e:
                messages.append(('warning', str(e)))
                entry['vetoed'] = str(e)
                return False
            dialog.done(W.QDialog.DialogCode.Accepted)
            entry['accepted'] = True
            return True

        def cancel(dialog, panel, entry):
            dialog.done(W.QDialog.DialogCode.Rejected)
            entry['accepted'] = False
            return True

        def pick_list(text):
            def go(dialog, panel, entry):
                for i in range(panel._list.count()):
                    if panel._list.item(i).text() == text:
                        panel._list.setCurrentRow(i)
                        panel._list.item(i).setSelected(True)
                    else:
                        panel._list.item(i).setSelected(False)
                entry['picked'] = text
                return accept(dialog, panel, entry)
            return go

        def add_one_row(panel, key='Q'):
            shortcut = S.Shortcut(S.SHORTCUT_TYPE_KEYBOARD_CHARACTER, ord(key), S.SHORTCUT_PRESS_TYPE_PRESS, [S.SHORTCUT_MODIFIER_CTRL, S.SHORTCUT_MODIFIER_SHIFT])
            command = A.ApplicationCommand.STATICCreateSimpleCommand(A.SIMPLE_REFRESH)
            panel._shortcuts.AddData((shortcut, command))

        def make_panel():
            all_sets = S.shortcuts_manager().GetShortcutSets()
            panel = SP.ShortcutsPanel(c.gui, c.new_options, all_sets)
            return panel

        def select_data(ctrl, name):
            for data in ctrl.GetData():
                if data.GetName() == name:
                    ctrl.selectionModel().clearSelection()
                    ctrl.SelectDatas([data])
                    return
            raise KeyError(name)

        def snapshot(panel):
            return dict(
                reserved=set_rows(panel, panel._reserved_shortcuts),
                custom=set_rows(panel, panel._custom_shortcuts),
            )

        result = {}
        panel = make_panel()
        result['opened'] = dict(
            labels=labels(panel),
            buttons=[b.text() for b in panel.findChildren(W.QPushButton)],
            checkboxes=[b.isChecked() for b in panel.findChildren(W.QCheckBox)],
            **snapshot(panel),
        )

        # the set editor opened on each kind of set
        editors = []
        sets_by_name = {s.GetName(): s for s in S.shortcuts_manager().GetShortcutSets()}
        for name in sorted(S.SHORTCUTS_RESERVED_NAMES) + ['a custom set']:
            shortcut_set = sets_by_name.get(name) or S.ShortcutSet(name)
            editor = E.EditShortcutSetPanel(c.gui, shortcut_set, False, True)
            editors.append(dict(name=name, text=editor._name.text(), enabled=editor._name.isEnabled(), labels=labels(editor), rows=len(editor._shortcuts.GetData())))
            editor.deleteLater()
        new = E.EditShortcutSetPanel(c.gui, S.ShortcutSet('new shortcuts'), False, True)
        editors.append(dict(name='new shortcuts', text=new._name.text(), enabled=new._name.isEnabled(), labels=labels(new), rows=0))
        result['editors'] = editors

        # edit a built-in set
        select_data(panel._reserved_shortcuts, 'main_gui')
        before = snapshot(panel)
        answers.append(lambda d, p, e: accept(d, p, e, add_one_row))
        panel._EditReserved()
        result['edit_reserved'] = dict(log=log[-1], before=before['reserved'], after=snapshot(panel)['reserved'])
        # cancelled edit changes nothing
        before = snapshot(panel)
        answers.append(cancel)
        panel._EditReserved()
        result['edit_reserved_cancel'] = dict(log=log[-1], unchanged=before == snapshot(panel))

        # restore defaults
        restores = []

        def restore(pick, then, label):
            start = len(log)
            before = snapshot(panel)
            messages.clear()
            answers.append(pick_list(pick) if pick else cancel)
            if then is not None:
                answers.append(then)
            panel._RestoreDefaults()
            restores.append(dict(label=label, picked=pick, log=log[start:], messages=list(messages), before=before['reserved'], after=snapshot(panel)['reserved']))

        def yes(dialog, panel_, entry):
            dialog.done(W.QDialog.DialogCode.Accepted)
            entry['accepted'] = True
            return True

        def no(dialog, panel_, entry):
            dialog.done(W.QDialog.DialogCode.Rejected)
            entry['accepted'] = False
            return True
        restore(None, None, 'cancel the list')
        restore('main_gui', no, 'existing, answered no')
        restore('main_gui', yes, 'existing, answered yes')
        # a set that has gone missing
        panel._reserved_shortcuts.DeleteDatas([d for d in panel._reserved_shortcuts.GetData() if d.GetName() == 'global'])
        restore('global', None, 'missing')
        result['restore_defaults'] = restores

        # custom sets
        customs = []

        def add(name, rows_to_add, label, rename_after_veto=None):
            start = len(log)
            messages.clear()

            def edit(p):
                p._name.setText(name)
                for key in 'QW'[:rows_to_add]:
                    add_one_row(p, key)
            answers.append(lambda d, p, e: accept(d, p, e, edit))
            if rename_after_veto:
                answers.append(lambda d, p, e: accept(d, p, e, lambda q: q._name.setText(rename_after_veto)))
            panel._Add()
            customs.append(dict(label=label, log=log[start:], messages=list(messages), custom=snapshot(panel)['custom']))

        start = len(log)
        answers.append(cancel)
        panel._Add()
        customs.append(dict(label='add, cancelled', log=log[start:], custom=snapshot(panel)['custom']))
        add('rating keys', 2, 'add rating keys')
        add('rating keys', 0, 'add the same name again')
        add('main_gui', 1, 'add on a reserved name, vetoed then renamed', rename_after_veto='other keys')

        # edit: rename onto another set's name, then onto its own
        select_data(panel._custom_shortcuts, 'rating keys')
        start = len(log)
        messages.clear()
        answers.append(lambda d, p, e: accept(d, p, e, lambda q: q._name.setText('rating keys (1)')))
        panel._EditCustom()
        customs.append(dict(label='edit rename onto another name', log=log[start:], custom=snapshot(panel)['custom']))
        # delete: declined, then accepted
        select_data(panel._custom_shortcuts, 'other keys')
        start = len(log)
        answers.append(no)
        panel._Delete()
        customs.append(dict(label='delete declined', log=log[start:], custom=snapshot(panel)['custom']))
        start = len(log)
        answers.append(yes)
        panel._Delete()
        customs.append(dict(label='delete accepted', log=log[start:], custom=snapshot(panel)['custom']))
        result['custom'] = customs
        result['help'] = None
        messages.clear()
        panel._ShowHelp()
        result['help'] = messages[0][1]
        return result
    return session.controller.CallBlockingToQt(session.controller.gui, qt)


def main():
    import hydrus_driver
    if len(sys.argv) > 1:
        import record_api
        output = Path(sys.argv[2])
        result = hydrus_driver.run_client(record_api.unpack_fixture('basic'), record)
        output.write_text(json.dumps(result))
        return
    with tempfile.TemporaryDirectory() as directory:
        output = Path(directory) / 'out.json'
        hydrus_driver.run_in_subprocess(str(Path(__file__).resolve()), '--child', str(output))
        result = json.loads(output.read_text())
    OUT.write_text(json.dumps(result, indent=2) + '\n')


if __name__ == '__main__':
    main()

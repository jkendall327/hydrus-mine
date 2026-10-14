#!/usr/bin/env python3
"""Record Options > shortcuts (`ShortcutsPanel`) on the basic client: the built-in and custom
set lists, what the editor shows for a built-in set (its name locked) and for a new custom one,
the veto on a built-in name, how same-named custom sets are named apart, and the questions
"restore defaults" asks (the list of defaults, the confirmation, and what it says when the
set has gone missing).
"""
import json, os, sys, tempfile
HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)

def record(session):
    controller = session.controller
    gui = controller.gui
    def work():
        from qtpy import QtWidgets as QW
        from hydrus.core import HydrusExceptions as E, HydrusSerialisable as S
        from hydrus.client.gui import ClientGUIDialogsMessage as M, ClientGUIDialogsQuick as Q, ClientGUIShortcuts as SC
        from hydrus.client.gui import ClientGUITopLevelWindowsPanels as TLW, ClientGUIShortcutControls as SCC
        from hydrus.client.gui.panels.options import ShortcutsPanel as SP
        all_shortcuts = [s.Duplicate() for s in SC.shortcuts_manager().GetShortcutSets()]
        panel = SP.ShortcutsPanel(gui, controller.new_options, all_shortcuts)
        def rows(listctrl):
            return [list(panel._GetDisplayTuple(d)) for d in listctrl.GetData()]
        result = {'reserved_names': list(SC.SHORTCUTS_RESERVED_NAMES), 'reserved_rows': rows(panel._reserved_shortcuts), 'custom_rows': rows(panel._custom_shortcuts), 'descriptions': dict(SC.shortcut_names_to_descriptions)}
        class Scripted(QW.QDialog):
            seen = []
            script = None
            def __init__(self, parent, title):
                super().__init__(parent)
                self._title = title
            def __enter__(self): return self
            def __exit__(self, *a): return False
            def SetPanel(self, p): self._panel = p
            def exec(self):
                p = self._panel
                texts = [w.text() for w in p.findChildren(QW.QLabel)]
                Scripted.seen.append({'title': self._title, 'name': p._name.text(), 'name_enabled': p._name.isEnabled(), 'rows': [list(p._ConvertShortcutTupleToDisplayTuple(d)) for d in p._shortcuts.GetData()], 'labels': texts, 'buttons': sorted(b.text() for b in p.findChildren(QW.QPushButton) if b.text())})
                if Scripted.script is None:
                    return QW.QDialog.DialogCode.Rejected
                Scripted.script(p)
                try:
                    p.GetValue()
                except E.VetoException as e:
                    Scripted.seen[-1]['veto'] = str(e)
                    return QW.QDialog.DialogCode.Rejected
                return QW.QDialog.DialogCode.Accepted
        old = (TLW.DialogEdit, Q.GetYesNo, Q.SelectFromList, M.ShowInformation)
        asked = []
        answers = {'yes': True, 'pick': None}
        def get_yes_no(parent, text, **kw):
            asked.append({'kind': 'yes_no', 'text': text})
            return QW.QDialog.DialogCode.Accepted if answers['yes'] else QW.QDialog.DialogCode.Rejected
        def select(parent, title, choice_tuples, **kw):
            asked.append({'kind': 'select', 'title': title, 'choices': [c[0] for c in choice_tuples]})
            if answers['pick'] is None: raise E.CancelledException()
            return answers['pick']
        TLW.DialogEdit = Scripted
        SP.ClientGUITopLevelWindowsPanels.DialogEdit = Scripted
        Q.GetYesNo = get_yes_no
        Q.SelectFromList = select
        M.ShowInformation = lambda parent, text: asked.append({'kind': 'information', 'text': text})
        try:
            # the editor on every built-in set
            Scripted.seen = []
            Scripted.script = None
            result['edit_reserved'] = []
            for data in list(panel._reserved_shortcuts.GetData()):
                Scripted.seen = []
                panel._reserved_shortcuts.SelectDatas([data], deselect_others=True)
                panel._EditReserved()
                result['edit_reserved'].append(Scripted.seen[0])
            # a new custom set, and what it is called when added again
            Scripted.seen = []
            Scripted.script = None
            panel._Add()
            result['add_cancelled'] = {'dialogs': list(Scripted.seen), 'custom_rows': rows(panel._custom_shortcuts)}
            def named(name):
                return lambda p: p._name.setText(name)
            Scripted.seen = []
            for name in ['rating keys', 'rating keys', 'main_gui', 'Rating Keys']:
                Scripted.script = named(name)
                panel._Add()
            result['add_named'] = {'typed': ['rating keys', 'rating keys', 'main_gui', 'Rating Keys'], 'vetoes': [d.get('veto') for d in Scripted.seen], 'custom_rows': rows(panel._custom_shortcuts)}
            # restore defaults: cancel, "no", "yes", and a missing set
            del asked[:]
            answers['pick'] = None
            panel._RestoreDefaults()
            result['restore_cancel'] = list(asked)
            before = rows(panel._reserved_shortcuts)
            panel._reserved_shortcuts.SelectDatas([next(d for d in panel._reserved_shortcuts.GetData() if d.GetName() == 'main_gui')], deselect_others=True)
            Scripted.script = lambda p: None
            # alter main_gui so a restore is visible
            existing = next(d for d in panel._reserved_shortcuts.GetData() if d.GetName() == 'main_gui')
            edited = existing.Duplicate()
            edited.SetCommand(SC.Shortcut(SC.SHORTCUT_TYPE_KEYBOARD_CHARACTER, SC.SHORTCUT_PRESS_TYPE_PRESS, ord('X'), []), next(iter(existing.GetShortcutsAndCommands()))[1])
            panel._reserved_shortcuts.ReplaceData(existing, edited)
            altered = rows(panel._reserved_shortcuts)
            del asked[:]
            answers['pick'] = 'main_gui'; answers['yes'] = False
            panel._RestoreDefaults()
            no_rows = rows(panel._reserved_shortcuts)
            result['restore_no'] = {'asked': list(asked), 'changed': no_rows != altered}
            del asked[:]
            answers['yes'] = True
            panel._RestoreDefaults()
            result['restore_yes'] = {'asked': list(asked), 'rows_equal_original': rows(panel._reserved_shortcuts) == before}
            # a missing set
            missing = next(d for d in panel._reserved_shortcuts.GetData() if d.GetName() == 'global')
            panel._reserved_shortcuts.DeleteDatas([missing])
            del asked[:]
            answers['pick'] = 'global'
            panel._RestoreDefaults()
            result['restore_missing'] = {'asked': list(asked), 'reserved_count': len(panel._reserved_shortcuts.GetData())}
        finally:
            TLW.DialogEdit, Q.GetYesNo, Q.SelectFromList, M.ShowInformation = old
        panel.deleteLater()
        return result
    return controller.CallBlockingToQt(gui, work)

def main():
    import hydrus_driver
    if len(sys.argv) > 1 and sys.argv[1] == '--child':
        import record_api
        out = sys.argv[2]
        result = hydrus_driver.run_client(record_api.unpack_fixture('basic'), record)
        with open(out, 'w') as f: json.dump(result, f)
        return
    with tempfile.TemporaryDirectory() as work:
        out = os.path.join(work, 'result.json')
        hydrus_driver.run_in_subprocess(os.path.abspath(__file__), '--child', out)
        with open(out) as f: result = json.load(f)
    with open(os.path.join(HERE, 'fixtures/shortcuts_panel.json'), 'w') as f:
        json.dump(result, f, indent=1, ensure_ascii=False)
        f.write('\n')
    print('wrote shortcuts_panel.json')
if __name__ == '__main__': main()

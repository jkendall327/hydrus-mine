#!/usr/bin/env python3
"""Record real Qt regex favourite CRUD, duplicate/cancel boundaries and regex menu actions.

Invalid regex fragments are advisory and may be saved. Add rejects an exact
phrase/description duplicate; canceling either input stage preserves the list.
"""
import json
import os
import sys
import tempfile
import record_string_converter_editor as recorder
HERE = os.path.dirname(os.path.abspath(__file__))
OUT = os.path.join(HERE, 'fixtures/regex_favourites.json')
ACTIONS = [
    ['add', r'\d+', 'numbers', True],
    ['add', r'\d+', 'numbers', True],
    ['add', '[', 'fragment', True],
    ['add', 'discarded', 'unused', False],
    ['add', 'temporary', None, True],
    ['edit', ['[', 'fragment'], '^a+$', 'letters only', True],
    ['edit', ['^a+$', 'letters only'], 'discarded', 'unused', False],
    ['edit', ['^a+$', 'letters only'], 'temporary', None, True],
    ['delete', [[r'\d+', 'numbers'], ['z+', 'last']], False],
    ['delete', [[r'\d+', 'numbers'], ['z+', 'last']], True],
]

def record(session):
    controller = session.controller
    gui = controller.gui
    def qt():
        from qtpy import QtWidgets as QW
        from hydrus.client.gui.panels import ClientGUIScrolledPanelsEditRegexFavourites as M
        from hydrus.client.gui import ClientGUIDialogsQuick, ClientGUIDialogsMessage, ClientGUICore
        from hydrus.client.gui.widgets import ClientGUIRegex
        from hydrus.core import HydrusExceptions, HydrusConstants as HC
        defaults = list(HC.options['regex_favourites'])
        initial = [('z+', 'last'), ('a+', 'letters')]
        panel = M.EditRegexFavourites(gui, initial)
        said = []
        opened = []
        current = []
        class Dialog(QW.QWidget):
            def __init__(self, parent, title, **kwargs):
                super().__init__(gui)
                said.append({'dialog': title})
            def __enter__(self): return self
            def __exit__(self, *args): return False
            def SetPanel(self, child): self.panel = child
            def exec(self):
                control = self.panel._control
                before = control.GetValue()
                index = 2 if current[0] == 'edit' else 1
                phrase = current[index]
                control.SetValue(phrase)
                opened.append({'before': before, 'phrase': phrase, 'valid': control._regex_text.objectName() == 'HydrusValid'})
                return QW.QDialog.DialogCode.Accepted if current[-1] else QW.QDialog.DialogCode.Rejected
        def enter(parent, question, **kwargs):
            said.append({'asked': question, 'default': kwargs.get('default')})
            index = 3 if current[0] == 'edit' else 2
            if current[index] is None: raise HydrusExceptions.CancelledException()
            return current[index]
        def yes_no(parent, question, **kwargs):
            said.append({'asked': question})
            return QW.QDialog.DialogCode.Accepted if current[-1] else QW.QDialog.DialogCode.Rejected
        M.ClientGUITopLevelWindowsPanels.DialogEdit = Dialog
        ClientGUIDialogsQuick.EnterText = enter
        ClientGUIDialogsQuick.GetYesNo = yes_no
        ClientGUIDialogsMessage.ShowWarning = lambda parent, text, **kwargs: said.append({'warning': text})
        def state():
            ctrl = panel._regexes
            model = ctrl.model()
            return {'rows': [[model.data(model.index(row, col)) for col in range(model.columnCount())] for row in range(model.rowCount())],
                    'value': panel.GetValue(), 'selected': sorted(i.row() for i in ctrl.selectionModel().selectedRows())}
        steps = [{'state': state()}]
        for action in ACTIONS:
            current[:] = action
            said.clear(); opened.clear()
            if action[0] == 'add': panel._Add()
            elif action[0] == 'edit':
                panel._regexes.SelectDatas([tuple(action[1])], deselect_others=True)
                panel._Edit()
            else:
                panel._regexes.SelectDatas([tuple(v) for v in action[1]], deselect_others=True)
                panel._regexes.ProcessDeleteAction()
            steps.append({'do': action, 'said': list(said), 'opened': list(opened), 'state': state()})
        clipboard = []
        original_pub = controller.pub
        def pub(topic, *args, **kwargs):
            if topic == 'clipboard' and args[0] == 'text': clipboard.append(args[1]); return
            return original_pub(topic, *args, **kwargs)
        controller.pub = pub
        HC.options['regex_favourites'] = panel.GetValue()
        menu_rows = []
        menus = {}
        urls = []
        ClientGUIRegex.ClientGUIExecutableActions.OpenExternallyURLDefault = lambda parent, url: urls.append(url)
        def popup(parent, menu):
            for top in menu.actions():
                if top.menu() is None or top.text() == 'favourites': continue
                entries = []
                for action in top.menu().actions():
                    if action.isSeparator(): continue
                    before = len(clipboard), len(urls)
                    action.trigger()
                    if len(clipboard) > before[0]: entries.append([action.text(), clipboard[-1]])
                    elif len(urls) > before[1]: entries.append([action.text(), urls[-1]])
                menus[top.text()] = entries
            favourites = next(a.menu() for a in menu.actions() if a.text() == 'favourites')
            for action in favourites.actions():
                if not action.isSeparator() and action.isEnabled() and action.text() != 'manage favourites':
                    menu_rows.append(action.text())
                    action.trigger()
        ClientGUICore.core().PopupMenu = popup
        button = ClientGUIRegex.RegexButton(gui, show_group_menu=True)
        button._ShowMenu()
        clipboard = clipboard[-len(panel.GetValue()):]
        return {'menus': menus, 'initial': initial, 'defaults': defaults, 'steps': steps, 'menu_rows': menu_rows, 'copied': clipboard}
    return controller.CallBlockingToQt(gui, qt)
recorder.record = record
if __name__ == '__main__':
    import hydrus_driver
    if len(sys.argv) > 1 and sys.argv[1] == '--child': recorder.child(sys.argv[2])
    else:
        with tempfile.TemporaryDirectory() as work:
            path = os.path.join(work, 'regex.json')
            hydrus_driver.run_in_subprocess(os.path.abspath(__file__), '--child', path)
            with open(path) as stream: result = json.load(stream)
        with open(OUT, 'w') as stream:
            json.dump(result, stream, indent=1, ensure_ascii=False); stream.write('\n')
        print(f'wrote {OUT}')

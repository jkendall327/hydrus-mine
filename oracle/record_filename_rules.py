#!/usr/bin/env python3
"""Record real filename-tagging advanced list controls and child validation.

Drives FilenameTaggingOptionsPanel's quick namespace Add/Edit/Delete, its
real DialogInputNamespaceRegex validation, regex Enter/add and double-click
removal to the input. Scripted modal responses only replace exec/publication;
all handlers, selection, lists, generated tags and saved/reopened options run
unchanged in a private reference client. No files are imported or exported.
"""
import json
import sys
import tempfile
from pathlib import Path
HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
PATHS = ['/srv/import/artist/0012 - page 3.jpg', '/srv/import/artist/0001.png', '/srv/import/Some_File (v2).webm']


def record(session):
    from qtpy import QtWidgets as QW
    from hydrus.client import ClientConstants as CC
    from hydrus.client.gui import ClientGUIDialogs, ClientGUIDialogsQuick, ClientGUIDialogsMessage, ClientGUICore, ClientGUITopLevelWindowsPanels
    from hydrus.client.gui.widgets import ClientGUIRegex
    from hydrus.core import HydrusConstants as HC
    from hydrus.client.gui.importing import ClientGUIImport
    from hydrus.client.importing.options import FilenameTaggingOptions
    controller, gui = session.controller, session.controller.gui
    old = (ClientGUIDialogs.DialogInputNamespaceRegex.exec, ClientGUIDialogsQuick.GetYesNo,
           ClientGUIDialogsMessage.ShowWarning, ClientGUIDialogsMessage.ShowCritical,
           ClientGUICore.core().PopupMenu, controller.pub, ClientGUIRegex.ClientGUIExecutableActions.OpenExternallyURLDefault, ClientGUITopLevelWindowsPanels.DialogEdit, ClientGUIDialogsQuick.EnterText)
    old_favourites = list(HC.options['regex_favourites'])
    def drive():
        options = FilenameTaggingOptions.FilenameTaggingOptions()
        panel = ClientGUIImport.FilenameTaggingOptionsPanel(gui, CC.DEFAULT_LOCAL_TAG_SERVICE_KEY, options, True)
        advanced = panel._advanced_panel
        calls, responses, answers = [], [], []
        def warning(parent, text, *args, **kwargs): calls.append({'kind': 'warning', 'text': text})
        def critical(parent, title, text, *args, **kwargs): calls.append({'kind': 'critical', 'title': title, 'text': text})
        def dialog_exec(dialog):
            attempts = responses.pop(0)
            calls.append({'kind': 'child', 'title': dialog.windowTitle(), 'initial': list(dialog.GetInfo()), 'attempts': attempts})
            if attempts is None: return QW.QDialog.DialogCode.Rejected
            for namespace, regex in attempts:
                dialog._namespace.setText(namespace)
                dialog._regex.SetValue(regex)
                dialog.EventOK()
                if dialog.result() == QW.QDialog.DialogCode.Accepted: return dialog.result()
            return QW.QDialog.DialogCode.Rejected
        def question(parent, text, *args, **kwargs):
            answer = answers.pop(0)
            calls.append({'kind': 'question', 'text': text, 'answer': answer})
            return QW.QDialog.DialogCode.Accepted if answer else QW.QDialog.DialogCode.Rejected
        ClientGUIDialogs.DialogInputNamespaceRegex.exec = dialog_exec
        ClientGUIDialogsQuick.GetYesNo = question
        ClientGUIDialogsMessage.ShowWarning = warning
        ClientGUIDialogsMessage.ShowCritical = critical
        def state():
            value = panel.GetFilenameTaggingOptions()
            quick, regexes = value.AdvancedToTuple()
            return {'quick': [list(q) for q in quick], 'regexes': list(regexes),
                    'quick_selected': [list(q) for q in advanced._quick_namespaces_list.GetData(only_selected=True)],
                    'regex_selected': advanced._regexes.GetSelectedIndices(), 'input': advanced._regex_input.GetValue(),
                    'tags': [sorted(value.GetTags(CC.DEFAULT_LOCAL_TAG_SERVICE_KEY, path)) for path in PATHS]}
        initial, steps = state(), []
        def step(action, selected=None, attempts=None, text=None, answer=None):
            if selected is not None:
                if action.startswith('quick'):
                    data = advanced._quick_namespaces_list.GetData()
                    advanced._quick_namespaces_list.SelectDatas([data[i] for i in selected], deselect_others=True)
                else:
                    advanced._regexes.clearSelection()
                    for i in selected: advanced._regexes.item(i).setSelected(True)
            calls.clear()
            if action in ('quick_add', 'quick_edit'): responses.append(attempts)
            if action == 'quick_delete': answers.append(answer)
            if action == 'regex_add': advanced._regex_input.SetValue(text)
            {'quick_add': advanced.AddQuickNamespace, 'quick_edit': advanced.EditQuickNamespaces,
             'quick_delete': advanced._quick_namespaces_list.ShowDeleteSelectedDialog,
             'regex_add': advanced.AddRegex, 'regex_remove': lambda: advanced.EventRemoveRegex(None),
             'quick_sort_regex': lambda: advanced._quick_namespaces_list.Sort(1, False),
             'quick_sort_namespace': lambda: advanced._quick_namespaces_list.Sort(0, True)}[action]()
            steps.append({'action': action, 'selected': selected, 'attempts': attempts, 'text': text, 'answer': answer,
                          'calls': list(calls), 'state': state()})
        step('quick_add', attempts=None)
        step('quick_add', attempts=[['', r'\d+'], ['page', r'(?<=page )\d+']])
        step('quick_add', attempts=[['version', '[unclosed'], ['version', r'(?<=\(v)\d+(?=\))']])
        step('quick_add', attempts=[['page', r'(?<=page )\d+']])
        step('quick_add', attempts=[['creator:name', r'artist']])
        step('quick_sort_regex')
        step('quick_sort_namespace')
        step('quick_edit', selected=[0], attempts=None)
        step('quick_edit', selected=[0], attempts=[['  SOURCE  ', 'artist']])
        step('quick_edit', selected=[0, 1], attempts=[['all', '']])
        step('quick_delete', selected=[0, 1], answer=False)
        step('quick_delete', selected=[0, 1], answer=True)
        step('regex_add', text='')
        step('regex_add', text='[unclosed')
        step('regex_add', text=r'(\d{4})')
        step('regex_add', text=r'(?<=page )\d+')
        step('regex_add', text=r'(\d{4})')
        step('regex_remove', selected=[0, 2])
        step('regex_add', text=advanced._regex_input.GetValue())
        step('regex_remove', selected=[])
        step('quick_add', attempts=[['extension', r'(?<=\.)[^.]+$']])
        # Exercise the actual RegexButton on each original input. Publication is
        # intercepted only to record clipboard payloads and external URLs.
        menu_favourites = [(r'\d+', 'numbers'), ('artist', 'creator')]
        HC.options['regex_favourites'] = menu_favourites
        menu_records, current_publication = [], []
        def pub(topic, *args, **kwargs):
            if topic == 'clipboard' and args[0] == 'text':
                current_publication.append({'kind': 'copy', 'value': args[1]}); return
            return old[5](topic, *args, **kwargs)
        controller.pub = pub
        ClientGUIRegex.ClientGUIExecutableActions.OpenExternallyURLDefault = lambda parent, url: current_publication.append({'kind': 'url', 'value': url})
        def popup(parent, menu):
            def read(menu):
                rows = []
                for action in menu.actions():
                    if action.isSeparator(): rows.append({'kind': 'separator'}); continue
                    if action.menu() is not None:
                        rows.append({'kind': 'menu', 'label': action.text(), 'rows': read(action.menu())}); continue
                    current_publication.clear()
                    if action.text() != 'manage favourites': action.trigger()
                    rows.append({'kind': 'item', 'label': action.text(), 'enabled': action.isEnabled(), 'outputs': list(current_publication)})
                return rows
            menu_records.append(read(menu))
        ClientGUICore.core().PopupMenu = popup
        advanced._regex_input._regex_button._ShowMenu()
        quick_child = ClientGUIDialogs.DialogInputNamespaceRegex(gui, namespace='page', regex=r'\d+')
        quick_child._regex._regex_button._ShowMenu()
        managed = []
        for accepted in [False, True]:
            class ManageDialog(QW.QWidget):
                def __init__(self, parent, title, **kwargs):
                    super().__init__(gui); self.title = title
                def __enter__(self): return self
                def __exit__(self, *args): return False
                def SetPanel(self, panel): self.panel = panel
                def exec(self):
                    if self.title == 'manage regex favourites':
                        self.panel._Add()
                        return QW.QDialog.DialogCode.Accepted if accepted else QW.QDialog.DialogCode.Rejected
                    self.panel._control.SetValue('.*')
                    return QW.QDialog.DialogCode.Accepted
            ClientGUITopLevelWindowsPanels.DialogEdit = ManageDialog
            ClientGUIDialogsQuick.EnterText = lambda *args, **kwargs: 'from filename input'
            before = list(HC.options['regex_favourites'])
            filename_before = panel.GetFilenameTaggingOptions().AdvancedToTuple()
            advanced._regex_input._regex_button._ManageFavourites()
            managed.append({'accepted': accepted, 'before': [list(row) for row in before],
                            'after': [list(row) for row in HC.options['regex_favourites']],
                            'filename_unchanged': filename_before == panel.GetFilenameTaggingOptions().AdvancedToTuple()})
        applied = panel.GetFilenameTaggingOptions()
        reopened = ClientGUIImport.FilenameTaggingOptionsPanel(gui, CC.DEFAULT_LOCAL_TAG_SERVICE_KEY, applied, False)
        quick, regexes = reopened.GetFilenameTaggingOptions().AdvancedToTuple()
        return {'paths': PATHS, 'initial': initial, 'steps': steps, 'applied': state(),
                'reopened': {'quick': [list(q) for q in quick], 'regexes': list(regexes)},
                'draft_isolated': options.AdvancedToTuple() == ([], []),
                'regex_menus': menu_records, 'menu_favourites': [list(row) for row in menu_favourites], 'managed_favourites': managed}
    try: return controller.CallBlockingToQt(gui, drive)
    finally:
        (ClientGUIDialogs.DialogInputNamespaceRegex.exec, ClientGUIDialogsQuick.GetYesNo,
         ClientGUIDialogsMessage.ShowWarning, ClientGUIDialogsMessage.ShowCritical,
         ClientGUICore.core().PopupMenu, controller.pub, ClientGUIRegex.ClientGUIExecutableActions.OpenExternallyURLDefault, ClientGUITopLevelWindowsPanels.DialogEdit, ClientGUIDialogsQuick.EnterText) = old
        HC.options['regex_favourites'] = old_favourites


def main():
    import hydrus_driver, record_api
    if len(sys.argv) > 1 and sys.argv[1] == '--child':
        destination = Path(sys.argv[2])
        result = hydrus_driver.run_client(record_api.unpack_fixture('basic'), record)
        destination.write_text(json.dumps(result)); return
    with tempfile.TemporaryDirectory() as work:
        destination = Path(work) / 'filename-rules.json'
        hydrus_driver.run_in_subprocess(str(Path(__file__).resolve()), '--child', str(destination))
        result = json.loads(destination.read_text())
    (HERE / 'fixtures/filename_rules.json').write_text(json.dumps(result, indent=2) + '\n')
if __name__ == '__main__': main()

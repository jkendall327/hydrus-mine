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
    from hydrus.client.gui import ClientGUIDialogs, ClientGUIDialogsQuick, ClientGUIDialogsMessage
    from hydrus.client.gui.importing import ClientGUIImport
    from hydrus.client.importing.options import FilenameTaggingOptions
    controller, gui = session.controller, session.controller.gui
    old = (ClientGUIDialogs.DialogInputNamespaceRegex.exec, ClientGUIDialogsQuick.GetYesNo,
           ClientGUIDialogsMessage.ShowWarning, ClientGUIDialogsMessage.ShowCritical)
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
             'regex_add': advanced.AddRegex, 'regex_remove': lambda: advanced.EventRemoveRegex(None)}[action]()
            steps.append({'action': action, 'selected': selected, 'attempts': attempts, 'text': text, 'answer': answer,
                          'calls': list(calls), 'state': state()})
        step('quick_add', attempts=None)
        step('quick_add', attempts=[['', r'\d+'], ['page', r'(?<=page )\d+']])
        step('quick_add', attempts=[['version', '[unclosed'], ['version', r'(?<=\(v)\d+(?=\))']])
        step('quick_add', attempts=[['page', r'(?<=page )\d+']])
        step('quick_add', attempts=[['creator:name', r'artist']])
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
        applied = panel.GetFilenameTaggingOptions()
        reopened = ClientGUIImport.FilenameTaggingOptionsPanel(gui, CC.DEFAULT_LOCAL_TAG_SERVICE_KEY, applied, False)
        quick, regexes = reopened.GetFilenameTaggingOptions().AdvancedToTuple()
        return {'paths': PATHS, 'initial': initial, 'steps': steps, 'applied': state(),
                'reopened': {'quick': [list(q) for q in quick], 'regexes': list(regexes)},
                'draft_isolated': options.AdvancedToTuple() == ([], [])}
    try: return controller.CallBlockingToQt(gui, drive)
    finally:
        (ClientGUIDialogs.DialogInputNamespaceRegex.exec, ClientGUIDialogsQuick.GetYesNo,
         ClientGUIDialogsMessage.ShowWarning, ClientGUIDialogsMessage.ShowCritical) = old


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

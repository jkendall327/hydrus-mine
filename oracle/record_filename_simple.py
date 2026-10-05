#!/usr/bin/env python3
"""Record the real simple filename-tagging panel's all/selected tag actions.

Drives the actual manual-import path selection, clean tag-entry callbacks,
add-only autocomplete paste, direct paste buttons and list removal. Records
per-file and selected-union tags, misc controls and saved/reopened folder
options. Clipboard text/error publication is scripted; no files are imported.
"""
import json
import sys
import tempfile
from pathlib import Path
HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
PATHS = ['/srv/import/artist/0012 - page 3.jpg', '/srv/import/artist/0001.png', '/srv/import/Some_File (v2).webm']


def record(session):
    from hydrus.client import ClientConstants as CC
    from hydrus.client.gui import ClientGUIDialogsMessage
    from hydrus.client.gui.importing import ClientGUIImport
    from hydrus.core import HydrusTags, HydrusExceptions
    controller, gui = session.controller, session.controller.gui
    old_clipboard, old_critical = controller.GetClipboardText, ClientGUIDialogsMessage.ShowCritical
    def drive():
        panel = ClientGUIImport.EditLocalImportFilenameTaggingPanel(gui, PATHS)
        page = next(p for p in panel._filename_tagging_option_pages if p._service_key == CC.DEFAULT_LOCAL_TAG_SERVICE_KEY)
        simple = page._filename_tagging_panel._simple_panel
        calls = []
        ClientGUIDialogsMessage.ShowCritical = lambda parent, title, text, *args, **kwargs: calls.append({'title': title, 'text': text})
        def state():
            options = page._filename_tagging_panel.GetFilenameTaggingOptions()
            all_tags, filename, directories = options.SimpleToTuple()
            return {'all': sorted(all_tags), 'selected': [PATHS.index(path) for path in simple._selected_paths],
                    'single': sorted(simple._single_tags.GetTags()), 'enabled': simple._tag_autocomplete_selection.isEnabled(),
                    'tags': [page._GetTags(i, path) for i, path in enumerate(PATHS)],
                    'filename': list(filename), 'directories': {str(k): list(v) for k, v in directories.items()}}
        initial, steps = state(), []
        def step(action, value=None):
            calls.clear()
            error = None
            try:
                if action == 'select': page._paths_list.SelectDatas([(i, PATHS[i]) for i in value], deselect_others=True)
                elif action == 'enter_all': simple.EnterTags(HydrusTags.CleanTags(value))
                elif action == 'enter_single': simple.EnterTagsSingle(HydrusTags.CleanTags(value))
                elif action == 'autocomplete_paste_single': simple._tag_autocomplete_selection.tagsPasted.emit(list(HydrusTags.CleanTags(value)))
                elif action in ('paste_all', 'paste_single'):
                    controller.GetClipboardText = lambda: value
                    (simple._PasteTags if action == 'paste_all' else simple._PasteSingleTags)()
                elif action == 'remove_all': simple._tags._RemoveTags(value)
                elif action == 'remove_single': simple._single_tags._RemoveTags(value)
                elif action == 'filename_text':
                    simple._filename_namespace.SetValue(value)
                    simple._filename_namespace._lineedit.textEdited.emit(value)
                elif action == 'filename_check': simple._filename_namespace.SetChecked(value)
                elif action == 'directory_text':
                    simple._directory_namespace_controls[value[0]].SetValue(value[1])
                    simple._directory_namespace_controls[value[0]]._lineedit.textEdited.emit(value[1])
                elif action == 'directory_check': simple._directory_namespace_controls[value[0]].SetChecked(value[1])
                elif action == 'clipboard_missing':
                    def missing(): raise HydrusExceptions.DataMissing('clipboard has no text')
                    controller.GetClipboardText = missing
                    simple._PasteTags()
                else: raise AssertionError(action)
            except Exception as e: error = type(e).__name__ + ': ' + str(e)
            steps.append({'action': action, 'value': value, 'calls': list(calls), 'error': error, 'state': state()})
        step('enter_all', ['Imported', ' Creator:Someone ', ' : ', 'Imported'])
        step('enter_all', ['Imported'])
        step('paste_all', ' SHARED\nCreator:Someone\n\n: \n Imported ')
        step('remove_all', ['creator:someone'])
        step('select', [0])
        step('enter_single', ['First Only', 'Shared'])
        step('select', [2])
        step('enter_single', ['Third Only', 'Shared'])
        step('select', [0, 2])
        step('enter_single', ['First Only'])
        step('autocomplete_paste_single', ['Third Only', 'Autocomplete New'])
        step('paste_single', 'Third Only\n Direct Paste\n : \n')
        step('remove_single', ['shared'])
        step('select', [1])
        step('enter_single', ['Middle Only'])
        step('select', [])
        step('paste_all', '\n : \n')
        step('filename_text', 'title')
        step('filename_text', '')
        step('filename_check', False)
        step('directory_text', [-1, 'series'])
        step('directory_text', [-2, 'creator'])
        step('directory_check', [-2, False])
        step('clipboard_missing')
        applied = page._filename_tagging_panel.GetFilenameTaggingOptions()
        reopened = ClientGUIImport.FilenameTaggingOptionsPanel(gui, CC.DEFAULT_LOCAL_TAG_SERVICE_KEY, applied, False)
        tags, filename, directories = reopened.GetFilenameTaggingOptions().SimpleToTuple()
        _, by_path = panel.GetValue()
        return {'paths': PATHS, 'initial': initial, 'steps': steps, 'applied': state(),
                'value': {p: {controller.services_manager.GetName(k): sorted(v) for k, v in services.items()} for p, services in by_path.items()},
                'reopened': {'all': sorted(tags), 'filename': list(filename), 'directories': {str(k): list(v) for k, v in directories.items()}, 'selected_visible': not reopened._simple_panel._single_tags_panel.isHidden()}}
    try: return controller.CallBlockingToQt(gui, drive)
    finally:
        controller.GetClipboardText, ClientGUIDialogsMessage.ShowCritical = old_clipboard, old_critical


def main():
    import hydrus_driver, record_api
    if len(sys.argv) > 1 and sys.argv[1] == '--child':
        destination = Path(sys.argv[2])
        result = hydrus_driver.run_client(record_api.unpack_fixture('basic'), record)
        destination.write_text(json.dumps(result)); return
    with tempfile.TemporaryDirectory() as work:
        destination = Path(work) / 'filename-simple.json'
        hydrus_driver.run_in_subprocess(str(Path(__file__).resolve()), '--child', str(destination))
        result = json.loads(destination.read_text())
    (HERE / 'fixtures/filename_simple.json').write_text(json.dumps(result, indent=2) + '\n')
if __name__ == '__main__': main()

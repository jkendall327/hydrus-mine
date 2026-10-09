#!/usr/bin/env python3
"""Record the export folder editor's search taking typed predicates, and the
export folder running that search.

A private basic-fixture client builds the real EditExportFolderPanel over a
new export folder. Into its search's autocomplete it types, one at a time,
system predicates (good and unparsable), tags, a negated tag that cancels its
positive, a namespace, a wildcard and padded text, each entered as the Enter
key does; the active predicates list's rows and the text left in the field are
recorded after each. Rows are then removed by activating them in that list.

Then a second editor gets `system:filesize > 1B` and `system:filetype is png`
typed in, a destination folder and the phrase `{file_id}`, with run now set;
the folder it makes runs (the real ExportFolder.DoWork), and the files it
writes are recorded. The png predicate is removed from the list, the folder
runs again, and the folder's files are recorded again.

Usage: QT_QPA_PLATFORM=offscreen ~/pyenv/bin/python oracle/record_export_folder_predicates.py
"""
import json
import os
import sys
import tempfile

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
import hydrus_driver
import record_api

OUT = os.path.join(HERE, 'fixtures', 'export_folder_predicates.json')

TYPED = [
    'system:filesize > 1B',
    'system:filetype is png',
    'system:nonsense words',
    'blue eyes',
    'creator:*',
    'blue*',
    '-blue eyes',
    '  system:inbox  ',
    'system:width < 200',
    'system:filetype is png',
    'series:examples',
]


def record(session):
    controller = session.controller
    gui = controller.GetMainTLW()
    qt = lambda f: controller.CallBlockingToQt(gui, f)

    from hydrus.client.exporting import ClientExportingFiles
    from hydrus.client.gui import ClientGUITopLevelWindowsPanels
    from hydrus.client.gui.exporting import ClientGUIExport

    def editor(name):
        frame = ClientGUITopLevelWindowsPanels.FrameThatTakesScrollablePanel(gui, 'edit export folder')
        panel = ClientGUIExport.EditExportFolderPanel(frame, ClientExportingFiles.ExportFolder(name))
        return (frame, panel)

    def rows(panel):
        box = panel._tag_autocomplete._predicates_listbox
        return [''.join(text for (text, _) in box._GetRowsOfTextsAndColours(term)[0]) for term in box._ordered_terms]

    def enter(panel, typed):
        ac = panel._tag_autocomplete
        ac._text_ctrl.setText(typed)
        ac._BroadcastCurrentInputFromEnterKey(False)
        return {'typed': typed, 'rows': rows(panel), 'left': ac._text_ctrl.text()}

    def remove(panel, index):
        box = panel._tag_autocomplete._predicates_listbox
        box._selected_terms = {box._ordered_terms[index]}
        box._Activate(False, False)
        return {'removed': index, 'rows': rows(panel)}

    out = {}

    def typing():
        (frame, panel) = editor('typing')
        steps = [{'rows': rows(panel)}]
        for typed in TYPED:
            steps.append(enter(panel, typed))
        steps.append(remove(panel, 0))
        steps.append(remove(panel, 2))
        frame.close()
        return steps

    out['typing'] = qt(typing)

    destination = tempfile.mkdtemp(prefix='export-folder-predicates-')

    def make():
        (frame, panel) = editor('mine')
        panel._name.setText('mine')
        panel._path.SetPath(destination)
        panel._pattern.setText('{file_id}')
        panel._run_regularly.setChecked(False)
        panel._run_now.setChecked(True)
        for typed in ['system:filesize > 1B', 'system:filetype is png']:
            enter(panel, typed)
        return (frame, panel)

    (frame, panel) = qt(make)
    runs = []
    for step in ['both', 'png removed']:
        if step == 'png removed':
            qt(lambda: remove(panel, 1))
        shown = qt(lambda: rows(panel))
        folder = qt(panel.GetValue)
        folder.DoWork()
        runs.append({'step': step, 'rows': shown, 'error': folder.GetLastError(), 'files': sorted(os.listdir(destination))})
    qt(frame.close)
    out['runs'] = runs
    return out


def main():
    if len(sys.argv) > 1 and sys.argv[1] == '--child':
        output = sys.argv[2]
        db = record_api.unpack_fixture('basic')
        result = hydrus_driver.run_client(db, record)
        with open(output, 'w') as f:
            json.dump(result, f, ensure_ascii=False)
        return
    with tempfile.TemporaryDirectory() as tmp:
        path = os.path.join(tmp, 'out.json')
        hydrus_driver.run_in_subprocess(os.path.abspath(__file__), '--child', path)
        with open(path) as f:
            result = json.load(f)
    with open(OUT, 'w') as f:
        json.dump(result, f, ensure_ascii=False, indent=1)
        f.write('\n')
    print('wrote', OUT)


if __name__ == '__main__':
    main()

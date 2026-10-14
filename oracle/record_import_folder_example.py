#!/usr/bin/env python3
"""Record the import folder editor's filename tagging options dialog.

In the running client, on the `basic` fixture, the real `EditImportFolderPanel`
is opened on a folder holding a text file, a one-byte ".jpg" and a png named
`pic 1.png`; "add" on its filename tagging box (the service picked by script,
the edit dialog replaced by a script) records the dialog's title, the example
path it was given (the editor picks it, `_PathChanged`: the first of the first
25 directory entries that is a file with an allowed mime by content), the
example output as the dialog opens, and after the script has entered the tag
`Imported` for all files and ticked "add filename?"; then the dialog accepted.
A second "add" for the same service records the warning shown. Finally the
box's rows.

Usage: QT_QPA_PLATFORM=offscreen python oracle/record_import_folder_example.py
       (writes fixtures/import_folder_example.json)
"""
import json, os, shutil, sys, tempfile

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
PNG = os.path.join(HERE, 'fixtures', 'media', 'apng_rgba.png')


def record(session):
    controller = session.controller

    def qt():
        from hydrus.client import ClientConstants as CC
        from hydrus.client.gui import ClientGUIDialogsMessage, ClientGUIDialogsQuick
        from hydrus.client.gui import ClientGUITopLevelWindowsPanels as T
        from hydrus.client.gui.importing import ClientGUIImportFolders
        from hydrus.client.importing import ClientImportLocal
        from qtpy import QtWidgets as QW

        work = tempfile.mkdtemp()
        with open(os.path.join(work, 'notes.txt'), 'w') as f:
            f.write('a sidecar')
        with open(os.path.join(work, 'bad.jpg'), 'wb') as f:
            f.write(b'x')
        shutil.copy(PNG, os.path.join(work, 'pic 1.png'))

        out = {'dialogs': [], 'warnings': []}
        warnings = out['warnings']
        ClientGUIDialogsMessage.ShowWarning = lambda win, message, **kw: warnings.append(message)
        ClientGUIDialogsQuick.SelectServiceKey = lambda *a, **kw: CC.DEFAULT_LOCAL_TAG_SERVICE_KEY

        class FakeDialog(QW.QDialog):
            def __init__(self, parent, title, **kwargs):
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
                row = {'title': self.title, 'example_path': os.path.basename(p._example_path_input.text()),
                       'example_in_folder': os.path.dirname(p._example_path_input.text()) == work}
                p.RefreshTags()
                row['output_as_opened'] = p._example_output.text()
                simple = p._filename_tagging_options_panel._simple_panel
                simple.EnterTags(['Imported'])
                simple._filename_namespace.SetChecked(True)
                p.RefreshTags()
                row['output_edited'] = p._example_output.text()
                row['namespace_default'] = simple._filename_namespace.GetValue()
                out['dialogs'].append(row)
                return QW.QDialog.DialogCode.Accepted

        T.DialogEdit = FakeDialog
        folder = ClientImportLocal.ImportFolder('example folder', path=work)
        panel = ClientGUIImportFolders.EditImportFolderPanel(controller.gui, folder)
        panel._AddFilenameTaggingOptions()
        panel._AddFilenameTaggingOptions()
        out['rows'] = [list(panel._ConvertFilenameTaggingOptionsToDisplayTuple(d)) for d in panel._filename_tagging_options.GetData()]
        panel.deleteLater()
        return out
    return controller.CallBlockingToQt(controller.gui, qt)


def child(out):
    import hydrus_driver, record_api
    result = hydrus_driver.run_client(record_api.unpack_fixture('basic'), record)
    with open(out, 'w') as f:
        json.dump(result, f, ensure_ascii=False)


def main():
    if len(sys.argv) > 1 and sys.argv[1] == '--child':
        child(sys.argv[2])
        return
    import hydrus_driver
    with tempfile.TemporaryDirectory() as tmp:
        path = os.path.join(tmp, 'r.json')
        hydrus_driver.run_in_subprocess(os.path.abspath(__file__), '--child', path)
        with open(path) as f:
            result = json.load(f)
    with open(os.path.join(HERE, 'fixtures', 'import_folder_example.json'), 'w') as f:
        json.dump(result, f, ensure_ascii=False, indent=1)
        f.write('\n')
    print(json.dumps(result, indent=1))


if __name__ == '__main__':
    main()

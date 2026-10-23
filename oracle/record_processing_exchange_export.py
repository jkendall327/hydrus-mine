#!/usr/bin/env python3
"""Record the real processing-step queue exporting selected steps and pasting.

`QueueListBox._ExportToClipboard`, `_ExportToPNG` (the `PNGExportPanel` it
opens: its fields, then 'export' to a path) and `_ImportFromClipboard`
(paste), on a queue of three steps: what is exported for no selection, for
one, for two that are not next to each other (in queue order whatever order
they were selected in) and for all; the panel's defaults and the PNG it
writes; and each paste's messages and resulting queue (a list, one step, an
empty clipboard, text that is not a package, with the reference's own
"Clipboard Error!" text); "duplicate" of two selected steps; and "from
pngs" of the png it wrote and of a file that is no png.
"""
import json
import os
import shutil
import sys
import tempfile
import record_string_converter_editor as recorder
HERE = os.path.dirname(os.path.abspath(__file__))
OUT = os.path.join(HERE, 'fixtures/processing_exchange_export.json')
PNG = os.path.join(HERE, 'fixtures/processing_exchange_export.png')


def record(session):
    controller = session.controller
    gui = controller.gui

    def qt():
        from hydrus.client import ClientStrings, ClientSerialisable
        from hydrus.client.gui import ClientGUIStringPanels, ClientGUIDialogsMessage, ClientGUIDialogsQuick, ClientGUITopLevelWindowsPanels
        from hydrus.client.parsing import ClientParsing
        from hydrus.core import HydrusSerialisable, HydrusExceptions
        processor = ClientStrings.StringProcessor()
        processor.SetProcessingSteps([
            ClientStrings.StringSplitter(separator=','),
            ClientStrings.StringConverter(conversions=[(3, '!')]),
            ClientStrings.StringConverter(conversions=[(2, '>')]),
        ])
        panel = ClientGUIStringPanels.EditStringProcessorPanel(gui, processor, ClientParsing.ParsingTestData({}, ('a,b',)))
        queue = panel._processing_steps
        clipboard = ['sentinel']
        messages = []
        original_pub = controller.pub

        def pub(topic, *args, **kwargs):
            if topic == 'clipboard' and args[0] == 'text':
                clipboard[0] = args[1]
                return
            return original_pub(topic, *args, **kwargs)

        controller.pub = pub

        def get_clipboard():
            if clipboard[0] is None:
                raise HydrusExceptions.DataMissing('There was no text on the clipboard!')
            return clipboard[0]

        controller.GetClipboardText = get_clipboard
        ClientGUIDialogsMessage.ShowInformation = lambda parent, text, **kwargs: messages.append({'kind': 'information', 'text': text})
        ClientGUIDialogsMessage.ShowWarning = lambda parent, text, **kwargs: messages.append({'kind': 'warning', 'text': text})
        ClientGUIDialogsMessage.ShowCritical = lambda parent, title, text, **kwargs: messages.append({'kind': 'critical', 'title': title, 'text': text})

        def state():
            value = panel.GetValue()
            return {'processor': value.GetSerialisableTuple(), 'processed': value.ProcessStrings(['a,b']),
                    'selected': [i for i in range(queue._listbox.count()) if queue._listbox.item(i).isSelected()]}

        def select(rows):
            for i in range(queue._listbox.count()):
                queue._listbox.item(i).setSelected(False)
            for i in rows:
                queue._listbox.item(i).setSelected(True)

        out = {'start': state(), 'exports': []}
        for rows in [[], [1], [2, 0], [0, 1, 2]]:
            select(rows)
            clipboard[0] = 'sentinel'
            queue._ExportToClipboard()
            out['exports'].append({'selected': rows, 'clipboard': clipboard[0] if clipboard[0] == 'sentinel' else json.loads(clipboard[0])})

        # export to png: the panel it opens and the file it writes
        opened = []

        from qtpy import QtWidgets as QW

        class Dialog(QW.QDialog):
            def __init__(self, parent, title, *args, **kwargs):
                super().__init__(parent)
                self.title = title

            def __enter__(self):
                return self

            def __exit__(self, *args):
                return False

            def SetPanel(self, panel):
                self.panel = panel

            def exec(self):
                p = self.panel
                before = {'dialog': self.title, 'class': type(p).__name__, 'title': p._title.text(), 'payload_description': p._payload_description.text(),
                          'description': p._text.text(), 'width': p._width.value(), 'path': p._filepicker.GetPath(),
                          'payload_enabled': p._payload_description.isEnabled()}
                with tempfile.TemporaryDirectory() as folder:
                    p._filepicker.SetPath(os.path.join(folder, 'steps'))
                    p._title.setText('my steps')
                    p._text.setText('a note')
                    p._width.setValue(256)
                    p._export.click()
                    written = os.path.join(folder, 'steps.png')
                    shutil.copy(written, PNG)
                    payload = ClientSerialisable.LoadFromPNG(written)
                    obj = HydrusSerialisable.CreateFromNetworkBytes(payload)
                    opened.append({**before, 'button_after': p._export.text(), 'png_file': os.path.basename(written),
                                   'png_payload': json.loads(obj.DumpToString()), 'last_png_export_dir_set': controller.new_options.GetNoneableString('last_png_export_dir') == folder})
                return 0

        ClientGUITopLevelWindowsPanels.DialogNullipotent = Dialog
        select([0, 2])
        queue._ExportToPNG()
        out['png'] = opened[0]
        select([])
        before = len(opened)
        queue._ExportToPNG()
        out['png_none_selected_opens_panel'] = len(opened) != before

        # paste
        out['pastes'] = []
        one = ClientStrings.StringConverter(conversions=[(3, '?')])
        lst = HydrusSerialisable.SerialisableList([one, ClientStrings.StringSplitter(separator=';')])
        for (name, text) in [('a list of two steps', lst.DumpToString()), ('one step', one.DumpToString()), ('nothing on the clipboard', None), ('not a package', 'not valid JSON'), ('empty text', '')]:
            del messages[:]
            select([])
            clipboard[0] = text
            queue._ImportFromClipboard()
            out['pastes'].append({'name': name, 'text': text, 'messages': list(messages), 'state': state()})

        # duplicate: the selected steps again, at the end
        del messages[:]
        select([0, 2])
        queue._Duplicate()
        out['duplicate'] = {'selected': [0, 2], 'messages': list(messages), 'state': state()}

        # from pngs: the png written above, and a file that is no png
        from hydrus.client.gui import ClientGUIDialogsFiles
        picked = [[]]

        class FileDialog:
            def __init__(self, parent, message, *args, **kwargs):
                self.message = message

            def __enter__(self):
                return self

            def __exit__(self, *args):
                return False

            def exec(self):
                out['png_picker_title'] = self.message
                return QW.QDialog.DialogCode.Accepted

            def GetPaths(self):
                return picked[0]

        original_file_dialog = ClientGUIDialogsFiles.FileDialog
        ClientGUIDialogsFiles.FileDialog = FileDialog
        out['png_imports'] = []
        with tempfile.TemporaryDirectory() as folder:
            bad = os.path.join(folder, 'bad.png')
            with open(bad, 'wb') as fh:
                fh.write(b'not a png')
            for (name, paths) in [('the png of two steps', [PNG]), ('a file that is no png', [bad])]:
                del messages[:]
                select([])
                picked[0] = paths
                queue._ImportFromPNG()
                out['png_imports'].append({'name': name, 'messages': list(messages), 'state': state()})
        ClientGUIDialogsFiles.FileDialog = original_file_dialog
        return out
    return controller.CallBlockingToQt(gui, qt)


recorder.record = record
if __name__ == '__main__':
    import hydrus_driver
    if len(sys.argv) > 1 and sys.argv[1] == '--child':
        recorder.child(sys.argv[2])
    else:
        with tempfile.TemporaryDirectory() as work:
            path = os.path.join(work, 'processing.json')
            hydrus_driver.run_in_subprocess(os.path.abspath(__file__), '--child', path)
            with open(path) as stream:
                result = json.load(stream)
        with open(OUT, 'w') as stream:
            json.dump(result, stream, indent=1, ensure_ascii=False)
            stream.write('\n')
        print(f'wrote {OUT}')

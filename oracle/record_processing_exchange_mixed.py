#!/usr/bin/env python3
"""Record the real processing-step queue importing packages that are not all
processing steps (`QueueListBox._ImportObject`): a list mixing steps with a
tag filter, a bare string processor, a list of nothing permitted, and a PNG of
the mixed list. After each import: the messages shown, in order, and the
queue (its processor, the processed example and the selected rows).
"""
import json
import os
import sys
import tempfile
import record_string_converter_editor as recorder
HERE = os.path.dirname(os.path.abspath(__file__))
OUT = os.path.join(HERE, 'fixtures/processing_exchange_mixed.json')
PNG = os.path.join(HERE, 'fixtures/processing_exchange_mixed.png')

def record(session):
    controller = session.controller
    gui = controller.gui
    def qt():
        from qtpy import QtWidgets as QW
        from hydrus.client import ClientStrings, ClientSerialisable
        from hydrus.client.gui import ClientGUIStringPanels, ClientGUIDialogsMessage, ClientGUIDialogsQuick, ClientGUIDialogsFiles
        from hydrus.client.parsing import ClientParsing
        from hydrus.core import HydrusSerialisable, HydrusTags
        processor = ClientStrings.StringProcessor()
        processor.SetProcessingSteps([ClientStrings.StringSplitter(separator=',')])
        panel = ClientGUIStringPanels.EditStringProcessorPanel(gui, processor, ClientParsing.ParsingTestData({}, ('a,b',)))
        queue = panel._processing_steps
        clipboard = ['']
        messages = []
        controller.GetClipboardText = lambda: clipboard[0]
        ClientGUIDialogsMessage.ShowInformation = lambda parent, text, **kwargs: messages.append({'kind': 'information', 'text': text})
        ClientGUIDialogsMessage.ShowWarning = lambda parent, text, **kwargs: messages.append({'kind': 'warning', 'text': text})
        ClientGUIDialogsMessage.ShowCritical = lambda parent, title, text, **kwargs: messages.append({'kind': 'critical', 'title': title, 'text': text})
        ClientGUIDialogsQuick.PresentClipboardParseError = lambda *args, **kwargs: messages.append({'kind': 'clipboard parse error'})
        def state():
            value = panel.GetValue()
            return {'processor': value.GetSerialisableTuple(), 'processed': value.ProcessStrings(['a,b']),
                    'selected': [i for i in range(queue._listbox.count()) if queue._listbox.item(i).isSelected()]}
        converter = ClientStrings.StringConverter(conversions=[(3, '!')])
        match = ClientStrings.StringMatch(match_type=ClientStrings.STRING_MATCH_ANY, example_string='a')
        tag_filter = HydrusTags.TagFilter()
        mixed = HydrusSerialisable.SerialisableList([converter, tag_filter, match])
        bare_processor = ClientStrings.StringProcessor()
        bare_processor.SetProcessingSteps([ClientStrings.StringConverter(conversions=[(3, '?')])])
        nothing = HydrusSerialisable.SerialisableList([HydrusTags.TagFilter()])
        ClientSerialisable.DumpToPNG(512, mixed.DumpToNetworkBytes(), 'processing steps', 'mixed', '', PNG)
        cases = [('mixed list', mixed.DumpToString()), ('bare processor', bare_processor.DumpToString()), ('nothing permitted', nothing.DumpToString())]
        steps = [{'start': state()}]
        for (name, text) in cases:
            del messages[:]
            for i in range(queue._listbox.count()):
                queue._listbox.item(i).setSelected(False)
            clipboard[0] = text
            queue._ImportFromClipboard()
            steps.append({'name': name, 'how': 'clipboard', 'text': text, 'messages': list(messages), 'state': state()})
        class Dialog:
            def __init__(self, *args, **kwargs): pass
            def __enter__(self): return self
            def __exit__(self, *args): return False
            def exec(self): return QW.QDialog.DialogCode.Accepted
            def GetPaths(self): return [PNG]
        ClientGUIDialogsFiles.FileDialog = Dialog
        import hydrus.client.gui.lists.ClientGUIListBoxes as L
        L.ClientGUIDialogsFiles.FileDialog = Dialog
        del messages[:]
        for i in range(queue._listbox.count()):
            queue._listbox.item(i).setSelected(False)
        queue._ImportFromPNG()
        steps.append({'name': 'mixed png', 'how': 'png', 'messages': list(messages), 'state': state()})
        return {'steps': steps}
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

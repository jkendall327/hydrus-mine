#!/usr/bin/env python3
"""Record real Qt processing-step export selection, clipboard import and PNG.

The queue exports one object or a SerialisableList in execution order; import
appends steps and updates processing. Invalid clipboard text preserves the queue.
"""
import json
import os
import sys
import tempfile
import record_string_converter_editor as recorder
HERE = os.path.dirname(os.path.abspath(__file__))
OUT = os.path.join(HERE, 'fixtures/processing_exchange.json')

def record(session):
    controller = session.controller
    gui = controller.gui
    def qt():
        from hydrus.client import ClientStrings, ClientSerialisable
        from hydrus.client.gui import ClientGUIStringPanels, ClientGUIDialogsMessage, ClientGUIDialogsQuick
        from hydrus.client.parsing import ClientParsing
        from hydrus.core import HydrusSerialisable
        processor = ClientStrings.StringProcessor()
        processor.SetProcessingSteps([
            ClientStrings.StringSplitter(separator=','),
            ClientStrings.StringConverter(conversions=[(3, '!')]),
        ])
        panel = ClientGUIStringPanels.EditStringProcessorPanel(gui, processor, ClientParsing.ParsingTestData({}, ('a,b',)))
        queue = panel._processing_steps
        clipboard = ['']
        messages = []
        original_pub = controller.pub
        def pub(topic, *args, **kwargs):
            if topic == 'clipboard' and args[0] == 'text':
                clipboard[0] = args[1]
                return
            return original_pub(topic, *args, **kwargs)
        controller.pub = pub
        controller.GetClipboardText = lambda: clipboard[0]
        ClientGUIDialogsMessage.ShowInformation = lambda parent, text, **kwargs: messages.append(text)
        ClientGUIDialogsMessage.ShowWarning = lambda parent, text, **kwargs: messages.append(text)
        ClientGUIDialogsQuick.PresentClipboardParseError = lambda *args, **kwargs: messages.append('clipboard parse error')
        def state():
            value = panel.GetValue()
            return {'processor': value.GetSerialisableTuple(), 'processed': value.ProcessStrings(['a,b']),
                    'selected': [i for i in range(queue._listbox.count()) if queue._listbox.item(i).isSelected()]}
        start = state()
        queue._listbox.item(1).setSelected(True)
        queue._ExportToClipboard()
        single = json.loads(clipboard[0])
        queue._listbox.item(0).setSelected(True)
        queue._ExportToClipboard()
        multiple = json.loads(clipboard[0])
        obj = HydrusSerialisable.CreateFromString(clipboard[0])
        ClientSerialisable.DumpToPNG(512, obj.DumpToNetworkBytes(), 'processing steps', '2 processing steps', '', os.path.join(HERE, 'fixtures/processing_exchange.png'))
        queue._ImportFromClipboard()
        imported = state()
        before_invalid = state()
        clipboard[0] = 'not valid JSON'
        queue._ImportFromClipboard()
        after_invalid = state()
        return {'start': start, 'single': single, 'multiple': multiple, 'imported': imported,
                'before_invalid': before_invalid, 'after_invalid': after_invalid, 'messages': messages}
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

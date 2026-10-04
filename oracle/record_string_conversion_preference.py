#!/usr/bin/env python3
"""Record real Qt child acceptance/cancel and last-conversion option reload.

Discard each parent after using the conversion child. Serialize/reload the
actual ClientOptions between parents; only child acceptance changes the option.
"""
import json
import os
import sys
import tempfile
import record_string_converter_editor as recorder
HERE = os.path.dirname(os.path.abspath(__file__))
OUT = os.path.join(HERE, 'fixtures/string_conversion_preference.json')

def record(session):
    controller = session.controller
    gui = controller.gui
    def qt():
        from qtpy import QtWidgets as QW
        from hydrus.client import ClientOptions, ClientStrings
        from hydrus.client.gui import ClientGUIStringPanels as M
        from hydrus.core import HydrusSerialisable
        options = ClientOptions.ClientOptions()
        controller.new_options = options
        current = {}
        opened = []
        class Dialog(QW.QWidget):
            def __init__(self, *args, **kwargs): super().__init__(gui)
            def __enter__(self): return self
            def __exit__(self, *args): return False
            def SetPanel(self, panel): self.panel = panel
            def exec(self):
                panel = self.panel
                opened.append({'type': panel._conversion_type.currentText(), 'text': panel._data_text.text()})
                if current.get('type'):
                    choice = panel._conversion_type
                    choice.setCurrentIndex([choice.itemText(i) for i in range(choice.count())].index(current['type']))
                if 'text' in current: panel._data_text.setText(current['text'])
                return QW.QDialog.DialogCode.Accepted if current['accept'] else QW.QDialog.DialogCode.Rejected
        M.ClientGUITopLevelWindowsPanels.DialogEdit = Dialog
        steps = []
        for action in [
            {'type': 'prepend text', 'text': 'saved prefix', 'accept': True},
            {'type': 'append text', 'text': 'canceled suffix', 'accept': False},
            {'type': 'reverse text', 'accept': True},
            {'accept': False},
        ]:
            current.clear(); current.update(action)
            panel = M.EditStringConverterPanel(gui, ClientStrings.StringConverter(), example_string_override='example')
            panel._AddConversion()
            saved = options.GetRawSerialisable('last_used_string_conversion_step')
            minimal = HydrusSerialisable.SerialisableDictionary()
            minimal['last_used_string_conversion_step'] = saved
            steps.append({'do': action, 'opened': opened[-1], 'saved': saved.GetSerialisableTuple(), 'options': [22, 8, minimal.GetSerialisableTuple()]})
            panel.deleteLater()
            options = HydrusSerialisable.CreateFromSerialisableTuple(options.GetSerialisableTuple())
            controller.new_options = options
        return {'steps': steps}
    return controller.CallBlockingToQt(gui, qt)
recorder.record = record
if __name__ == '__main__':
    import hydrus_driver
    if len(sys.argv) > 1 and sys.argv[1] == '--child': recorder.child(sys.argv[2])
    else:
        with tempfile.TemporaryDirectory() as work:
            path = os.path.join(work, 'preference.json')
            hydrus_driver.run_in_subprocess(os.path.abspath(__file__), '--child', path)
            with open(path) as stream: result = json.load(stream)
        with open(OUT, 'w') as stream:
            json.dump(result, stream, indent=1, ensure_ascii=False); stream.write('\n')
        print(f'wrote {OUT}')

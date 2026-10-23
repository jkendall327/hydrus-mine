#!/usr/bin/env python3
"""Record the reference's formula panel exporting and importing formulae.

On the `basic` fixture, the real `EditFormulaPanel` (the formula box of the
content and subsidiary parsers' editors: its description, "edit formula",
"change formula type", "export" and "import") is given a formula of each
kind, taken from `fixtures/formulas.json` (the reference's own
serialisations; for each kind the longest), and:

- `exports`: "export > to clipboard" (`ExportToClipboard`) for each: the
  clipboard text;
- `png`: "export > to png" (`ExportToPNG`) for one: the `PNGExportPanel`'s
  title and payload description, and the png it writes (kept beside the
  fixture) with the text it carries;
- `imports`: "import > from clipboard" (`ImportFromClipboard`) of each
  formula, of other objects (a string match, a list of two formulae, a
  simple downloader formula), of text that is not an object and of an empty
  clipboard: the messages shown and the formula the panel then holds (as
  its serialised text);
- `png_imports`: "import > from png" (`ImportFromPNG`) of the reference's
  png, of a png that carries a string match, and of a file that is no png.

Usage: QT_QPA_PLATFORM=offscreen ~/pyenv/bin/python oracle/record_formula_exchange.py
       (writes fixtures/formula_exchange.json)
"""
import json, os, shutil, sys, tempfile
HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)

OUT = os.path.join(HERE, 'fixtures', 'formula_exchange.json')
PNG = 'formula_exchange.png'
OTHER_PNG = 'formula_exchange_string_match.png'

KINDS = [27, 31, 59, 60, 133, 136]


def formulas():
    with open(os.path.join(HERE, 'fixtures', 'formulas.json')) as fh:
        cases = json.load(fh)['cases']
    chosen = {}
    for case in cases:
        kind = case['formula'][0]
        text = json.dumps(case['formula'])
        if kind in KINDS and len(text) > len(chosen.get(kind, '')):
            chosen[kind] = text
    return [chosen[k] for k in KINDS if k in chosen]


def record(session):
    c = session.controller
    gui = c.gui

    def f():
        from qtpy import QtWidgets as QW
        from hydrus.core import HydrusCompression, HydrusExceptions, HydrusSerialisable
        from hydrus.client import ClientSerialisable, ClientStrings
        from hydrus.client.gui import ClientGUIDialogsFiles, ClientGUIDialogsMessage, ClientGUITopLevelWindowsPanels
        from hydrus.client.gui.parsing import ClientGUIParsingFormulae as F
        from hydrus.client.parsing import ClientParsing

        clipboard = [None]
        original_pub = c.pub

        def pub(topic, *args, **kwargs):
            if topic == 'clipboard' and args[0] == 'text':
                clipboard[0] = args[1]
                return
            return original_pub(topic, *args, **kwargs)

        c.pub = pub
        c.GetClipboardText = lambda: clipboard[0]
        messages = []
        ClientGUIDialogsMessage.ShowInformation = lambda parent, text, **kwargs: messages.append({'kind': 'information', 'text': text})
        ClientGUIDialogsMessage.ShowWarning = lambda parent, text, **kwargs: messages.append({'kind': 'warning', 'text': text})
        ClientGUIDialogsMessage.ShowCritical = lambda parent, title, text, **kwargs: messages.append({'kind': 'critical', 'title': title, 'text': text})

        texts = formulas()
        test_data = lambda: ClientParsing.ParsingTestData({}, ('',))

        def panel_for(text):
            return F.EditFormulaPanel(gui, HydrusSerialisable.CreateFromString(text), test_data)

        out = {'exports': [], 'imports': [], 'png_imports': []}
        for text in texts:
            panel = panel_for(text)
            clipboard[0] = None
            panel.ExportToClipboard()
            out['exports'].append({'formula': text, 'clipboard': clipboard[0],
                                   'description': panel._formula_description.toPlainText()})

        # to png: the panel it opens, and the file
        pngs = []

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
                started = {'dialog': self.title, 'title': p._title.text(), 'payload_description': p._payload_description.text(),
                           'description': p._text.text(), 'width': p._width.value()}
                with tempfile.TemporaryDirectory() as folder:
                    p._filepicker.SetPath(os.path.join(folder, 'export'))
                    p._Update()
                    p._export.click()
                    written = os.path.join(folder, 'export.png')
                    shutil.copy(written, os.path.join(HERE, 'fixtures', PNG))
                    payload = ClientSerialisable.LoadFromPNG(written)
                pngs.append({**started, 'file': PNG, 'text': HydrusCompression.DecompressBytesToString(payload)})
                return 0

        original_dialog = ClientGUITopLevelWindowsPanels.DialogNullipotent
        F.ClientGUITopLevelWindowsPanels.DialogNullipotent = Dialog
        panel_for(texts[0]).ExportToPNG()
        F.ClientGUITopLevelWindowsPanels.DialogNullipotent = original_dialog
        out['png'] = pngs[0]

        # from the clipboard: into a panel holding the last formula
        string_match = ClientStrings.StringMatch(match_type=ClientStrings.STRING_MATCH_FIXED, match_value='x', example_string='x')
        two = HydrusSerialisable.SerialisableList([HydrusSerialisable.CreateFromString(texts[0]), HydrusSerialisable.CreateFromString(texts[1])])
        simple = ClientParsing.SimpleDownloaderParsingFormula(name='simple', formula=HydrusSerialisable.CreateFromString(texts[0]))
        cases = [(f'a formula ({json.loads(t)[0]})', t) for t in texts] + [
            ('a string match', string_match.DumpToString()),
            ('a list of two formulae', two.DumpToString()),
            ('a simple downloader formula', simple.DumpToString()),
            ('text that is not an object', 'not a formula'),
            ('an empty clipboard', ''),
        ]
        for (name, text) in cases:
            panel = panel_for(texts[-1])
            del messages[:]
            clipboard[0] = text
            panel.ImportFromClipboard()
            value = panel.GetValue()
            out['imports'].append({'name': name, 'text': text, 'messages': list(messages),
                                   'formula': value.DumpToString(),
                                   'description': panel._formula_description.toPlainText()})

        # from a png
        with tempfile.TemporaryDirectory() as folder:
            other = os.path.join(folder, 'other.png')
            ClientSerialisable.DumpToPNG(512, HydrusCompression.CompressStringToBytes(string_match.DumpToString()), 'a string match', '', '', other)
            shutil.copy(other, os.path.join(HERE, 'fixtures', OTHER_PNG))
            bad = os.path.join(folder, 'bad.png')
            with open(bad, 'wb') as fh:
                fh.write(b'not a png')
            picked = [None]

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

                def GetPath(self):
                    return picked[0]

            original_file_dialog = ClientGUIDialogsFiles.FileDialog
            F.ClientGUIDialogsFiles.FileDialog = FileDialog
            for (name, path, file) in [('the reference\'s png', os.path.join(HERE, 'fixtures', PNG), PNG),
                                       ('a png of a string match', other, OTHER_PNG),
                                       ('a file that is no png', bad, None)]:
                panel = panel_for(texts[-1])
                del messages[:]
                picked[0] = path
                panel.ImportFromPNG()
                out['png_imports'].append({'name': name, 'file': file, 'messages': list(messages),
                                           'formula': panel.GetValue().DumpToString()})
            F.ClientGUIDialogsFiles.FileDialog = original_file_dialog
        return out

    return c.CallBlockingToQt(gui, f)


def main():
    import hydrus_driver, record_api
    if len(sys.argv) > 1 and sys.argv[1] == '--child':
        output = sys.argv[2]
        result = hydrus_driver.run_client(record_api.unpack_fixture('basic'), record)
        with open(output, 'w') as fh:
            json.dump(result, fh, ensure_ascii=False)
        return
    with tempfile.TemporaryDirectory() as tmp:
        path = os.path.join(tmp, 'r.json')
        hydrus_driver.run_in_subprocess(os.path.abspath(__file__), '--child', path)
        with open(path) as fh:
            result = json.load(fh)
    with open(OUT, 'w') as fh:
        json.dump(result, fh, indent=1, ensure_ascii=False)
        fh.write('\n')
    print(len(result['exports']), len(result['imports']), len(result['png_imports']))


if __name__ == '__main__':
    main()

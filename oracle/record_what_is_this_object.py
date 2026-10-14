#!/usr/bin/env python3
"""Help > debug > what is this object?, on the real client.

* every registered serialisable type: its name, version and kind (the
  descriptions use the names);
* the real Help QAction opens the window; its title, controls and the picture
  are read back;
* payloads of constructed objects go through the window's own
  `_ImportPayloads` (the same code paste, drop and the PNG chooser reach), and
  the description and JSON boxes are read back; so are errors (bad JSON, a
  future version);
* paste: the clipboard holds JSON text, or a path to a JSON file or to a hydrus
  PNG, and the paste button's handler is run, with the micro-notification text
  and the problem dialogs captured.

Usage: QT_QPA_PLATFORM=offscreen python oracle/record_what_is_this_object.py
"""
import json
import os
import sys
import tempfile
from pathlib import Path

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))


def samples():
    from hydrus.core import HydrusSerialisable as S
    from hydrus.client.gui import ClientGUIShortcuts
    from hydrus.core import HydrusTags  # noqa
    from hydrus.client.metadata import ClientTagsHandling  # noqa
    out = {}

    d = S.SerialisableDictionary()
    d['text'] = 'a'
    d['int'] = 1
    d['float'] = 1.5
    d['bool'] = True
    d['none'] = None
    d['list'] = [1, 2]
    d[3] = 'three'
    out['dictionary of plain values'] = d

    lst = S.SerialisableList([1, 'two', 3.0, None])
    out['list of plain values'] = lst

    inner = S.SerialisableDictionary()
    inner['x'] = 1
    nested = S.SerialisableDictionary()
    nested['inner'] = inner
    nested['inner list'] = S.SerialisableList([1])
    out['dictionary holding serialisables'] = nested

    out['list of serialisables'] = S.SerialisableList([S.SerialisableList([1]), S.SerialisableDictionary(), S.SerialisableList([2])])

    out['empty dictionary'] = S.SerialisableDictionary()
    out['empty list'] = S.SerialisableList()

    b = S.SerialisableBytesDictionary()
    b[b'ab'] = b'cd'
    out['bytes dictionary'] = b

    out['shortcut set'] = ClientGUIShortcuts.ShortcutSet('my shortcuts')

    from hydrus.core import HydrusSerialisable
    from hydrus.client.metadata import ClientTags
    tf = HydrusTags.TagFilter() if hasattr(HydrusTags, 'TagFilter') else None
    if tf is not None:
        out['tag filter'] = tf
    named = S.SerialisableList([ClientGUIShortcuts.ShortcutSet('one'), ClientGUIShortcuts.ShortcutSet('two')])
    out['list of named objects'] = named
    return out


def record(session):
    def drive():
        from qtpy import QtTest as T, QtWidgets as W, QtGui as G
        from hydrus.core import HydrusSerialisable as S
        from hydrus.client import ClientSerialisable
        from hydrus.core import HydrusCompression
        from hydrus.client.gui import ClientGUIFunctions
        from hydrus.client.gui import ClientGUIDialogsMessage
        from hydrus.client.gui.panels import ClientGUISerialisableImport as Imp
        controller = session.controller
        gui = controller.gui

        def find(menu, labels):
            for action in menu.actions():
                if action.text() == labels[0]:
                    return action if len(labels) == 1 else find(action.menu(), labels[1:])
            raise ValueError(labels)

        out = {}
        out['types'] = [dict(type=t, class_name=c.__name__, name=c.SERIALISABLE_NAME, version=c.SERIALISABLE_VERSION,
                             kind=('dict' if issubclass(c, dict) else 'list' if issubclass(c, list) else 'named' if issubclass(c, S.SerialisableBaseNamed) else 'base'))
                        for t, c in sorted(S.SERIALISABLE_TYPES_TO_OBJECT_TYPES.items())]

        menu = None
        before = set(W.QApplication.topLevelWidgets())
        micro, critical = [], []
        orig_micro, orig_crit = ClientGUIFunctions.ShowMicroNotification, ClientGUIDialogsMessage.ShowCritical
        try:
            ClientGUIFunctions.ShowMicroNotification = lambda w, text: micro.append(text)
            ClientGUIDialogsMessage.ShowCritical = lambda parent, title, message, **kw: critical.append(dict(title=title, message=message))
            menu, label = gui._InitialiseMenuInfoHelp()
            action = find(menu, ['debug', 'what is this object?'])
            out['menu_path'] = [label.replace('&', ''), 'debug', action.text()]
            out['tooltip'] = action.toolTip()
            action.trigger()
            T.QTest.qWait(300)
            new = [w for w in W.QApplication.topLevelWidgets() if w not in before and w.isVisible()]
            frame = new[0]
            panel = frame.findChild(Imp.ReviewWhatIsThisObject)
            labels = panel.findChildren(W.QLabel)
            edits = panel.findChildren(W.QTextEdit)
            out['window'] = dict(
                title=frame.windowTitle(), edits=len(edits),
                edit_read_only=[e.isReadOnly() for e in edits],
                pixmaps=[[l.pixmap().width(), l.pixmap().height()] for l in labels if l.pixmap() is not None and not l.pixmap().isNull()],
                paste_tooltip=panel._paste_button.toolTip(),
                initial=[e.toPlainText() for e in edits])

            def run(payloads):
                panel._ImportPayloads(payloads)
                return dict(payloads=[text for (_, text) in payloads], description=panel._description.toPlainText(),
                            json_len=len(panel._json_content.toPlainText()), json=panel._json_content.toPlainText())

            out['objects'] = {}
            for name, obj in samples().items():
                text = obj.DumpToString()
                result = run([(f'JSON "{name}"', text)])
                out['objects'][name] = result

            bad = {}
            bad['not json'] = run([('JSON "x"', 'this is not json')])
            bad['empty'] = run([('JSON "x"', '')])
            bad['a json list'] = run([('JSON "x"', '[1, 2, 3]')])
            bad['two payloads'] = run([('JSON "a"', S.SerialisableList([1]).DumpToString()), ('JSON "b"', 'nope')])
            bad['nothing'] = run([])
            future = json.loads(S.SerialisableList([1]).DumpToString())
            future[1] = 9999
            bad['future version'] = run([('JSON "x"', json.dumps(future))])
            for name, text in [('truncated list', '[1, 2'), ('missing colon', '{"a" 1}'), ('bare key', '{a: 1}'),
                               ('missing comma', '[1 2]'), ('extra data', '[1] x'), ('unterminated string', '"abc'),
                               ('missing value', '{"a": }'), ('whitespace only', '   '), ('a json object', '{"a": 1}'),
                               ('a number', '5'), ('a string', '"str"'), ('null', 'null'), ('true', 'true'),
                               ('type 21 only', '[21]'), ('type 21 and version', '[21, 2]'), ('unknown type', '[99999, 1, []]'),
                               ('nan', '[NaN]'), ('two lines', '[1,\n2'), ('unicode before', '"\u00e9" x'),
                               ('control character', '"a\tb"'), ('bad escape', '"\\q"')]:
                bad[name] = run([('JSON "x"', text)])
            out['errors'] = bad

            # paste: text, a path to JSON, a path to a hydrus png
            clip = W.QApplication.clipboard()
            tmp = tempfile.mkdtemp()
            json_path = os.path.join(tmp, 'object.json')
            text = S.SerialisableList([1, 2]).DumpToString()
            Path(json_path).write_text(text, encoding='utf-8')
            png_path = os.path.join(tmp, 'object.png')
            payload = HydrusCompression.CompressStringToBytes(text)
            ClientSerialisable.DumpToPNG(1024, payload, 'a title', 'a description', 'some text', png_path)
            pastes = []
            for name, setter in [
                ('json text', lambda: clip.setText(text)),
                ('path to a json file', lambda: clip.setText(json_path)),
                ('url to a json file', lambda: _set_urls(clip, [json_path])),
                ('url to a png', lambda: _set_urls(clip, [png_path])),
                ('two urls', lambda: _set_urls(clip, [json_path, png_path])),
                ('missing path', lambda: _set_urls(clip, [os.path.join(tmp, 'missing.json')])),
                ('garbage text', lambda: clip.setText('garbage')),
            ]:
                setter()
                micro.clear()
                critical.clear()
                panel._description.setText('')
                panel._Paste()
                T.QTest.qWait(80)
                pastes.append(dict(case=name, description=panel._description.toPlainText(),
                                   json=panel._json_content.toPlainText(), micro=list(micro), critical=list(critical)))
            out['pastes'] = pastes
            out['paste_payload'] = text

            # the PNG chooser's and the drop's path: the same _ImportPaths
            critical.clear()
            panel._ImportPaths([json_path])
            out['import_paths_json'] = dict(description=panel._description.toPlainText(), critical=list(critical))
            panel._ImportPaths([png_path])
            out['import_paths_png'] = dict(description=panel._description.toPlainText(), critical=list(critical))
            bad_path = os.path.join(tmp, 'binary.json')
            Path(bad_path).write_bytes(b'\xff\xfe\x00bad')
            critical.clear()
            panel._ImportPaths([bad_path])
            out['import_paths_not_utf8'] = dict(critical=list(critical))
            critical.clear()
            panel._ImportPaths([os.path.join(tmp, 'absent.json')])
            out['import_paths_absent'] = dict(critical=[dict(c, message=c['message'].replace(tmp, '<dir>')) for c in critical])
            for w in new:
                w.close()
            return out
        finally:
            ClientGUIFunctions.ShowMicroNotification = orig_micro
            ClientGUIDialogsMessage.ShowCritical = orig_crit
            if menu is not None:
                menu.deleteLater()
            W.QApplication.processEvents()

    def _set_urls(clip, paths):
        from qtpy import QtCore as C
        mime = C.QMimeData()
        mime.setUrls([C.QUrl.fromLocalFile(p) for p in paths])
        clip.setMimeData(mime)

    import builtins
    builtins._set_urls = _set_urls
    return session.controller.CallBlockingToQt(session.controller.gui, drive)


def main():
    import hydrus_driver
    import record_api
    if len(sys.argv) > 1 and sys.argv[1] == '--child':
        database = record_api.unpack_fixture('basic')
        Path(sys.argv[2]).write_text(json.dumps(hydrus_driver.run_client(database, record)))
        return
    with tempfile.TemporaryDirectory() as directory:
        output = Path(directory) / 'result.json'
        hydrus_driver.run_in_subprocess(str(Path(__file__).resolve()), '--child', str(output))
        result = json.loads(output.read_text())
    (HERE / 'fixtures/what_is_this_object.json').write_text(json.dumps(result, indent=2) + '\n')
    print('wrote what_is_this_object.json')


if __name__ == '__main__':
    main()

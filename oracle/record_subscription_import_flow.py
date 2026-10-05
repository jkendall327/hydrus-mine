#!/usr/bin/env python3
"""Actual Qt subscription clipboard/package and ordered multi-file imports.

Known wrong object types warn after valid objects are added. Generic file errors
stop only that file sequence; serialisation/future errors skip later similar
messages but continue. All subscriptions and files belong to private fixtures.
"""
import json
import sys
import tempfile
from pathlib import Path
HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
NOW = 1_700_000_000


def record(session):
    def qt():
        from hydrus.core import HydrusSerialisable as S, HydrusTime as T
        from hydrus.client.gui import ClientGUISubscriptions as G, ClientGUIDialogsMessage as M, ClientGUIDialogsQuick as D
        from hydrus.client.importing import ClientImportSubscriptionQuery as Q
        from hydrus.client import ClientSerialisable as PNG
        c = session.controller
        source = json.loads((HERE / 'fixtures/subscription_exchange.json').read_text())['single']
        a = S.CreateFromSerialisableTuple(source)
        b = S.CreateFromSerialisableTuple(source)
        b.subscription.SetName('Second')
        checker = a.subscription.GetCheckerOptions()
        mixed = S.SerialisableList([a, S.SerialisableList([checker, b]), checker, 'wrong string'])
        missing = S.CreateFromSerialisableTuple(source)
        missing.subscription.SetName('Incomplete')
        missing.query_log_containers = S.SerialisableList()
        missing_package = S.SerialisableList([a, missing, b])
        future = list(checker.GetSerialisableTuple())
        future[1] += 1
        panel = None
        messages = []
        counter = [0]
        def fresh():
            counter[0] += 1
            return f'{counter[0]:064x}'
        def rows():
            return [s.GetName() for s in panel._subscriptions.GetData()]
        def message(kind, text, title=None):
            messages.append(dict(kind=kind, title=title, text=text, during=rows(), selected=[s.GetName() for s in panel._subscriptions.GetData(only_selected=True)]))
        old = (T.GetNow, Q.GenerateQueryLogContainerName, c.GetClipboardText, c.ClipboardHasImage, M.ShowInformation, M.ShowWarning, M.ShowCritical, D.PresentClipboardParseError, D.GetYesNo)
        T.GetNow = lambda: NOW
        Q.GenerateQueryLogContainerName = fresh
        c.ClipboardHasImage = lambda: False
        M.ShowInformation = lambda owner, text: message('information', text)
        M.ShowWarning = lambda owner, text: message('warning', text)
        M.ShowCritical = lambda owner, title, text: message('critical', text, title)
        D.PresentClipboardParseError = lambda owner, text, expected, error: message('parse', expected, 'clipboard parse')
        def reject_missing(owner, text, title='', **kwargs):
            message('question', text, title)
            return __import__('qtpy.QtWidgets', fromlist=['QDialog']).QDialog.DialogCode.Rejected
        D.GetYesNo = reject_missing
        cases = []
        try:
            with tempfile.TemporaryDirectory() as directory:
                root = Path(directory)
                sources = {'a': a.DumpToString(), 'b': b.DumpToString(), 'mixed': mixed.DumpToString(), 'missing': missing_package.DumpToString(), 'wrong': checker.DumpToString(), 'bad': 'not json', 'future': json.dumps(future), 'absent': None}
                for name, text in sources.items():
                    if text is None:
                        continue
                    (root / f'{name}.json').write_text(text)
                    if name == 'bad':
                        (root / f'{name}.png').write_bytes(b'not png')
                    else:
                        obj = S.CreateFromSerialisableTuple(json.loads(text)) if name != 'future' else None
                        payload = obj.DumpToNetworkBytes() if obj else __import__('zlib').compress(text.encode())
                        PNG.DumpToPNG(512, payload, 'synthetic subscription import', 'subscriptions', '', str(root / f'{name}.png'))
                for name, mode, inputs in [
                    ('mixed clipboard', 'clipboard', ['mixed']), ('wrong clipboard', 'clipboard', ['wrong']),
                    ('missing reject clipboard', 'clipboard', ['missing']),
                    ('json prefix before broken file', 'json', ['a', 'bad', 'b']),
                    ('json future errors continue once', 'json', ['a', 'future', 'future', 'b']),
                    ('json types continue', 'json', ['wrong', 'a', 'mixed']),
                    ('json prefix before unreadable file', 'json', ['a', 'absent', 'b']),
                    ('png prefix before broken file', 'png', ['a', 'bad', 'b']),
                    ('png future errors continue once', 'png', ['a', 'future', 'future', 'b']),
                    ('png types continue', 'png', ['wrong', 'a', 'mixed']),
                    ('png prefix before unreadable file', 'png', ['a', 'absent', 'b']),
                ]:
                    panel = G.EditSubscriptionsPanel(c.gui, [])
                    messages.clear()
                    try:
                        control = panel._subscriptions_panel
                        if mode == 'clipboard':
                            c.GetClipboardText = lambda: sources[inputs[0]]
                            control._ImportFromClipboard()
                        elif mode == 'json':
                            control._ImportJSONs([str(root / f'{key}.json') for key in inputs])
                        else:
                            control._ImportPNGs([str(root / f'{key}.png') for key in inputs])
                        selected = [s.GetName() for s in panel._subscriptions.GetData(only_selected=True)]
                        panel._subscriptions.SelectDatas(panel._subscriptions.GetData())
                        containers = panel._GetSelectedSubsAsExportableContainers() or []
                        if not isinstance(containers, (list, S.SerialisableList)):
                            containers = [containers]
                        cases.append(dict(case=name, mode=mode, inputs=inputs, sources={key:sources[key] for key in inputs}, messages=list(messages), names=rows(), selected=selected, exported=[json.loads(obj.DumpToString()) for obj in containers]))
                    finally:
                        panel.deleteLater()
            return dict(now=NOW, cases=cases, type_names={str(code):kind.__name__ for code,kind in sorted(S.SERIALISABLE_TYPES_TO_OBJECT_TYPES.items())})
        finally:
            T.GetNow, Q.GenerateQueryLogContainerName, c.GetClipboardText, c.ClipboardHasImage, M.ShowInformation, M.ShowWarning, M.ShowCritical, D.PresentClipboardParseError, D.GetYesNo = old
    return session.controller.CallBlockingToQt(session.controller.gui, qt)


def main():
    import hydrus_driver
    if len(sys.argv) > 1:
        import record_api
        output = Path(sys.argv[2])
        result = hydrus_driver.run_client(record_api.unpack_fixture('basic'), record)
        output.write_text(json.dumps(result))
        return
    with tempfile.TemporaryDirectory() as directory:
        output = Path(directory) / 'result.json'
        hydrus_driver.run_in_subprocess(str(Path(__file__).resolve()), '--child', str(output))
        result = json.loads(output.read_text())
    (HERE / 'fixtures/subscription_import_flow.json').write_text(json.dumps(result, indent=2) + '\n')
    (HERE.parent / 'crates/hydrus-downloader-exchange/src/subscription_type_names.json').write_text(json.dumps(result['type_names'], indent=2) + '\n')


if __name__ == '__main__':
    main()

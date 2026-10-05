#!/usr/bin/env python3
"""Record real router-list permitted subsets and ordered multiple-PNG imports.

Uses the existing six real router definitions, real serialisable objects and PNG
transport. Only message dialogs, clipboard and file selection are scripted.
"""
import json
import os
import sys
import tempfile

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)


def record(session):
    c = session.controller

    def work():
        from qtpy import QtWidgets as QW
        from hydrus.client import ClientSerialisable as PNG, ClientStrings as S
        from hydrus.client.gui import ClientGUIDialogsFiles as F, ClientGUIDialogsMessage as D
        from hydrus.client.gui.metadata import ClientGUIMetadataMigration as G, ClientGUIMetadataMigrationTest as T
        from hydrus.client.metadata import ClientMetadataMigrationImporters as I, ClientMetadataMigrationExporters as E
        from hydrus.core import HydrusSerialisable as HS

        with open(os.path.join(HERE, 'fixtures/router_exchange.json')) as f:
            old = json.load(f)
        imports = [HS.CreateFromSerialisableTuple(v) for v in old['imports']]
        exports = [HS.CreateFromSerialisableTuple(v) for v in old['exports']]
        cases = []
        messages = []
        paths = []
        dialogs = []
        old_clip, old_warn, old_info, old_critical, old_dialog = c.GetClipboardText, D.ShowWarning, D.ShowInformation, D.ShowCritical, F.FileDialog

        class Dialog:
            def __init__(self, owner, title, **kwargs):
                dialogs.append({'title': title, 'multiple': kwargs['fileMode'] == QW.QFileDialog.FileMode.ExistingFiles, 'wildcard': kwargs['wildcard']})
            def __enter__(self): return self
            def __exit__(self, *args): return False
            def exec(self): return QW.QDialog.DialogCode.Accepted
            def GetPaths(self): return paths

        def control(context):
            if context == 'import':
                return G.SingleFileMetadataRoutersControl(c.gui, [], [I.SingleFileMetadataImporterTXT, I.SingleFileMetadataImporterJSON], [E.SingleFileMetadataExporterMediaTags, E.SingleFileMetadataExporterMediaNotes, E.SingleFileMetadataExporterMediaURLs, E.SingleFileMetadataExporterMediaTimestamps], T.MigrationTestContextFactorySidecar([]))
            return G.SingleFileMetadataRoutersControl(c.gui, [], [I.SingleFileMetadataImporterMediaTags, I.SingleFileMetadataImporterMediaNotes, I.SingleFileMetadataImporterMediaURLs, I.SingleFileMetadataImporterMediaTimestamps], [E.SingleFileMetadataExporterTXT, E.SingleFileMetadataExporterJSON], T.MigrationTestContextFactoryMedia([]))

        D.ShowWarning = lambda owner, text: messages.append(['warning', text])
        D.ShowInformation = lambda owner, text: messages.append(['information', text])
        D.ShowCritical = lambda owner, title, text: messages.append(['critical', title, text])
        F.FileDialog = Dialog
        try:
            for context, good, wrong in [('import', imports, exports), ('export', exports, imports)]:
                for name, objects in [('mixed', [good[0], S.StringMatch(), wrong[0], good[-1], S.StringMatch()]), ('nested', [HS.SerialisableList([good[-1], wrong[0]]), good[0]]), ('wrong_only', [wrong[0], S.StringMatch()])]:
                    box = control(context)
                    payload = HS.SerialisableList(objects).DumpToString()
                    c.GetClipboardText = lambda: payload
                    messages.clear()
                    box._ImportFromClipboard()
                    cases.append({'name': name, 'context': context, 'text': json.loads(payload), 'added': [r.GetSerialisableTuple() for r in box.GetData()], 'selected': len(box.GetData(only_selected=True)), 'messages': list(messages)})
                    box.deleteLater()
            with tempfile.TemporaryDirectory() as directory:
                first, second, broken, trailing = [os.path.join(directory, n + '.png') for n in ['first', 'second', 'broken', 'trailing']]
                for path, obj in [(first, imports[-1]), (second, HS.SerialisableList([exports[0], imports[0], S.StringMatch()])), (trailing, imports[1])]:
                    PNG.DumpToPNG(512, obj.DumpToNetworkBytes(), 'router import', 'synthetic routers', '', path)
                # Valid transport but unparseable object: stable real critical message.
                PNG.DumpToPNG(512, b'not a compressed hydrus object', 'bad object', '', '', broken)
                for name, chosen in [('ordered_pngs', [first, second, trailing]), ('later_failure', [first, broken, trailing]), ('cancelled_picker', [])]:
                    box = control('import')
                    paths[:] = chosen
                    messages.clear()
                    box._ImportFromPNG()
                    cases.append({'name': name, 'context': 'import', 'files': [os.path.basename(p) for p in chosen], 'added': [r.GetSerialisableTuple() for r in box.GetData()], 'selected': len(box.GetData(only_selected=True)), 'messages': list(messages)})
                    if name == 'ordered_pngs':
                        box.resize(1000, 480); box.show(); QW.QApplication.processEvents()
                        box.grab().save(os.path.join(HERE, 'fixtures/router_import_editor.png'))
                    box.deleteLater()
                for path in [first, second, broken, trailing]:
                    with open(path, 'rb') as f:
                        data = f.read()
                    with open(os.path.join(HERE, 'fixtures/router_import_' + os.path.basename(path)), 'wb') as f:
                        f.write(data)
        finally:
            c.GetClipboardText, D.ShowWarning, D.ShowInformation, D.ShowCritical, F.FileDialog = old_clip, old_warn, old_info, old_critical, old_dialog
        return {'cases': cases, 'dialogs': dialogs, 'object_types': {str(k): v.__name__ for k, v in HS.SERIALISABLE_TYPES_TO_OBJECT_TYPES.items()}}

    return c.CallBlockingToQt(c.gui, work)


def main():
    import hydrus_driver
    if len(sys.argv) > 1 and sys.argv[1] == '--child':
        import record_api
        output = sys.argv[2]
        result = hydrus_driver.run_client(record_api.unpack_fixture('basic'), record)
        with open(output, 'w') as f: json.dump(result, f)
        return
    with tempfile.TemporaryDirectory() as directory:
        output = os.path.join(directory, 'result.json')
        hydrus_driver.run_in_subprocess(os.path.abspath(__file__), '--child', output)
        with open(output) as f: result = json.load(f)
    with open(os.path.join(HERE, 'fixtures/router_import.json'), 'w') as f:
        json.dump(result, f, indent=1, ensure_ascii=False); f.write('\n')
    print('wrote router_import.json')


if __name__ == '__main__':
    main()

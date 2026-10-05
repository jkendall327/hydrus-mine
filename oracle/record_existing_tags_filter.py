#!/usr/bin/env python3
"""Real per-service existing-tag filter dialogs and saved GetTags consumers.

Qt timers answer actual modal dialogs; business handlers, tag-filter editor,
serialization and filter_existing_tags DB reads are unchanged. Synthetic tags
on fixture files distinguish service counts and parsed/additional candidates.
"""
import json
import sys
import tempfile
from pathlib import Path

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))


def record(session):
    from qtpy import QtCore as QC, QtWidgets as QW
    from hydrus.core import HydrusConstants as HC, HydrusSerialisable
    from hydrus.client import ClientConstants as CC
    from hydrus.client.gui.importing import ClientGUIImportOptionsPanels as P
    from hydrus.client.importing.options import TagImportOptions as T
    from hydrus.client.metadata import ClientContentUpdates as U

    c = session.controller
    services = {s.GetName(): s.GetServiceKey() for s in c.services_manager.GetServices((HC.LOCAL_TAG,))}
    key, second = services['my tags'], services['second tags']
    manifest = json.loads((HERE / 'fixtures/legacy_db/basic.manifest.json').read_text())
    hashes = [bytes.fromhex(f['hash']) for f in manifest['files'][:2]]
    known = ['parity existing plain', 'parity added plain']
    updates = [U.ContentUpdate(HC.CONTENT_TYPE_MAPPINGS, HC.CONTENT_UPDATE_ADD, (tag, {hashes[0]})) for tag in known]
    c.WriteSynchronous('content_updates', U.ContentUpdatePackage.STATICCreateFromContentUpdates(key, updates))
    media = c.Read('media_results', [hashes[1]])[0]
    parsed = ['parity existing plain', 'parity new plain', 'creator:fresh']
    additional = ['parity added plain', 'parity absent added', 'series:added']

    def drive():
        panels = []
        events = []

        def snapshot(value):
            data = value.ToTuple()
            return dict(enabled=data[6], rules=sorted([list(pair) for pair in data[7].GetTagSlicesToRules().items()]))

        def edit(owner, accept, namespace):
            before = snapshot(owner.GetValue())
            captured = {}
            timer = QC.QTimer(c.gui)

            def answer():
                dialog = QW.QApplication.activeModalWidget()
                if dialog is None or dialog.windowTitle() != 'edit already-exist filter':
                    return
                timer.stop()
                panel = dialog._panel
                captured.update(title=dialog.windowTitle(), tabs=[panel._notebook.tabText(i) for i in range(panel._notebook.count())], before=snapshot(owner.GetValue()))
                # Drive the real whitelist checkbox event, not a replacement filter.
                index = 1 if namespace == '' else 0
                row = panel._simple_whitelist_global_checkboxes.model().index(index, 0)
                panel.EventSimpleWhitelistGlobalCheck(row)
                captured['edited_rules'] = sorted([list(pair) for pair in panel.GetValue().GetTagSlicesToRules().items()])
                dialog.resize(900, 760)
                QW.QApplication.processEvents()
                assert dialog.grab().save(str(HERE / 'fixtures/existing_tags_filter.png'))
                dialog.done(QW.QDialog.DialogCode.Accepted if accept else QW.QDialog.DialogCode.Rejected)

            timer.timeout.connect(answer)
            timer.start(10)
            owner._EditOnlyAddExistingTagsFilter()
            timer.deleteLater()
            events.append(dict(accepted=accept, before=before, dialog=captured, after=snapshot(owner.GetValue())))

        initial = T.ServiceTagImportOptions(get_tags=True, additional_tags=additional)
        panel = P.EditServiceTagImportOptionsPanel(c.gui, key, initial)
        panels.append(panel)
        edit(panel, False, '')
        assert snapshot(panel.GetValue()) == snapshot(initial)
        edit(panel, True, '')
        saved = panel.GetValue()
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / 'saved-service.json'
            path.write_text(json.dumps(saved.GetSerialisableTuple()))
            restored = HydrusSerialisable.CreateFromSerialisableTuple(json.loads(path.read_text()))
        reopened = P.EditServiceTagImportOptionsPanel(c.gui, key, restored)
        panels.append(reopened)
        result = dict(events=events, initial=snapshot(initial), saved=snapshot(saved), reopened=snapshot(reopened.GetValue()), saved_tuple=restored.GetSerialisableTuple(), known=known, parsed=parsed, additional=additional, consumer=sorted(restored.GetTags(key, CC.STATUS_SUCCESSFUL_AND_NEW, media, parsed)), second_service_consumer=sorted(restored.GetTags(second, CC.STATUS_SUCCESSFUL_AND_NEW, media, parsed)))
        reopened._only_add_existing_tags = False
        result['disabled_consumer'] = sorted(reopened.GetValue().GetTags(key, CC.STATUS_SUCCESSFUL_AND_NEW, media, parsed))
        for panel in panels:
            panel.deleteLater()
        return result

    return c.CallBlockingToQt(c.gui, drive)


def main():
    import hydrus_driver
    import record_api
    if len(sys.argv) > 1 and sys.argv[1] == '--child':
        Path(sys.argv[2]).write_text(json.dumps(hydrus_driver.run_client(record_api.unpack_fixture('basic'), record)))
        return
    with tempfile.TemporaryDirectory() as directory:
        result = Path(directory) / 'result.json'
        hydrus_driver.run_in_subprocess(str(Path(__file__).resolve()), '--child', str(result))
        data = json.loads(result.read_text())
    (HERE / 'fixtures/existing_tags_filter.json').write_text(json.dumps(data, indent=2) + '\n')
    print('wrote existing_tags_filter.json')


if __name__ == '__main__':
    main()

#!/usr/bin/env python3
"""Record real ReviewExportFilesPanel previews, collisions, removal and questions.

A private basic-fixture client constructs the actual Qt panel over three local
media, changes filename phrases, removes one selected row, disables links by
checking trash, and records cancelled destructive/close export confirmations.
No source files are deleted; destructive confirmations answer no. The real
reference export worker then copies the three files into a private temporary
folder over an existing destination, recording output names and SHA-256s.
"""
import json
import hashlib
import os
import sys
import tempfile
HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
import hydrus_driver
import record_api


def record(session):
    controller = session.controller
    gui = controller.GetMainTLW()
    destination = tempfile.TemporaryDirectory(prefix="manual-export-reference-")
    workers = []
    def qt():
        from qtpy import QtWidgets as QW
        from hydrus.client import ClientConstants as CC
        from hydrus.core import HydrusConstants as HC
        from hydrus.client.gui import ClientGUIDialogsQuick, ClientGUITopLevelWindowsPanels
        from hydrus.client.gui.exporting import ClientGUIExport
        from hydrus.client.media import ClientMediaSingle
        candidates = controller.Read('media_results_from_ids', [1, 8, 3])
        local = [m for m in candidates if m.GetLocationsManager().IsLocal() and not m.GetLocationsManager().IsTrashed()]
        pngs = [m for m in local if HC.mime_ext_lookup.get(m.GetMime()) == '.png']
        results = pngs[:2] + [next(m for m in local if m.GetMime() == HC.AUDIO_FLAC)]
        media = [ClientMediaSingle.MediaSingle(m) for m in results]
        frame = ClientGUITopLevelWindowsPanels.FrameThatTakesScrollablePanel(gui, 'export files')
        controller.options['export_path'] = '/tmp/hydrus-manual-export-oracle'
        panel = ClientGUIExport.ReviewExportFilesPanel(frame, media)
        panel._directory_picker.SetPath('/tmp/hydrus-manual-export-oracle')
        out = {'files': [{'file_id': m.GetHashId(), 'mime': m.GetMime(), 'hash': m.GetHash().hex()} for m in media], 'previews': []}
        for phrase in ['same', '{#} {file_id}', '[creator] - {hash}', '../escape', '{bad']:
            panel._pattern.setText(phrase)
            panel._RefreshPaths()
            out['previews'].append({'phrase': phrase, 'rows': [list(panel._ConvertDataToDisplayTuple(m)) for m in media]})
        said = []
        original = ClientGUIDialogsQuick.GetYesNo
        def no(parent, message, **kwargs):
            said.append(message)
            return QW.QDialog.DialogCode.Rejected
        ClientGUIDialogsQuick.GetYesNo = no
        try:
            panel._pattern.setText('same')
            panel._RefreshPaths()
            panel._export_symlinks.setChecked(True)
            panel._delete_files_after_export.setChecked(True)
            panel.EventDeleteFilesChanged()
            out['symlinks_with_trash'] = panel._export_symlinks.isChecked()
            panel._DoExport()
            panel._DoExport(True)
            panel._delete_files_after_export.setChecked(False)
            panel._DoExport(True)
            out['questions'] = list(said)
            real_thread = controller.CallToThread
            def capture(function, *args, **kwargs):
                if function.__name__ == 'do_it':
                    workers.append((function, args, kwargs))
                else:
                    real_thread(function, *args, **kwargs)
            controller.CallToThread = capture
            try:
                panel._directory_picker.SetPath(destination.name)
                panel._pattern.setText('same')
                panel._RefreshPaths()
                with open(os.path.join(destination.name, 'same.png'), 'wb') as f:
                    f.write(b'previous destination')
                panel._DoExport()
            finally:
                controller.CallToThread = real_thread
            panel._directory_picker.SetPath('/tmp/hydrus-manual-export-oracle')
            panel._RefreshPaths()
            ClientGUIDialogsQuick.GetYesNo = lambda parent, message, **kwargs: (said.append(message), QW.QDialog.DialogCode.Accepted)[1]
            panel._paths.SelectDatas([media[1]], deselect_others=True)
            panel._DeletePaths()
            out['remove_question'] = said[-1]
            out['removed_rows'] = [list(panel._ConvertDataToDisplayTuple(m)) for m in panel._paths.GetData()]
            panel._pattern.setText('{#}')
            panel._RefreshPaths()
            out['renumbered_rows'] = [list(panel._ConvertDataToDisplayTuple(m)) for m in panel._paths.GetData()]
        finally:
            ClientGUIDialogsQuick.GetYesNo = original
            frame.close()
        return out
    result = controller.CallBlockingToQt(gui, qt)
    try:
        for function, args, kwargs in workers:
            function(*args, **kwargs)
        result['copies'] = {}
        for name in sorted(os.listdir(destination.name)):
            with open(os.path.join(destination.name, name), 'rb') as f:
                result['copies'][name] = hashlib.sha256(f.read()).hexdigest()
        return result
    finally:
        destination.cleanup()


def main():
    if len(sys.argv) > 1 and sys.argv[1] == '--child':
        output = sys.argv[2]
        db = record_api.unpack_fixture('basic')
        result = hydrus_driver.run_client(db, record)
        with open(output, 'w') as f:
            json.dump(result, f)
        return
    with tempfile.TemporaryDirectory() as work:
        path = os.path.join(work, 'export_files.json')
        hydrus_driver.run_in_subprocess(os.path.abspath(__file__), '--child', path)
        with open(path) as f:
            result = json.load(f)
    with open(os.path.join(HERE, 'fixtures', 'export_files.json'), 'w') as f:
        json.dump(result, f, indent=1)
        f.write('\n')
    print('wrote export_files.json')

if __name__ == '__main__':
    main()

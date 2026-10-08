#!/usr/bin/env python3
"""Record real manual-export worker failure/cancel and successful-prefix deletion.

Each case uses a fresh private basic database and temporary destination. The
actual Qt export handler constructs its worker; only worker scheduling is held
until the recording thread runs it. Missing source files cause real I/O errors.
MirrorFile is observed unchanged; selected completed copies cancel the real
JobStatus to fix the cancellation boundary. Questions/errors are recorded,
while actual synchronous content updates and database reads remain active.
"""
import json
import sqlite3
import sys
import tempfile
from pathlib import Path

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
import hydrus_driver
import record_api


def record(session, case):
    from qtpy import QtWidgets as W
    from hydrus.core import HydrusPaths
    from hydrus.core import HydrusConstants as HC
    from hydrus.client import ClientConstants as CC
    from hydrus.client.metadata import ClientContentUpdates as U
    from hydrus.client.metadata import ClientMetadataMigration as R
    from hydrus.client.metadata import ClientMetadataMigrationImporters as I
    from hydrus.client.metadata import ClientMetadataMigrationExporters as E
    from hydrus.client.gui import ClientGUIDialogsQuick as D
    from hydrus.client.gui import ClientGUIDialogsMessage as M
    from hydrus.client.gui import ClientGUITopLevelWindowsPanels as F
    from hydrus.client.gui.exporting import ClientGUIExport
    from hydrus.client.media import ClientMediaSingle

    controller = session.controller
    gui = controller.GetMainTLW()
    workers, jobs, copied, errors, questions, deletions = [], [], [], [], [], []
    old_thread, old_pub = controller.CallToThread, controller.pub
    old_write = controller.WriteSynchronous
    old_question, old_error, old_mirror = D.GetYesNo, M.ShowCritical, HydrusPaths.MirrorFile
    with tempfile.TemporaryDirectory(prefix='export-prefix-') as directory:
        def capture(fn, *args, **kwargs):
            if fn.__name__ == 'do_it':
                workers.append((fn, args, kwargs))
            else:
                return old_thread(fn, *args, **kwargs)

        def pub(topic, *args, **kwargs):
            if topic == 'message' and args[0].GetStatusTitle() == 'file export':
                jobs.append(args[0])
            return old_pub(topic, *args, **kwargs)

        def question(parent, message, **kwargs):
            questions.append(message)
            return W.QDialog.DialogCode.Accepted

        def mirror(source, destination, *args, **kwargs):
            result = old_mirror(source, destination, *args, **kwargs)
            copied.append(Path(destination).name)
            if (case == 'cancel_after_first' and len(copied) == 1 or
                    case == 'cancel_after_last' and len(copied) == 3):
                jobs[-1].Cancel()
            return result

        def write(command, *args, **kwargs):
            result = old_write(command, *args, **kwargs)
            if command == 'content_updates':
                for key, updates in args[0].IterateContentUpdates():
                    for update in updates:
                        if key == CC.COMBINED_LOCAL_FILE_DOMAINS_SERVICE_KEY and update.GetAction() == HC.CONTENT_UPDATE_DELETE:
                            deletions.append(dict(count=len(update.GetHashes()), reason=update.GetReason().replace(directory, '<destination>')))
            return result

        def prepare():
            controller.options['export_path'] = directory
            media = [ClientMediaSingle.MediaSingle(m) for m in
                     controller.Read('media_results_from_ids', [1, 8, 3])]
            if case == 'sidecar_second':
                for m in media:
                    old_write('content_updates', U.ContentUpdatePackage.STATICCreateFromContentUpdate(
                        CC.LOCAL_NOTES_SERVICE_KEY,
                        U.ContentUpdate(HC.CONTENT_TYPE_NOTES, HC.CONTENT_UPDATE_SET, (m.GetHash(), 'test', 'recorded note'))))
                media = [ClientMediaSingle.MediaSingle(m) for m in controller.Read('media_results_from_ids', [1, 8, 3])]
            controller.new_options.SetBoolean('delete_lock_for_archived_files', True)
            frame = F.FrameThatTakesScrollablePanel(gui, 'export files')
            panel = ClientGUIExport.ReviewExportFilesPanel(frame, media)
            frame.SetPanel(panel)
            panel._directory_picker.SetPath(directory)
            panel._pattern.setText('{#}')
            panel._delete_files_after_export.setChecked(True)
            panel.EventDeleteFilesChanged()
            panel._RefreshPaths()
            ordered = panel._paths.GetData()
            if case == 'sidecar_second':
                router = R.SingleFileMetadataRouter(importers=[I.SingleFileMetadataImporterMediaNotes()], exporter=E.SingleFileMetadataExporterTXT())
                panel._metadata_routers_button.SetValue([router])
                Path(panel._GetPath(ordered[1]) + '.txt').mkdir()
            if case in ('fail_first', 'fail_second'):
                failed = ordered[0 if case == 'fail_first' else 1]
                Path(controller.client_files_manager.GetFilePath(failed.GetHash(), failed.GetMime())).unlink()
            panel._DoExport()
            return frame, panel, [m.GetHashId() for m in ordered], [not m.GetLocationsManager().inbox for m in ordered]

        try:
            controller.CallToThread, controller.pub, controller.WriteSynchronous = capture, pub, write
            D.GetYesNo = question
            M.ShowCritical = lambda parent, title, message: errors.append(dict(title=title, message=message))
            HydrusPaths.MirrorFile = mirror
            frame, panel, ids, archived = controller.CallBlockingToQt(gui, prepare)
            assert len(workers) == 1
            fn, args, kwargs = workers.pop()
            fn(*args, **kwargs)
            states = controller.Read('media_results_from_ids', ids)
            by_id = {m.GetHashId(): m for m in states}
            result = dict(case=case, ids=ids, copied=copied,
                          trashed=[by_id[i].GetLocationsManager().IsTrashed() for i in ids],
                          cancelled=jobs[-1].IsCancelled(), done=jobs[-1].IsDone(),
                          questions=questions, error_count=len(errors),
                          error_titles=[e['title'] for e in errors], deletions=deletions,
                          archived=archived)
            def finish():
                if case == 'fail_second':
                    panel.grab().save(str(HERE / 'fixtures/export_failure_prefix.png'))
                frame.close()
            controller.CallBlockingToQt(gui, finish)
            return result
        finally:
            controller.CallToThread, controller.pub = old_thread, old_pub
            controller.WriteSynchronous = old_write
            D.GetYesNo, M.ShowCritical, HydrusPaths.MirrorFile = old_question, old_error, old_mirror


def main():
    if len(sys.argv) > 1 and sys.argv[1] == '--child':
        output, case = Path(sys.argv[2]), sys.argv[3]
        database = record_api.unpack_fixture('basic')
        result = hydrus_driver.run_client(database, lambda session: record(session, case))
        from hydrus.client import ClientConstants as CC
        with sqlite3.connect(str(Path(database) / 'client.db')) as conn:
            trash_id = conn.execute('SELECT service_id FROM services WHERE service_key=?', (CC.TRASH_SERVICE_KEY,)).fetchone()[0]
            current = {row[0] for row in conn.execute(f'SELECT hash_id FROM current_files_{trash_id}')}
        result['durable_trashed'] = [i in current for i in result['ids']]
        assert result['durable_trashed'] == result['trashed']
        output.write_text(json.dumps(result))
        return
    cases = []
    with tempfile.TemporaryDirectory() as work:
        for case in ['success', 'fail_first', 'fail_second', 'sidecar_second', 'cancel_after_first', 'cancel_after_last']:
            output = Path(work) / (case + '.json')
            hydrus_driver.run_in_subprocess(str(Path(__file__).resolve()), '--child', str(output), case)
            cases.append(json.loads(output.read_text()))
    (HERE / 'fixtures/export_failure_prefix.json').write_text(json.dumps(cases, indent=2) + '\n')
    print('wrote export_failure_prefix.json')


if __name__ == '__main__':
    main()

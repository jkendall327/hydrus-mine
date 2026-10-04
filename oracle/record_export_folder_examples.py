#!/usr/bin/env python3
"""Record export-folder example refresh against real local file queries and media.

Only the AsyncQtJob scheduler is deferred: its original work and publish callbacks
run unchanged, using the reference database and media metadata. No network or
export writes occur. Capture button state, query limits, owner factory data, and
URL-source preview strings before/after a query change.
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
        from hydrus.core import HydrusConstants as HC
        from hydrus.client import ClientConstants as CC, ClientLocation
        from hydrus.client.exporting import ClientExportingFiles
        from hydrus.client.gui import ClientGUIAsync
        from hydrus.client.gui.exporting import ClientGUIExport
        from hydrus.client.metadata import ClientMetadataMigrationImporters as I
        from hydrus.client.networking.api import ClientLocalServerCore as API
        from hydrus.client.search import ClientSearchFileSearchContext as S
        jobs = []
        class DeferredJob:
            def __init__(self, owner, callback, publish):
                self.callback, self.publish = callback, publish
            def start(self):
                jobs.append(self)
        old_job = ClientGUIAsync.AsyncQtJob
        ClientGUIAsync.AsyncQtJob = DeferredJob
        location = ClientLocation.LocationContext.STATICCreateSimple(CC.COMBINED_LOCAL_FILE_DOMAINS_SERVICE_KEY)
        def context(tags):
            return S.FileSearchContext(location_context=location, predicates=API.ConvertTagListToPredicates(None, tags, do_permission_check=False))
        panel = ClientGUIExport.EditExportFolderPanel(c.gui, ClientExportingFiles.ExportFolder('synthetic examples', file_search_context=context(['system:everything'])))
        def state():
            factory = panel._test_context_factory
            media = factory.GetTestObjects()
            return {'text': panel._update_test_context_factory_button.text(),
                    'enabled': panel._update_test_context_factory_button.isEnabled(),
                    'media': [{'id': m.GetHashId(), 'hash': m.GetHash().hex(), 'size': m.GetSize()} for m in media],
                    'source_strings': factory.GetExampleTestStrings(I.SingleFileMetadataImporterMediaURLs())}
        result = {'initial': state(), 'cases': []}
        try:
            for tags in [['system:everything'], ['system:limit=2'], ['synthetic:export-example-no-match'], ['system:limit=100']]:
                panel._tag_autocomplete.SetFileSearchContext(context(tags))
                panel._SearchUpdated()
                row = {'tags': tags, 'before': state()}
                panel._UpdateTestExampleFiles()
                row['loading'] = state()
                job = jobs.pop(0)
                media = job.callback()
                job.publish(media)
                row['finished'] = state()
                result['cases'].append(row)
            # Reference query changes reset the button, retaining previous examples.
            panel._tag_autocomplete.SetFileSearchContext(context(['synthetic:export-example-no-match']))
            panel._SearchUpdated()
            result['query_changed'] = state()
            panel.resize(760, 820); panel.show(); QW.QApplication.processEvents()
            panel.grab().save(os.path.join(HERE, 'fixtures/export_folder_examples_editor.png'))
        finally:
            ClientGUIAsync.AsyncQtJob = old_job
            panel.deleteLater()
        return result
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
    with open(os.path.join(HERE, 'fixtures/export_folder_examples.json'), 'w') as f:
        json.dump(result, f, indent=1, ensure_ascii=False); f.write('\n')
    print('wrote export_folder_examples.json')

if __name__ == '__main__': main()

#!/usr/bin/env python3
"""Record "Open files in a new duplicates filter page" (command 154,
`SIMPLE_OPEN_SELECTION_IN_NEW_DUPLICATES_FILTER_PAGE`) on the real thumbnail media panel and
the real media viewer canvas, with "Set to combined local file domains when hitting ..."
(`open_files_to_duplicate_filter_uses_all_my_files`) on and off: the page the client is asked
to open -- its location context, predicates, name and whether the main window is brought
forward -- and what the page's own location was.
"""
import json, os, sys, tempfile
HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)

def record(session):
    controller = session.controller
    gui = controller.gui
    def work():
        from qtpy import QtWidgets as QW, QtTest as QT
        from hydrus.client import ClientApplicationCommand as CAC, ClientConstants as CC, ClientLocation
        from hydrus.client.gui.canvas import ClientGUICanvas, ClientGUICanvasFrame
        from hydrus.client.media import ClientMediaFileFilter
        options = controller.new_options
        location = ClientLocation.LocationContext.STATICCreateSimple(CC.LOCAL_FILE_SERVICE_KEY)
        manifest = json.loads(open(os.path.join(HERE, 'fixtures/legacy_db/basic.manifest.json')).read())
        hashes = [bytes.fromhex(f['hash']) for f in manifest['files'] if f['name'].startswith('jpeg_')][:3]
        results = controller.Read('media_results', hashes)
        def settle():
            QT.QTest.qWait(50)
            QW.QApplication.processEvents()
        calls = []
        old_pub = controller.pub
        def pub(topic, *args, **kwargs):
            if topic == 'new_page_duplicates':
                context = args[0] if args else kwargs['location_context']
                calls.append({'current': sorted(k.hex() for k in context.current_service_keys), 'deleted': sorted(k.hex() for k in context.deleted_service_keys), 'name': kwargs.get('page_name'), 'predicates': [p.ToString() for p in kwargs.get('initial_predicates', [])], 'activate_window': kwargs.get('activate_window')})
            else:
                old_pub(topic, *args, **kwargs)
        controller.pub = pub
        out = {'page_location': [CC.LOCAL_FILE_SERVICE_KEY.hex()], 'combined': [CC.COMBINED_LOCAL_FILE_DOMAINS_SERVICE_KEY.hex()], 'hashes': [h.hex() for h in hashes], 'cases': []}
        command = CAC.ApplicationCommand.STATICCreateSimpleCommand(CAC.SIMPLE_OPEN_SELECTION_IN_NEW_DUPLICATES_FILTER_PAGE)
        try:
            page = gui.NewPageQuery(location, initial_hashes=hashes, page_name='dup source', select_page=True)
            for _ in range(40):
                settle()
                if len(page.GetMediaResultsPanel().GetFlatMedia()) == len(hashes): break
            panel = page.GetMediaResultsPanel()
            panel._Select(ClientMediaFileFilter.FileFilter(ClientMediaFileFilter.FILE_FILTER_ALL))
            settle()
            frame = ClientGUICanvasFrame.CanvasFrame(gui)
            canvas = ClientGUICanvas.CanvasMediaListBrowser(frame, os.urandom(32), location, results, hashes[0])
            frame.SetCanvas(canvas)
            frame.showNormal(); frame.resize(1000, 700)
            settle()
            for activate in (False, True):
                options.SetBoolean('activate_window_on_tag_search_page_activation', activate)
                for combined in (True, False):
                    options.SetBoolean('open_files_to_duplicate_filter_uses_all_my_files', combined)
                    for where, target in (('thumbnails', panel), ('viewer', canvas)):
                        del calls[:]
                        target.ProcessApplicationCommand(command)
                        settle()
                        out['cases'].append({'where': where, 'combined_option': combined, 'activate_option': activate, 'calls': list(calls)})
            frame.close()
        finally:
            controller.pub = old_pub
        return out
    return controller.CallBlockingToQt(gui, work)

def main():
    import hydrus_driver
    if len(sys.argv) > 1 and sys.argv[1] == '--child':
        import record_api
        out = sys.argv[2]
        result = hydrus_driver.run_client(record_api.unpack_fixture('basic'), record)
        with open(out, 'w') as f: json.dump(result, f)
        return
    with tempfile.TemporaryDirectory() as work:
        out = os.path.join(work, 'result.json')
        hydrus_driver.run_in_subprocess(os.path.abspath(__file__), '--child', out)
        with open(out) as f: result = json.load(f)
    with open(os.path.join(HERE, 'fixtures/open_in_duplicates.json'), 'w') as f:
        json.dump(result, f, indent=1, ensure_ascii=False)
        f.write('\n')
    print('wrote open_in_duplicates.json')
if __name__ == '__main__': main()

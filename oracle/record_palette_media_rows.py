#!/usr/bin/env python3
"""Record the command palette's media menu provider (`MediaMenuSearchProvider`, provider 2) on a
real search page with files selected: its rows (text and menu path) for a set of queries, with one
file selected and with three, and with the setting off.
"""
import json, os, sys, tempfile
HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)

QUERIES = ['select', 'archive', 'copy', 'open', 'manage', 'share', 'tags', 'delete', 'inbox']

def record(session):
    controller = session.controller
    gui = controller.gui
    def work():
        from qtpy import QtWidgets as QW, QtTest as QT
        from hydrus.client import ClientConstants as CC, ClientLocation
        from hydrus.client.gui import ClientGUILocatorSearchProviders as providers
        from hydrus.client.media import ClientMediaFileFilter
        options = controller.new_options
        location = ClientLocation.LocationContext.STATICCreateSimple(CC.LOCAL_FILE_SERVICE_KEY)
        manifest = json.loads(open(os.path.join(HERE, 'fixtures/legacy_db/basic.manifest.json')).read())
        hashes = [bytes.fromhex(f['hash']) for f in manifest['files'] if f['name'].startswith('jpeg_')][:3]
        controller.Read('media_results', hashes)
        def settle():
            QT.QTest.qWait(50)
            QW.QApplication.processEvents()
        options.SetBoolean('command_palette_show_media_menu', True)
        options.SetInteger('command_palette_num_chars_for_results_threshold', 1)
        page = gui.NewPageQuery(location, initial_hashes=hashes, page_name='palette media', select_page=True)
        for _ in range(100):
            settle()
            if len(page.GetMediaResultsPanel().GetFlatMedia()) == len(hashes): break
        gui.ShowPage(page.GetPageKey()); settle()
        panel = page.GetMediaResultsPanel()
        def rows(query):
            provider = providers.GetSearchProvider(2) if hasattr(providers, 'GetSearchProvider') else None
            found = []
            provider.resultsAvailable.connect(lambda job, data: found.extend(data))
            provider.processQuery(query, None, 99)
            return [list(r.text) for r in found]
        out = {'queries': QUERIES, 'hashes': [h.hex() for h in hashes], 'selections': []}
        panel = page.GetMediaResultsPanel()
        flat = list(panel.GetFlatMedia())
        assert len(flat) == len(hashes), (len(flat), len(hashes))
        for label, count in (('none', 0), ('one', 1), ('three', 3)):
            panel._Select(ClientMediaFileFilter.FileFilter(ClientMediaFileFilter.FILE_FILTER_NONE))
            if count == 1:
                panel._HitMedia(flat[0], False, False)
            elif count == 3:
                panel._Select(ClientMediaFileFilter.FileFilter(ClientMediaFileFilter.FILE_FILTER_ALL))
            settle()
            out['selections'].append({'label': label, 'count': count, 'rows': {q: rows(q) for q in QUERIES}})
        # activating an entry: the effect on the selection (it runs the menu action)
        activations = []
        for query in ('archive', 'inbox', 'not selected'):
            panel._Select(ClientMediaFileFilter.FileFilter(ClientMediaFileFilter.FILE_FILTER_NONE))
            settle()
            provider = providers.GetSearchProvider(2)
            found = []
            provider.resultsAvailable.connect(lambda job, data: found.extend(data))
            provider.processQuery(query, None, 99)
            if not found:
                continue
            row = found[0]
            provider.resultSelected(row.id)
            settle()
            selected = [m.GetHash().hex() for m in panel._GetSelectedFlatMedia()]
            activations.append({'query': query, 'row': list(row.text), 'selected': len(selected)})
        out['activations'] = activations
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
    with open(os.path.join(HERE, 'fixtures/palette_media_rows.json'), 'w') as f:
        json.dump(result, f, indent=1, ensure_ascii=False)
        f.write('\n')
    print('wrote palette_media_rows.json')
if __name__ == '__main__': main()

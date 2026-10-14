#!/usr/bin/env python3
"""Record "refresh this page" on a URL importer page: three files in the
page sorted by file size, one selected and moved to the end by hand, then
the tab menu's "refresh this page" (captured menu, its action triggered):
the sort is applied again (the files are back in sorted order), the
selection stays, the importer's queue stays paused, and the page shown
stays the one that was shown.

Usage: QT_QPA_PLATFORM=offscreen ~/pyenv/bin/python oracle/record_tab_refresh_importer.py
       (writes fixtures/tab_refresh_importer.json)
"""
import json
import sys
import tempfile
import time
from pathlib import Path
HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))


def record(session):
    from hydrus.client import ClientConstants as CC, ClientLocation
    from hydrus.client.gui import ClientGUICore
    from hydrus.client.gui.pages import ClientGUIPages
    from hydrus.client.media import ClientMediaSort
    from record_main_menu import tree
    controller, gui = session.controller, session.controller.gui
    qt = lambda f: controller.CallBlockingToQt(gui, f)
    context = ClientLocation.LocationContext.STATICCreateSimple(CC.LOCAL_FILE_SERVICE_KEY)
    manifest = json.loads((HERE / 'fixtures/legacy_db/basic.manifest.json').read_text())
    hashes = [bytes.fromhex(f['hash']) for f in manifest['files'][:3]]
    core = ClientGUICore.core()
    old_popup = core.PopupMenu
    menus, popup_menus = [], []

    def popup(win, menu):
        menus.append(tree(menu))
        popup_menus.append(menu)
    core.PopupMenu = popup
    try:
        def build():
            top = ClientGUIPages.PagesNotebook(gui, 'refresh importer')
            importer = top.NewPageImportURLs(page_name='importer')
            urls = importer.GetPageManager().GetVariable('urls_import')
            if not urls.IsPaused():
                urls.PausePlay()
            urls.PendURLs(['https://files.example/%d.jpg' % i for i in range(3)])
            other = top.NewPageQuery(context, page_name='other', initial_predicates=[])
            locked = top.NewPageQuery(context, page_name='locked', initial_hashes=hashes[:2])
            top.setCurrentIndex(1)
            return top, importer, other, locked
        top, importer, other, locked = qt(build)
        for _ in range(400):
            if qt(lambda: importer._initialised and other._initialised and locked._initialised):
                break
            time.sleep(0.025)
        results = qt(lambda: controller.Read('media_results', hashes))
        qt(lambda: importer.AddMediaResults(results))
        time.sleep(1)
        panel = lambda: importer._media_panel
        order = lambda: [m.GetHash().hex() for m in panel().GetSortedMedia()]
        selected = lambda: sorted(m.GetHash().hex() for m in panel()._selected_media)
        sort = ClientMediaSort.MediaSort(('system', CC.SORT_FILES_BY_FILESIZE), CC.SORT_ASC)

        def set_sort():
            importer._sidebar_management_panel._media_sort_widget.SetSort(sort, do_sort=True)
        qt(set_sort)
        sorted_order = qt(order)

        def select_and_move():
            media = panel().GetSortedMedia()
            panel()._HitMedia(media[0], False, False)
            panel().MoveMedia([media[0]], len(media))
        qt(select_and_move)
        moved = {'order': qt(order), 'selected': qt(selected)}
        shown_before = qt(lambda: top.GetCurrentMediaPage().GetName())
        at = qt(lambda: [top.widget(i).GetName() for i in range(top.count())]).index('importer')

        def refresh():
            menus.clear()
            popup_menus.clear()
            top._ShowMenuForTabIndex(at)
            action = [a for a in popup_menus[0].actions() if a.text() == 'refresh this page'][0]
            action.trigger()
        qt(refresh)
        time.sleep(1)
        urls = lambda: importer.GetPageManager().GetVariable('urls_import')
        # a page locked to its hashes, with its sync paused by hand, keeps its files
        # and stays paused when refreshed
        panel_of = lambda page: page._media_panel
        locked_hashes = lambda: [m.GetHash().hex() for m in panel_of(locked).GetSortedMedia()]
        sync_of = lambda: bool(locked._sidebar_management_panel._tag_autocomplete.IsSynchronised())
        qt(lambda: locked._sidebar_management_panel._tag_autocomplete.SetSynchronised(False))
        time.sleep(1)
        locked_before = {'files': qt(locked_hashes), 'synchronised': qt(sync_of),
            'hash_locked': bool(qt(lambda: locked.GetPageManager().GetVariable('system_hash_locked')))}
        at_locked = qt(lambda: [top.widget(i).GetName() for i in range(top.count())]).index('locked')

        def refresh_locked():
            menus.clear()
            popup_menus.clear()
            top._ShowMenuForTabIndex(at_locked)
            [a for a in popup_menus[0].actions() if a.text() == 'refresh this page'][0].trigger()
        qt(refresh_locked)
        time.sleep(1)
        locked_after = {'files': qt(locked_hashes), 'synchronised': qt(sync_of)}
        return {'locked': {'before': locked_before, 'after': locked_after}, 'hashes': [h.hex() for h in hashes], 'sorted': sorted_order, 'moved': moved,
            'refreshed': {'order': qt(order), 'selected': qt(selected),
                'shown': qt(lambda: top.GetCurrentMediaPage().GetName()), 'paused': qt(lambda: bool(urls().IsPaused()))},
            'shown_before': shown_before, 'menu_has_refresh': True}
    finally:
        core.PopupMenu = old_popup


def main():
    import hydrus_driver, record_api
    if len(sys.argv) > 1 and sys.argv[1] == '--child':
        Path(sys.argv[2]).write_text(json.dumps(hydrus_driver.run_client(record_api.unpack_fixture('basic'), record)))
        return
    with tempfile.TemporaryDirectory() as work:
        out = Path(work) / 'refresh.json'
        hydrus_driver.run_in_subprocess(str(Path(__file__).resolve()), '--child', str(out))
        result = json.loads(out.read_text())
    (HERE / 'fixtures' / 'tab_refresh_importer.json').write_text(json.dumps(result, indent=2) + '\n')


if __name__ == '__main__':
    main()

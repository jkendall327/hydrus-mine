#!/usr/bin/env python3
"""Record what the thumbnail menu's select and remove rows do.

On pages of the `basic` fixture's files (all local files, with the trash and
the files it does not have, and the inbox files), with several selections,
each row of the grid's own `GetMenu` "select" and "remove" submenus is
triggered (as the menu's click does): after a select row, the files selected;
after a remove row, the files left on the page (a fresh page for each). Files
are named by their hash.

Usage: QT_QPA_PLATFORM=offscreen python oracle/record_thumbnail_select_remove.py
       (writes fixtures/thumbnail_select_remove.json)
"""
import json
import os
import sys
import time

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
OUT = os.path.join(HERE, 'fixtures', 'thumbnail_select_remove.json')
MANIFEST = json.load(open(os.path.join(HERE, 'fixtures', 'legacy_db', 'basic.manifest.json')))


def record(session):
    from hydrus.core import HydrusConstants as HC
    from hydrus.client import ClientConstants as CC, ClientLocation
    from hydrus.client.media import ClientMediaFileFilter
    controller = session.controller
    gui = controller.gui
    hashes = [bytes.fromhex(f['hash']) for f in MANIFEST['files']]

    def qt(f):
        return controller.CallBlockingToQt(gui, f)

    def open_page(service_key, page_hashes):
        location = ClientLocation.LocationContext.STATICCreateSimple(service_key)
        page = qt(lambda: gui._notebook.NewPageQuery(location, initial_hashes=page_hashes))
        for _ in range(600):
            panel = qt(lambda: page.GetMediaResultsPanel())
            if hasattr(panel, 'GetMenu') and len(panel._sorted_media) > 0:
                return page, panel
            time.sleep(0.05)
        raise Exception('the page never loaded')

    def close(page):
        def do():
            notebook = gui._notebook
            for i in range(notebook.count()):
                if notebook.widget(i) is page:
                    notebook._ClosePage(i, polite=False)
                    return
        qt(do)

    media_results = controller.Read('media_results', hashes)
    inbox_hashes = [m.GetHash() for m in media_results if m.GetInbox()]
    pages = []
    for name, service_key, page_hashes in (
        ('all local files', CC.COMBINED_LOCAL_FILE_DOMAINS_SERVICE_KEY, hashes),
        ('inbox files', CC.LOCAL_FILE_SERVICE_KEY, inbox_hashes),
    ):
        page, panel = open_page(service_key, page_hashes)
        media = qt(lambda: list(panel._sorted_media))
        inbox = [m for m in media if m.HasInbox()]
        archived = [m for m in media if not m.HasInbox()]
        selections = [('none', []), ('one in the inbox', inbox[:1]), ('several', inbox[:2] + archived[:2])]
        order = [m.GetHash().hex() for m in media]
        cases = []
        for label, selected in selections:
            picked = [m.GetHash() for m in selected]

            def select_these(p):
                p._Select(ClientMediaFileFilter.FileFilter(ClientMediaFileFilter.FILE_FILTER_NONE))
                by_hash = {m.GetHash(): m for m in p._sorted_media}
                for i, h in enumerate(picked):
                    p._HitMedia(by_hash[h], i > 0, False)

            def rows(p, which):
                def build():
                    select_these(p)
                    menu = p.GetMenu()
                    for action in menu.actions():
                        if action.menu() is not None and action.text() == which:
                            return menu, action.menu()
                    return menu, None
                menu, sub = qt(build)
                out = [a.text() for a in sub.actions() if not a.isSeparator()] if sub is not None else []
                return out, menu, sub

            def trigger(p, which, text):
                def do():
                    select_these(p)
                    menu = p.GetMenu()
                    for action in menu.actions():
                        if action.menu() is not None and action.text() == which:
                            for a in action.menu().actions():
                                if a.text() == text:
                                    a.trigger()
                                    return
                    raise Exception('no such row')
                qt(do)
                time.sleep(0.15)

            select_rows, _, _ = rows(panel, 'select')
            remove_rows, _, _ = rows(panel, 'remove')
            case = {'selection': label, 'selected': [h.hex() for h in picked], 'select': {}, 'remove': {}, 'select_rows': select_rows, 'remove_rows': remove_rows}
            for text in select_rows:
                trigger(panel, 'select', text)
                case['select'][text] = sorted(m.GetHash().hex() for m in qt(lambda: list(panel._selected_media)))
            for text in remove_rows:
                fresh, fresh_panel = open_page(service_key, page_hashes)
                trigger(fresh_panel, 'remove', text)
                case['remove'][text] = [m.GetHash().hex() for m in qt(lambda: list(fresh_panel._sorted_media))]
                close(fresh)
            cases.append(case)
        pages.append({'page': name, 'files': order, 'cases': cases})
        close(page)
    return {'pages': pages}


def main():
    import hydrus_driver
    import record_api
    result = hydrus_driver.run_client(record_api.unpack_fixture('basic'), record)
    with open(OUT, 'w') as f:
        json.dump(result, f, indent=1, sort_keys=True, ensure_ascii=False)
        f.write('\n')
    print(f'wrote {OUT}')


if __name__ == '__main__':
    main()

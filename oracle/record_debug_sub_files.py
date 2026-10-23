#!/usr/bin/env python3
"""Help > debug > gui actions > "publish some sub files in five seconds".

The real QAction of the reference's help menu is triggered on the `basic`
fixture, with a nested page of pages current (so where the new page goes, the
deepest notebook, can be seen). The real CallLater and pub are observed and
forwarded unchanged: the action schedules a publication of five new random
hashes to the page named "example sub files" five seconds on, as a
subscription would publish its files. After the delay, the pages are
recorded: each page's name, its notebook and whether it is current, and the new
page's search (location, predicates) and number of files. The action is then
triggered again (the page exists: no new page), and last the fixture's real
files are published to the same name (`imported_files_to_page`, as a
subscription does), which adds them to that page.

Writes fixtures/debug_sub_files.json.
"""
import json
import sqlite3
import sys
import tempfile
import time
from pathlib import Path

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
NAME = 'example sub files'


def record(session):
    def drive():
        from qtpy import QtTest as T, QtWidgets as W
        from hydrus.client import ClientConstants as CC, ClientLocation
        from hydrus.client.gui.pages import ClientGUIPages
        controller = session.controller
        gui = controller.gui
        root = gui._notebook
        original_later, original_pub = controller.CallLater, controller.pub
        scheduled, published, menus = [], [], []
        started = time.monotonic()

        def elapsed():
            return round((time.monotonic() - started) * 1000)

        def later(delay, callback, *args, **kwargs):
            job = original_later(delay, callback, *args, **kwargs)
            scheduled.append(dict(at_ms=elapsed(), delay_seconds=delay))
            return job

        def pub(topic, *args, **kwargs):
            if topic == 'imported_files_to_page':
                published.append(dict(at_ms=elapsed(), hashes=len(args[0]), page_name=args[1]))
            return original_pub(topic, *args, **kwargs)

        def find(menu, labels):
            for action in menu.actions():
                if action.text() == labels[0]:
                    return action if len(labels) == 1 else find(action.menu(), labels[1:])
            raise ValueError(labels)

        def wait(ms):
            end = time.monotonic() + ms / 1000
            while time.monotonic() < end:
                T.QTest.qWait(50)

        def location(value):
            return dict(current=sorted(k.hex() for k in value.current_service_keys),
                        deleted=sorted(k.hex() for k in value.deleted_service_keys))

        def pages(notebook, parent):
            out = []
            for index, page in enumerate(notebook.GetPages()):
                entry = dict(name=page.GetName(), notebook=parent, index=index,
                             is_notebook=isinstance(page, ClientGUIPages.PagesNotebook))
                out.append(entry)
                if entry['is_notebook']:
                    out.extend(pages(page, page.GetName()))
            return out

        def named():
            page = root._GetPageFromName(NAME, only_media_pages=True)
            if page is None:
                return None
            context = page.GetPageManager().GetVariable('file_search_context')
            return dict(location=location(context.GetLocationContext()),
                        predicates=[p.ToString() for p in context.GetPredicates()],
                        files=sorted(m.GetHash().hex() for m in page.GetMediaResultsPanel().GetSortedMedia()),
                        current=gui.GetCurrentPage() is page)

        def state():
            return dict(pages=pages(root, None), current=gui.GetCurrentPage().GetName(), named=named())

        try:
            controller.CallLater, controller.pub = later, pub
            nested = root.NewPagesNotebook(name='nested', give_it_a_blank_page=False)
            nested.NewPageQuery(ClientLocation.LocationContext.STATICCreateSimple(CC.LOCAL_FILE_SERVICE_KEY), page_name='inner')
            W.QApplication.processEvents()
            before = state()
            menu, label = gui._InitialiseMenuInfoHelp()
            menus.append(menu)
            action = find(menu, ['debug', 'gui actions', 'publish some sub files in five seconds'])
            started = time.monotonic()
            action.trigger()
            wait(4500)
            at_four_and_a_half = dict(published=len(published), named=named() is not None)
            wait(1500)
            wait(1000)
            first = state()
            started = time.monotonic()
            action.trigger()
            wait(6500)
            second = state()
            # three of the fixture's own files, as a subscription would publish them
            from hydrus.client.search import ClientSearchFileSearchContext as FSC, ClientSearchPredicate as P
            everything = FSC.FileSearchContext(
                location_context=ClientLocation.LocationContext.STATICCreateSimple(CC.COMBINED_LOCAL_FILE_DOMAINS_SERVICE_KEY),
                predicates=[P.Predicate(P.PREDICATE_TYPE_SYSTEM_EVERYTHING)])
            ids = sorted(controller.Read('file_query_ids', everything))[:3]
            real = list(controller.Read('hash_ids_to_hashes', hash_ids=ids).values())
            controller.pub('imported_files_to_page', real, NAME)
            wait(1500)
            third = state()
            first['named_files'] = len(first['named']['files']) if first['named'] else None
            return dict(menu_path=['help', 'debug', 'gui actions', action.text()],
                        schedule=scheduled, published=published, before=before,
                        at_four_and_a_half_seconds=at_four_and_a_half,
                        after_first=first, after_second=second,
                        real_files_published=sorted(h.hex() for h in real), after_real_files=third)
        finally:
            controller.CallLater, controller.pub = original_later, original_pub
            for menu in menus:
                menu.deleteLater()
            W.QApplication.processEvents()

    return session.controller.CallBlockingToQt(session.controller.gui, drive)


def main():
    import hydrus_driver
    import record_api
    if len(sys.argv) > 1 and sys.argv[1] == '--child':
        database = record_api.unpack_fixture('basic')
        conn = sqlite3.connect(str(Path(database) / 'client.db'))
        raw = json.loads(conn.execute('SELECT dictionary_string FROM services WHERE service_type=18').fetchone()[0])
        for key, value in raw[2]:
            if key == [0, 'port']:
                value[1] = None
        conn.execute('UPDATE services SET dictionary_string=? WHERE service_type=18', (json.dumps(raw),))
        conn.commit()
        conn.close()
        Path(sys.argv[2]).write_text(json.dumps(hydrus_driver.run_client(database, record)))
        return
    with tempfile.TemporaryDirectory() as directory:
        output = Path(directory) / 'result.json'
        hydrus_driver.run_in_subprocess(str(Path(__file__).resolve()), '--child', str(output))
        result = json.loads(output.read_text())
    (HERE / 'fixtures/debug_sub_files.json').write_text(json.dumps(result, indent=1) + '\n')
    print('wrote debug_sub_files.json')


if __name__ == '__main__':
    main()

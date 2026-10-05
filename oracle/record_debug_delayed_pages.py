#!/usr/bin/env python3
"""Genuine delayed-page QAction, immutable menu location, current notebook delivery.

Observe and forward actual CallLater/pub with unmodified scheduler/clock. Two
menus capture different locations before preference changes; hidden/minimized
Main receives real selected query pages without explicit activation.
"""
import json
import sqlite3
import sys
import tempfile
import time
from pathlib import Path
HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))


def record(session):
    def drive():
        from qtpy import QtTest as T, QtWidgets as W
        from hydrus.client import ClientConstants as CC, ClientLocation
        from hydrus.core import HydrusConstants as HC
        controller = session.controller
        gui, options = controller.gui, controller.new_options
        root = gui._notebook
        local_tags = controller.services_manager.GetServices((HC.LOCAL_TAG,))[0].GetServiceKey()
        before_location = options.GetDefaultLocalLocationContext().Duplicate()
        before_tags = options.GetKey('default_tag_service_search_page')
        original_later, original_pub = controller.CallLater, controller.pub
        scheduled, published, menus, trace = [], [], [], []
        started = time.monotonic()
        def elapsed(): return round((time.monotonic() - started) * 1000, 3)
        def location(value):
            return dict(current=sorted(k.hex() for k in value.current_service_keys),
                        deleted=sorted(k.hex() for k in value.deleted_service_keys))
        def later(delay, callback, *args, **kwargs):
            job = original_later(delay, callback, *args, **kwargs)
            if args and args[0] == 'new_page_query':
                scheduled.append(dict(at_ms=elapsed(), delay_seconds=delay,
                                      callback=callback.__name__, topic=args[0],
                                      location=location(args[1]), job=job))
            return job
        def pub(topic, *args, **kwargs):
            if topic == 'new_page_query':
                published.append(dict(at_ms=elapsed(), location=location(args[0])))
            return original_pub(topic, *args, **kwargs)
        def find(menu, labels):
            for action in menu.actions():
                if action.text() == labels[0]:
                    return action if len(labels) == 1 else find(action.menu(), labels[1:])
            raise ValueError(labels)
        def make_menu(key):
            options.SetDefaultLocalLocationContext(ClientLocation.LocationContext.STATICCreateSimple(key))
            menu, label = gui._InitialiseMenuInfoHelp(); menus.append(menu)
            return find(menu, ['debug', 'gui actions', 'make a new page in five seconds'])
        def wait_to(target):
            while elapsed() < target:
                T.QTest.qWait(max(1, min(50, round(target - elapsed()))))
            trace.append(dict(at_ms=elapsed(), requested_ms=target, published=len(published),
                              visible=gui.isVisible(), minimized=gui.isMinimized()))
        def page_snapshot(page):
            context = page.GetPageManager().GetVariable('file_search_context')
            return dict(name=page.GetName(), location=location(context.GetLocationContext()),
                        tag_service=context.GetTagContext().service_key.hex(),
                        predicates=[p.ToString() for p in context.GetPredicates()],
                        selected=gui.GetCurrentPage() is page)
        try:
            controller.CallLater, controller.pub = later, pub
            source = root.NewPageQuery(ClientLocation.LocationContext.STATICCreateSimple(CC.LOCAL_FILE_SERVICE_KEY), page_name='delayed source')
            action_a = make_menu(CC.LOCAL_FILE_SERVICE_KEY)
            options.SetDefaultLocalLocationContext(ClientLocation.LocationContext.STATICCreateSimple(CC.TRASH_SERVICE_KEY))
            started = time.monotonic(); action_a.trigger(); wait_to(1000)
            action_b = make_menu(CC.TRASH_SERVICE_KEY)
            options.SetDefaultLocalLocationContext(ClientLocation.LocationContext.STATICCreateSimple(CC.LOCAL_FILE_SERVICE_KEY))
            action_b.trigger()
            nested = root.NewPagesNotebook(name='delivery notebook', give_it_a_blank_page=False)
            options.SetKey('default_tag_service_search_page', local_tags)
            gui.hide(); wait_to(4900); wait_to(5400)
            first = gui.GetCurrentPage()
            first_result = page_snapshot(first)
            first_result['in_delivery_notebook'] = first in nested.GetPages()
            assert first_result['in_delivery_notebook'] and first_result['selected']
            assert first_result['location'] == location(ClientLocation.LocationContext.STATICCreateSimple(CC.LOCAL_FILE_SERVICE_KEY))
            root.setCurrentWidget(source)
            gui.showMinimized(); wait_to(5900); wait_to(6400)
            second = gui.GetCurrentPage()
            second_result = page_snapshot(second)
            second_result['in_root_notebook'] = second in root.GetPages()
            assert second_result['in_root_notebook'] and second_result['selected']
            assert second_result['location'] == location(ClientLocation.LocationContext.STATICCreateSimple(CC.TRASH_SERVICE_KEY))
            assert len(published) == len(scheduled) == 2
            assert all(item['job'].IsWorkComplete() for item in scheduled)
            assert first_result['tag_service'] == second_result['tag_service'] == local_tags.hex()
            assert first_result['predicates'] == second_result['predicates'] == []
            gui.showNormal(); T.QTest.qWait(100)
            gui.grab().save(str(HERE / 'fixtures/debug_delayed_pages.png'))
            return dict(menu_path=['help','debug','gui actions','make a new page in five seconds'],
                        schedule=[{k:v for k,v in item.items() if k!='job'} for item in scheduled],
                        published=published, trace=trace, pages=[first_result, second_result],
                        location_snapshot='Help menu construction, before trigger and deadline',
                        scheduler='actual controller CallLater/background SingleJob',
                        clock='unmodified real monotonic observer')
        finally:
            controller.CallLater, controller.pub = original_later, original_pub
            options.SetDefaultLocalLocationContext(before_location)
            options.SetKey('default_tag_service_search_page', before_tags)
            for item in scheduled:
                if not item['job'].IsWorkComplete(): item['job'].Cancel()
            gui.showNormal()
            for menu in menus: menu.deleteLater()
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
    (HERE / 'fixtures/debug_delayed_pages.json').write_text(json.dumps(result, indent=2) + '\n')
    print('wrote debug_delayed_pages.json')


if __name__ == '__main__':
    main()

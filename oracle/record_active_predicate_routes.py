#!/usr/bin/env python3
"""Trigger real active-list copy/open QActions and inspect a real new-page consumer.

Only popup transport and controller publications are captured. A recorded search
publication is then given to the actual GUI handler. The inherited active-list
file-selection handler is recorded as the reference base no-op, not media-list
selection. No reference source or persistent user's database is changed.
"""
import json
import os
import shutil
import sys
import time

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
from hydrus_driver import run_client
import record_api


def record(session):
    c = session.controller
    from hydrus.client import ClientConstants as CC, ClientLocation
    from hydrus.client.gui import ClientGUICore as Core
    from hydrus.client.gui.search import ClientGUIACDropdown as AC
    from hydrus.client.search import ClientSearchPredicate as P, ClientSearchFileSearchContext as F, ClientSearchTagContext as T
    from qtpy import QtCore as QC
    gui = c.gui
    qt = lambda fn: c.CallBlockingToQt(gui, fn)
    location = ClientLocation.LocationContext.STATICCreateSimple(CC.LOCAL_FILE_SERVICE_KEY)
    context = F.FileSearchContext(location_context=location, tag_context=T.TagContext())
    page = qt(lambda: gui._notebook.NewPageQueryFileSearchContext(context, page_name='Active routes'))
    def ready(page):
        for _ in range(500):
            sidebar = qt(page.GetSidebar)
            if sidebar is not None and hasattr(sidebar, '_tag_autocomplete'):
                return sidebar
            time.sleep(.01)
        raise AssertionError('query sidebar not ready')
    box = ready(page)._tag_autocomplete._predicates_listbox
    ac = box._my_ac_parent
    old_popup, old_pub, old_thread = Core.core().PopupMenu, c.pub, c.CallToThread
    c.CallToThread = lambda fn, *a, **kw: None if fn == AC.ReadFetch else old_thread(fn, *a, **kw)
    menus, publications = [], []
    Core.core().PopupMenu = lambda _widget, menu: menus.append(menu)
    def publish(topic, *args, **kwargs):
        if topic in ('clipboard', 'new_page_query', 'new_page_duplicates', 'select_files_with_tags'):
            publications.append((topic, args, kwargs))
        else:
            old_pub(topic, *args, **kwargs)
    c.pub = publish
    def payload(event):
        topic, args, kw = event
        if topic == 'clipboard':
            return {'topic': topic, 'type': args[0], 'text': args[1]}
        return {'topic': topic, 'location': args[0].GetSerialisableTuple(),
                'predicates': [p.GetSerialisableTuple() for p in kw['initial_predicates']],
                'name': kw['page_name'], 'activate': kw['activate_window']}
    tag = lambda name: P.Predicate(P.PREDICATE_TYPE_TAG, name)
    alpha, beta = tag('series:active alpha'), tag('series:active beta')
    inbox = P.Predicate(P.PREDICATE_TYPE_SYSTEM_INBOX)
    group = P.Predicate(P.PREDICATE_TYPE_OR_CONTAINER, [alpha, beta])
    hashes = P.Predicate(P.PREDICATE_TYPE_SYSTEM_HASH, ((bytes([1])*32, bytes([2])*32), 'sha256'))
    specs = [('tag', [alpha, beta, inbox], [alpha]),
             ('two_tags', [alpha, beta, inbox], [alpha, beta]),
             ('all_tags', [alpha, beta], [alpha, beta]),
             ('inbox', [alpha, inbox], [inbox]),
             ('or', [group, inbox], [group]),
             ('negative_namespace', [P.Predicate(P.PREDICATE_TYPE_NAMESPACE, 'series', inclusive=False), inbox], None),
             ('negative_wildcard', [P.Predicate(P.PREDICATE_TYPE_WILDCARD, 'series:active*', inclusive=False), inbox], None),
             ('deduplicated_subtags', [tag('series:same tag'), tag('creator:same tag'), inbox], None),
             ('hashes', [hashes, inbox], [hashes]),
             ('similar_files', [P.Predicate(P.PREDICATE_TYPE_SYSTEM_SIMILAR_TO_FILES, ((bytes([3])*32,), 4)), inbox], None),
             ('similar_data', [P.Predicate(P.PREDICATE_TYPE_SYSTEM_SIMILAR_TO_DATA, ((bytes([4])*32,), (bytes([5])*8,), 3)), inbox], None),
             ('two_tags_raise', [alpha, beta, inbox], [alpha, beta])]
    cases, consumer_event = [], None
    def replay():
        nonlocal consumer_event
        for name, values, selected in specs:
            c.new_options.SetBoolean('activate_window_on_tag_search_page_activation', name == 'two_tags_raise')
            if selected is None:
                selected = values[:-1]
            ctx = context.Duplicate()
            ctx.SetPredicates(values)
            ac.SetFileSearchContext(ctx)
            ac.SetSynchronised(False)
            box._selected_terms = {box._GenerateTermFromPredicate(p) for p in selected}
            menus.clear()
            box.ShowMenuFromSignal(QC.QPoint(1, 1))
            actions = []
            for top in menus[-1].actions():
                if top.text() not in ('copy', 'open') or top.menu() is None:
                    continue
                for action in top.menu().actions():
                    if action.isSeparator():
                        continue
                    publications.clear()
                    action.trigger()
                    actions.append({'group': top.text(), 'label': action.text().replace('&&', '&'),
                                    'publications': [payload(e) for e in publications]})
                    if name == 'inbox' and top.text() == 'open' and consumer_event is None:
                        consumer_event = publications[0]
            publications.clear()
            box._SelectFilesWithTags('AND')
            assert not publications
            cases.append({'name': name, 'current': [p.GetSerialisableTuple() for p in box.GetPredicates()],
                          'selected': [p.GetSerialisableTuple() for p in selected], 'actions': actions,
                          'select_files_publications': []})
    try:
        qt(replay)
        topic, args, kwargs = consumer_event
        assert topic == 'new_page_query'
        opened = qt(lambda: gui.NewPageQuery(*args, **kwargs))
        sidebar = ready(opened)
        def consume():
            ctx = sidebar._tag_autocomplete.GetFileSearchContext()
            return {'published': payload(consumer_event), 'location': ctx.GetLocationContext().GetSerialisableTuple(),
                    'tag_context': ctx.GetTagContext().GetSerialisableTuple(),
                    'default_tag_key': c.new_options.GetKey('default_tag_service_search_page').hex(),
                    'predicates': [p.GetSerialisableTuple() for p in ctx.GetPredicates()],
                    'query_count': len(c.Read('file_query_ids', ctx))}
        return {'cases': cases, 'search_consumer': qt(consume)}
    finally:
        Core.core().PopupMenu, c.pub, c.CallToThread = old_popup, old_pub, old_thread


if __name__ == '__main__':
    db = record_api.unpack_fixture('basic')
    try:
        result = run_client(db, record)
        with open(os.path.join(HERE, 'fixtures', 'active_predicate_routes.json'), 'w') as stream:
            json.dump(result, stream, indent=2, ensure_ascii=False)
            stream.write('\n')
    finally:
        shutil.rmtree(db)

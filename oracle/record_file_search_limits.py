#!/usr/bin/env python3
"""Record implicit limits, database sort eligibility and real sidebar refreshes.

FileSearchPanel edits actual preferences; query_ids returns real hash subsets.
The real sidebar SortChanged path records its RefreshQuery dispatch, covering
paused, disabled, implicit-only and unsupported-sort boundaries.
"""
import json
import os
import sys
import tempfile

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)


def record(session):
    def qt():
        from hydrus.client import ClientConstants as CC, ClientLocation
        from hydrus.client.gui.panels.options import FileSearchPanel
        from hydrus.client.media import ClientMediaSort
        from hydrus.client.search import ClientSearchFileSearchContext, ClientSearchPredicate
        controller = session.controller
        options = controller.new_options
        before = options.GetNoneableInteger('forced_search_limit')
        before_refresh = options.GetBoolean('refresh_search_page_on_system_limited_sort_changed')
        panel = FileSearchPanel.FileSearchPanel(controller.gui, options)
        initial = {'limit': panel._forced_search_limit.GetValue(),
                   'refresh_sort': panel._refresh_search_page_on_system_limited_sort_changed.isChecked()}
        location = ClientLocation.LocationContext.STATICCreateSimple(CC.LOCAL_FILE_SERVICE_KEY)
        everything = ClientSearchPredicate.Predicate(ClientSearchPredicate.PREDICATE_TYPE_SYSTEM_EVERYTHING)
        sort = ClientMediaSort.MediaSort(sort_type=('system', CC.SORT_FILES_BY_HASH), sort_order=CC.SORT_ASC)
        queries = []
        sorts = []
        try:
            for implicit in (None, 3):
                panel._forced_search_limit.SetValue(implicit)
                panel.UpdateOptions()
                for explicit in (None, 1, 8):
                    predicates = [everything]
                    if explicit is not None:
                        predicates.append(ClientSearchPredicate.Predicate(ClientSearchPredicate.PREDICATE_TYPE_SYSTEM_LIMIT, explicit))
                    context = ClientSearchFileSearchContext.FileSearchContext(location_context=location, predicates=predicates)
                    ids = controller.Read('file_query_ids', context, limit_sort_by=sort)
                    hashes = controller.Read('hash_ids_to_hashes', ids)
                    queries.append({'implicit': implicit, 'explicit': explicit,
                                    'effective': context.GetSystemPredicates().GetLimit(),
                                    'hashes': sorted(value.hex() for value in hashes.values())})
            for enabled in (False, True):
                panel._refresh_search_page_on_system_limited_sort_changed.setChecked(enabled)
                panel.UpdateOptions()
                for local in (True, False):
                    where = location if local else ClientLocation.LocationContext.STATICCreateSimple(CC.COMBINED_FILE_SERVICE_KEY)
                    for sort_code in range(28):
                        media_sort = ClientMediaSort.MediaSort(sort_type=('system', sort_code), sort_order=CC.SORT_ASC)
                        sorts.append({'enabled': enabled, 'local': local, 'code': sort_code,
                                      'can_sort_at_db': media_sort.CanSortAtDBLevel(where)})
            page = controller.gui._notebook.NewPageQuery(location, page_name='synthetic limited sort recording', select_page=False)
            sidebar = page.GetSidebar()
            ac = sidebar._tag_autocomplete
            original = sidebar._RefreshQuery
            calls = []
            sidebar._RefreshQuery = lambda: calls.append('query')
            refreshes = []
            try:
                for enabled, sync, explicit, code in ((True,True,True,CC.SORT_FILES_BY_HASH), (False,True,True,CC.SORT_FILES_BY_HASH), (True,False,True,CC.SORT_FILES_BY_HASH), (True,True,False,CC.SORT_FILES_BY_HASH), (True,True,True,CC.SORT_FILES_BY_MIME)):
                    options.SetBoolean('refresh_search_page_on_system_limited_sort_changed', enabled)
                    ac.SetSynchronised(sync)
                    predicates = [everything]
                    if explicit:
                        predicates.append(ClientSearchPredicate.Predicate(ClientSearchPredicate.PREDICATE_TYPE_SYSTEM_LIMIT, 3))
                    ac.SetFileSearchContext(ClientSearchFileSearchContext.FileSearchContext(location_context=location, predicates=predicates))
                    calls.clear()
                    sidebar._SortChanged(ClientMediaSort.MediaSort(sort_type=('system',code), sort_order=CC.SORT_DESC))
                    refreshes.append({'enabled':enabled,'sync':sync,'explicit':explicit,'code':code,'refresh_count':len(calls)})
            finally:
                sidebar._RefreshQuery = original
        finally:
            options.SetNoneableInteger('forced_search_limit', before)
            options.SetBoolean('refresh_search_page_on_system_limited_sort_changed', before_refresh)
            panel.deleteLater()
        return {'initial':initial,'queries':queries,'sort_eligibility':sorts,'refreshes':refreshes}
    return session.controller.CallBlockingToQt(session.controller.gui, qt)


def main():
    import hydrus_driver
    import record_api
    if len(sys.argv) > 1:
        output = sys.argv[2]
        result = hydrus_driver.run_client(record_api.unpack_fixture('basic'), record)
        with open(output, 'w') as stream:
            json.dump(result, stream)
        return
    with tempfile.TemporaryDirectory() as directory:
        path = os.path.join(directory, 'result.json')
        hydrus_driver.run_in_subprocess(os.path.abspath(__file__), '--child', path)
        with open(path) as stream:
            result = json.load(stream)
    output = os.path.join(HERE, 'fixtures', 'file_search_limits.json')
    with open(output, 'w') as stream:
        json.dump(result, stream, indent=2)
        stream.write('\n')
    print('wrote ' + output)


if __name__ == '__main__':
    main()

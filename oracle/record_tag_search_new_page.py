#!/usr/bin/env python3
"""Record what "search for this tag in a new page" (a tag list's middle click, or its menu)
asks the client for -- `ListBoxTags._NewSearchPages` -- with "Switch to main window when
creating new file search page from media viewer" off and on, and with tag presentation
options that change how the page is named: the page name, the location context, the
predicates, and the activate_window flag on the first page.
"""
import json, os, sys, tempfile
HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)

def record(session):
    controller = session.controller
    gui = controller.gui
    def work():
        from qtpy import QtWidgets as QW
        from hydrus.client import ClientConstants as CC, ClientLocation
        from hydrus.client.gui.lists import ClientGUIListBoxes as LB
        from hydrus.client.search import ClientSearchPredicate as P
        options = controller.new_options
        calls = []
        old_pub = controller.pub
        def pub(topic, *args, **kwargs):
            if topic in ('new_page_query', 'new_page_duplicates'):
                location = kwargs['location_context'] if 'location_context' in kwargs else args[0]
                calls.append({'topic': topic, 'location': sorted(k.hex() for k in location.current_service_keys) + ['deleted'] + sorted(k.hex() for k in location.deleted_service_keys), 'page_name': kwargs.get('page_name'), 'predicates': [p.ToString() for p in kwargs.get('initial_predicates', [])], 'activate_window': kwargs.get('activate_window')})
            else:
                old_pub(topic, *args, **kwargs)
        controller.pub = pub
        class Stub:
            def __init__(self, location): self._location = location
            def _GetCurrentLocationContext(self): return self._location
            def window(self): return gui
        location = ClientLocation.LocationContext.STATICCreateSimple(CC.LOCAL_FILE_SERVICE_KEY)
        result = {'cases': []}
        try:
            for connector, underscores in [(': ', False), (' ⇢ ', True)]:
                options.SetString('namespace_connector', connector)
                options.SetBoolean('replace_tag_underscores_with_spaces', underscores)
                for activate in (False, True):
                    options.SetBoolean('activate_window_on_tag_search_page_activation', activate)
                    del calls[:]
                    tags = ['series:canonical_under_score', 'plain_tag']
                    pages = [[P.Predicate(P.PREDICATE_TYPE_TAG, 'series:canonical_under_score')], [P.Predicate(P.PREDICATE_TYPE_TAG, 'plain_tag'), P.Predicate(P.PREDICATE_TYPE_TAG, 'series:other_tag')]]
                    LB.ListBoxTags._NewSearchPages(Stub(location), pages)
                    result['cases'].append({'connector': connector, 'underscores': underscores, 'activate': activate, 'calls': list(calls), 'default_location': [CC.LOCAL_FILE_SERVICE_KEY.hex()]})
        finally:
            controller.pub = old_pub
        return result
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
    with open(os.path.join(HERE, 'fixtures/tag_search_new_page.json'), 'w') as f:
        json.dump(result, f, indent=1, ensure_ascii=False)
        f.write('\n')
    print('wrote tag_search_new_page.json')
if __name__ == '__main__': main()

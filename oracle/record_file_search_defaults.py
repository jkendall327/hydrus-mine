#!/usr/bin/env python3
"""Record FileSearchPanel pause/everything controls and their live consumers.

The actual notebook creates each query page, and the database offers its system
predicates for both my files and all known files, including the forced override.
An already created page retains its pause state when the default changes.
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
        from hydrus.client.search import ClientSearchFileSearchContext, ClientSearchPredicate
        controller = session.controller
        options = controller.new_options
        keys = ('default_search_synchronised', 'show_system_everything')
        before = {key: options.GetBoolean(key) for key in keys}
        panel = FileSearchPanel.FileSearchPanel(controller.gui, options)
        initial = {'search_immediately': panel._default_search_synchronised.isChecked(),
                   'show_system_everything': panel._show_system_everything.isChecked()}
        pages = []
        events = []
        try:
            for enabled in (False, True):
                panel._default_search_synchronised.setChecked(enabled)
                panel._show_system_everything.setChecked(enabled)
                panel.UpdateOptions()
                page = controller.gui._notebook.NewPageQuery(
                    ClientLocation.LocationContext.STATICCreateSimple(CC.LOCAL_FILE_SERVICE_KEY),
                    page_name='synthetic search default recording', select_page=False)
                pages.append(page)
                offered = []
                for key in (CC.LOCAL_FILE_SERVICE_KEY, CC.COMBINED_FILE_SERVICE_KEY):
                    context = ClientSearchFileSearchContext.FileSearchContext(
                        location_context=ClientLocation.LocationContext.STATICCreateSimple(key))
                    predicates = controller.Read('file_system_predicates', context)
                    forced = controller.Read('file_system_predicates', context, force_system_everything=True)
                    offered.append({'location': key.hex(),
                                    'everything': any(p.GetType() == ClientSearchPredicate.PREDICATE_TYPE_SYSTEM_EVERYTHING for p in predicates),
                                    'forced_everything': any(p.GetType() == ClientSearchPredicate.PREDICATE_TYPE_SYSTEM_EVERYTHING for p in forced),
                                    'predicates': [p.ToString() for p in predicates]})
                events.append({'enabled': enabled,
                               'new_page_synchronised': page.GetPageManager().GetVariable('synchronised'),
                               'first_page_synchronised': pages[0].GetPageManager().GetVariable('synchronised'),
                               'offered': offered})
        finally:
            for key, value in before.items():
                options.SetBoolean(key, value)
            panel.deleteLater()
        return {'initial': initial, 'events': events}
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
    output = os.path.join(HERE, 'fixtures', 'file_search_defaults.json')
    with open(output, 'w') as stream:
        json.dump(result, stream, indent=2)
        stream.write('\n')
    print('wrote ' + output)


if __name__ == '__main__':
    main()

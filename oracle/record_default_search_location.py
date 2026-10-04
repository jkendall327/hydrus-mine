#!/usr/bin/env python3
"""Record the importable-domain selector, saved defaults and real search fallback.

FileSearchPanel edits the location with its real multiple-location panel. The
client resolves empty/missing locations, creates actual pages, and switches an
all-known-files read autocomplete to all known tags to exercise its fallback.
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
        from hydrus.client.gui.search import ClientGUILocation
        from hydrus.client.search import ClientSearchTagContext
        controller = session.controller
        options = controller.new_options
        before = options.GetDefaultLocalLocationContext().Duplicate()
        panel = FileSearchPanel.FileSearchPanel(controller.gui, options)
        button = panel._default_local_location_context
        def snapshot(location):
            return {'current': sorted(key.hex() for key in location.current_service_keys),
                    'deleted': sorted(key.hex() for key in location.deleted_service_keys),
                    'label': location.ToString(controller.services_manager.GetName)}
        selector = ClientGUILocation.EditMultipleLocationContextPanel(panel, button.GetValue(), False, True, False, False)
        choices = [{'label': selector._location_list.item(i).text(),
                    'data': selector._location_list.item(i).data(256)[1].hex()}
                   for i in range(selector._location_list.count())]
        initial = snapshot(button.GetValue())
        events = []
        try:
            for selected in ([], [CC.LOCAL_FILE_SERVICE_KEY], [bytes.fromhex(choices[0]["data"])], [bytes.fromhex(choice["data"]) for choice in choices]):
                selector.SetValue(ClientLocation.LocationContext(current_service_keys=selected))
                button.SetValue(selector.GetValue())
                panel.UpdateOptions()
                saved = snapshot(button.GetValue())
                resolved = options.GetDefaultLocalLocationContext()
                page = controller.gui._notebook.NewPageQuery(resolved, page_name='synthetic default location recording', select_page=False)
                ac = page.GetSidebar()._tag_autocomplete
                ac._SetLocationContext(ClientLocation.LocationContext.STATICCreateSimple(CC.COMBINED_FILE_SERVICE_KEY))
                ac._SetTagContext(ClientSearchTagContext.TagContext(service_key=CC.COMBINED_TAG_SERVICE_KEY))
                events.append({'selected': saved, 'resolved': snapshot(resolved),
                               'new_page': snapshot(page.GetPageManager().GetVariable('file_search_context').GetLocationContext()),
                               'fallback': snapshot(ac.GetFileSearchContext().GetLocationContext())})
            options.SetDefaultLocalLocationContext(ClientLocation.LocationContext(current_service_keys=[b'missing synthetic file domain']))
            missing = snapshot(options.GetDefaultLocalLocationContext())
        finally:
            options.SetDefaultLocalLocationContext(before)
            selector.deleteLater()
            panel.deleteLater()
        return {'initial': initial, 'choices': choices, 'events': events, 'missing': missing}
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
    output = os.path.join(HERE, 'fixtures', 'default_search_location.json')
    with open(output, 'w') as stream:
        json.dump(result, stream, indent=2)
        stream.write('\n')
    print('wrote ' + output)


if __name__ == '__main__':
    main()

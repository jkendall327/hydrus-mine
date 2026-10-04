#!/usr/bin/env python3
"""Record the real default-tag-service option and the new-search-page consumer.

For every offered service, FileSearchPanel.UpdateOptions persists its key, then
NewPageQuery creates a real page whose manager's tag context is captured. A
missing stored service falls back to all known tags, matching the live notebook.
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
        controller = session.controller
        options = controller.new_options
        panel = FileSearchPanel.FileSearchPanel(controller.gui, options)
        control = panel._default_tag_service_search_page
        choices = [{'name': control.itemText(i), 'key': control.itemData(i).hex()} for i in range(control.count())]
        initial = control.GetValue().hex()
        events = []
        location = ClientLocation.LocationContext.STATICCreateSimple(CC.LOCAL_FILE_SERVICE_KEY)
        def create(name):
            page = controller.gui._notebook.NewPageQuery(location, page_name=name, select_page=False)
            context = page.GetPageManager().GetVariable('file_search_context')
            events.append({'saved': options.GetKey('default_tag_service_search_page').hex(), 'page_service': context.GetTagContext().service_key.hex()})
        try:
            for index in range(control.count()):
                control.setCurrentIndex(index)
                panel.UpdateOptions()
                create('default service recording')
            options.SetKey('default_tag_service_search_page', b'missing service')
            create('missing default recording')
            missing_panel = FileSearchPanel.FileSearchPanel(controller.gui, options)
            missing_control = missing_panel._default_tag_service_search_page.GetValue().hex()
            missing_panel.deleteLater()
        finally:
            options.SetKey('default_tag_service_search_page', bytes.fromhex(initial))
            panel.deleteLater()
        return {'initial': initial, 'choices': choices, 'events': events, 'missing_control': missing_control}
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
    output = os.path.join(HERE, 'fixtures', 'search_default_service.json')
    with open(output, 'w') as stream:
        json.dump(result, stream, indent=2)
        stream.write('\n')
    print('wrote ' + output)


if __name__ == '__main__':
    main()

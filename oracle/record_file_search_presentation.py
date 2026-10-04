#!/usr/bin/env python3
"""Record FileSearchPanel list-height/float controls and actual new-page widgets.

The real notebook constructs read autocomplete for each setting combination.
Record font-row size hints, allowed ranges and floating versus layout embedding,
including the original page retaining its presentation after preferences change.
"""
import json
import os
import sys
import tempfile

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)


def record(session):
    def qt():
        from qtpy import QtWidgets as QW
        from hydrus.client import ClientConstants as CC, ClientLocation
        from hydrus.client.gui.panels.options import FileSearchPanel
        controller = session.controller
        options = controller.new_options
        panel = FileSearchPanel.FileSearchPanel(controller.gui, options)
        controls = (panel._active_search_predicates_height_num_chars, panel._ac_read_list_height_num_chars)
        initial = {'active_rows': controls[0].value(), 'autocomplete_rows': controls[1].value(),
                   'floating': panel._autocomplete_float_main_gui.isChecked()}
        before = initial.copy()
        events = []
        first = None
        def view(ac):
            def dimensions(widget):
                return {'rows': widget._height_num_chars, 'hint': widget.sizeHint().height(),
                        'text_height': widget.fontMetrics().height(), 'frame': widget.frameWidth()}
            return {'active': dimensions(ac._predicates_listbox),
                    'autocomplete': dimensions(ac._search_results_list),
                    'floating': ac._float_mode,
                    'embedded_in_layout': ac._main_vbox.indexOf(ac._dropdown_window) >= 0}
        try:
            for active, suggestions, floating in ((6, 22, True), (6, 22, False), (1, 128, False), (128, 1, True)):
                controls[0].setValue(active)
                controls[1].setValue(suggestions)
                panel._autocomplete_float_main_gui.setChecked(floating)
                panel.UpdateOptions()
                page = controller.gui._notebook.NewPageQuery(ClientLocation.LocationContext.STATICCreateSimple(CC.LOCAL_FILE_SERVICE_KEY), page_name='synthetic search presentation recording', select_page=True)
                ac = page.GetSidebar()._tag_autocomplete
                if first is None:
                    first = ac
                QW.QApplication.processEvents()
                events.append({'active_rows': active, 'autocomplete_rows': suggestions, 'floating': floating,
                               'view': view(ac), 'first_view': view(first)})
        finally:
            options.SetInteger('active_search_predicates_height_num_chars', before['active_rows'])
            options.SetInteger('ac_read_list_height_num_chars', before['autocomplete_rows'])
            options.SetBoolean('autocomplete_float_main_gui', before['floating'])
            panel.deleteLater()
        return {'initial': initial, 'bounds': [{'min': w.minimum(), 'max': w.maximum()} for w in controls], 'events': events}
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
    output = os.path.join(HERE, 'fixtures', 'file_search_presentation.json')
    with open(output, 'w') as stream:
        json.dump(result, stream, indent=2)
        stream.write('\n')
    print('wrote ' + output)


if __name__ == '__main__':
    main()

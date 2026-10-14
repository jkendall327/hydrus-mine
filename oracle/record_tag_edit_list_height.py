#!/usr/bin/env python3
"""Record the reference's write-autocomplete results list under the
`ac_write_list_height_num_chars` option (Options > tag editing > tag edit
autocomplete > Autocomplete list height).

The real `AutoCompleteDropdownTagsWrite` is built for each height, shown in a
window tall enough for it, and the results list's size hint and its laid-out
height are recorded, with the font height and frame the hint is made from
(`rows * text_height + 2 * frame`). The option's limits come from the real
`TagEditingPanel`.

Usage: QT_QPA_PLATFORM=offscreen ~/pyenv/bin/python oracle/record_tag_edit_list_height.py
       (writes fixtures/tag_edit_list_height.json)
"""
import json
import os
import sys
import tempfile

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
OUT = os.path.join(HERE, 'fixtures', 'tag_edit_list_height.json')
HEIGHTS = [1, 3, 11, 40]


def record(session):
    from qtpy import QtWidgets as QW
    from hydrus.client import ClientConstants as CC, ClientLocation
    from hydrus.client.gui.search import ClientGUIACDropdown as A
    from hydrus.core import HydrusConstants as HC
    c = session.controller
    local = next(s.GetServiceKey() for s in c.services_manager.GetServices((HC.LOCAL_TAG,)) if s.GetName() == 'my tags')
    location = ClientLocation.LocationContext.STATICCreateSimple(CC.LOCAL_FILE_SERVICE_KEY)

    def work():
        from hydrus.client.gui.panels.options.TagEditingPanel import TagEditingPanel
        panel = TagEditingPanel(c.gui, c.new_options)
        control = panel._ac_write_list_height_num_chars
        out = {'initial': control.value(), 'minimum': control.minimum(), 'maximum': control.maximum(), 'heights': []}
        panel.deleteLater()
        original = c.new_options.GetInteger('ac_write_list_height_num_chars')
        try:
            for rows in HEIGHTS:
                c.new_options.SetInteger('ac_write_list_height_num_chars', rows)
                host = QW.QWidget()
                layout = QW.QVBoxLayout(host)
                ac = A.AutoCompleteDropdownTagsWrite(host, lambda tags: None, location, local)
                layout.addWidget(ac)
                host.resize(700, 1400)
                host.show()
                for _ in range(4):
                    QW.QApplication.processEvents()
                box = ac._search_results_list
                out['heights'].append({
                    'rows': rows,
                    'hint': box.sizeHint().height(),
                    'height': box.height(),
                    'text_height': box.fontMetrics().height(),
                    'frame': box.frameWidth(),
                    'host': [host.width(), host.height()],
                })
                host.hide()
                host.deleteLater()
        finally:
            c.new_options.SetInteger('ac_write_list_height_num_chars', original)
        return out
    return c.CallBlockingToQt(c.gui, work)


def main():
    import hydrus_driver
    if len(sys.argv) > 1 and sys.argv[1] == '--child':
        import record_api
        output = sys.argv[2]
        result = hydrus_driver.run_client(record_api.unpack_fixture('basic'), record)
        with open(output, 'w') as f:
            json.dump(result, f)
        return
    with tempfile.TemporaryDirectory() as folder:
        path = os.path.join(folder, 'result.json')
        hydrus_driver.run_in_subprocess(os.path.abspath(__file__), '--child', path)
        with open(path) as f:
            result = json.load(f)
    with open(OUT, 'w') as f:
        json.dump(result, f, indent=2)
        f.write('\n')
    print('wrote', OUT)


if __name__ == '__main__':
    main()

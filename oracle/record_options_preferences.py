#!/usr/bin/env python3
"""Record initial/remembered option pages, search placement and auxiliary matches.

The real Qt ManageOptionsPanel is opened with remembering disabled, remembered
pages present/absent and search at top/bottom. Changing pages is recorded before
Apply, and a new panel verifies it remembers the choice. Also records the GUI
preference controls and all completer suggestions to prove unit/combo matches.
"""
import json
import os
import sys
import tempfile

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)


def record(session):
    def qt():
        from hydrus.client.gui.panels.options import ClientGUIManageOptionsPanel
        import record_options_dialog
        options = session.controller.new_options
        cases = []
        for remember, last, top in [(False, 'audio', True), (True, 'connection', True), (True, 'absent native page', False)]:
            options.SetBoolean('remember_options_window_panel', remember)
            options.SetBoolean('options_search_bar_top_of_window', top)
            options.SetString('last_options_window_panel', last)
            panel = ClientGUIManageOptionsPanel.ManageOptionsPanel(session.controller.gui)
            book = panel._listbook
            opened = next(book.tabText(i) for i, page in enumerate(book.GetPages()) if page is book.currentWidget())
            layout = panel.widget().layout()
            search_index = next(i for i in range(layout.count()) if layout.itemAt(i).widget() is panel._options_search)
            book.SelectName('audio')
            remembered = options.GetString('last_options_window_panel')
            suggestions = panel._completer.model().stringList()
            gui = next(book.widget(i) for i in range(book.count()) if book.tabText(i) == 'gui')
            gui_items = record_options_dialog.walk(gui.layout())
            panel.deleteLater()
            reopened = ClientGUIManageOptionsPanel.ManageOptionsPanel(session.controller.gui)
            reopened_name = next(reopened._listbook.tabText(i) for i, page in enumerate(reopened._listbook.GetPages()) if page is reopened._listbook.currentWidget())
            reopened.deleteLater()
            cases.append({'remember': remember, 'last': last, 'top': top, 'opened': opened, 'remembered_after_page_change': remembered, 'reopened': reopened_name, 'search_index': search_index})
        return {'cases': cases, 'suggestions': suggestions, 'gui_items': gui_items}
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
    output = os.path.join(HERE, 'fixtures', 'options_preferences.json')
    with open(output, 'w') as stream:
        json.dump(result, stream, indent=2, ensure_ascii=False)
        stream.write('\n')
    print('wrote ' + output)


if __name__ == '__main__':
    main()

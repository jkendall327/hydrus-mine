#!/usr/bin/env python3
"""Record "Update default file sort every time a new sort is manually chosen"
(`save_page_sort_on_change`) acting on the real sort control of a real search page: with
the option off and on, a sort type is chosen and the order flipped (the user routes
`_SetSortTypeFromUser` and `EventSortAscChoice`), and after each the default sort the
options hold and the sort a newly opened page starts with are recorded.
"""
import json, os, sys, tempfile
HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)

def record(session):
    controller = session.controller
    gui = controller.gui
    def work():
        from qtpy import QtWidgets as QW, QtTest as QT
        from hydrus.client import ClientConstants as CC, ClientLocation
        from record_media_collect import sort_facts
        options = controller.new_options
        location = ClientLocation.LocationContext.STATICCreateSimple(CC.LOCAL_FILE_SERVICE_KEY)
        def settle():
            QT.QTest.qWait(50)
            QW.QApplication.processEvents()
        def facts(sort):
            f = sort_facts(sort)
            return f
        page = gui.NewPageQuery(location, page_name='sort update', select_page=True)
        settle()
        control = page.GetSidebar()._media_sort_widget
        result = {'cases': []}
        for save in (False, True):
            options.SetBoolean('save_page_sort_on_change', save)
            from hydrus.client.media import ClientMediaSort
            control.SetSort(ClientMediaSort.MediaSort(('system', CC.SORT_FILES_BY_IMPORT_TIME), CC.SORT_ASC))
            before = facts(options.GetDefaultSort())
            steps = []
            control._SetSortTypeFromUser(('system', CC.SORT_FILES_BY_NUM_TAGS))
            settle()
            steps.append({'step': 'type num tags', 'page_sort': facts(control.GetSort()), 'default': facts(options.GetDefaultSort())})
            ascending = control.GetSort().sort_order
            control._sort_order_choice.SetValue(CC.SORT_DESC if ascending == CC.SORT_ASC else CC.SORT_ASC)
            control.EventSortAscChoice()
            settle()
            steps.append({'step': 'flip order', 'page_sort': facts(control.GetSort()), 'default': facts(options.GetDefaultSort())})
            other = gui.NewPageQuery(location, page_name='sort update new', select_page=True)
            settle()
            new_sort = facts(other.GetSidebar()._media_sort_widget.GetSort())
            result['cases'].append({'save': save, 'default_before': before, 'steps': steps, 'new_page_sort': new_sort})
            options.SetDefaultSort(ClientMediaSort.MediaSort(('system', CC.SORT_FILES_BY_IMPORT_TIME), CC.SORT_DESC))
        result['save_default'] = False
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
    with open(os.path.join(HERE, 'fixtures/default_sort_update.json'), 'w') as f:
        json.dump(result, f, indent=1, ensure_ascii=False)
        f.write('\n')
    print('wrote default_sort_update.json')
if __name__ == '__main__': main()

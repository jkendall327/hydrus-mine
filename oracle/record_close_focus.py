#!/usr/bin/env python3
"""Record where focus goes when a tab is closed (`close_page_focus_goes`).

Fresh temporary `basic` client. In a real `PagesNotebook` of five search pages
named a-e, for the option left and right: every page is made current in turn
and every page is closed through `_ClosePage` (what a tab's close does), and
the page that is current afterwards is recorded, by name (and its index),
including the edges (the first tab closed with "left", the last with
"right"), closing a tab that is not the current one, and closing the only
page left.
"""
import json
import os
import sys
import tempfile
HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
OUT = os.path.join(HERE, 'fixtures', 'close_focus.json')
NAMES = ['a', 'b', 'c', 'd', 'e']


def record(session):
    from hydrus.client import ClientConstants as CC, ClientLocation
    from hydrus.client.gui.pages import ClientGUIPages
    c = session.controller
    gui = c.gui

    def work():
        context = ClientLocation.LocationContext.STATICCreateSimple(CC.LOCAL_FILE_SERVICE_KEY)
        out = dict(options={str(v): CC.closed_page_focus_string_lookup[v] for v in CC.closed_page_focus_string_lookup},
                   default=c.new_options.GetInteger('close_page_focus_goes'), cases=[])
        for option in sorted(CC.closed_page_focus_string_lookup):
            c.new_options.SetInteger('close_page_focus_goes', option)
            for current in range(len(NAMES)):
                for closing in range(len(NAMES)):
                    notebook = ClientGUIPages.PagesNotebook(gui, 'recording')
                    for name in NAMES:
                        notebook.NewPageQuery(context, page_name=name, select_page=False)
                    notebook.setCurrentIndex(current)
                    notebook._ClosePage(closing, polite=False)
                    index = notebook.currentIndex()
                    names = [notebook.widget(i).GetName() for i in range(notebook.count())]
                    shown = notebook.widget(index).GetName() if 0 <= index < notebook.count() else None
                    out['cases'].append(dict(option=option, label=CC.closed_page_focus_string_lookup[option], current=current, closed=closing, remaining=names, current_index=index, shown=shown))
                    notebook.deleteLater()
        c.new_options.SetInteger('close_page_focus_goes', out['default'])
        return out
    return c.CallBlockingToQt(gui, work)


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
        json.dump(result, f, indent=2, ensure_ascii=False)
        f.write('\n')
    print('wrote', OUT)


if __name__ == '__main__':
    main()

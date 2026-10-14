#!/usr/bin/env python3
"""Record which tab has focus after a tab is closed, under each value of "When closing the
current tab, move focus" (`close_page_focus_goes`), on the real notebook of the basic client.

The main notebook is given four search pages a, b, c and d. For each option, each tab is
selected in turn and closed (`_ClosePage`, as a middle click or ctrl+w does), and each tab
other than the closed one is selected and the closed one closed; the page left current
and the tabs left are recorded. Then the notebook is reset.

Usage: QT_QPA_PLATFORM=offscreen ~/pyenv/bin/python oracle/record_close_focus.py
       (writes fixtures/close_focus.json)
"""
import json, os, sys, tempfile, time
HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)

NAMES = ['a', 'b', 'c', 'd']

def record(session):
    controller = session.controller
    gui = controller.gui
    notebook = gui._notebook
    def qt(f):
        return controller.CallBlockingToQt(gui, f)
    def names():
        return [p.GetName() for p in notebook.GetPages()]
    def current():
        page = notebook.GetCurrentMediaPage()
        return None if page is None else page.GetName()
    def setup():
        while notebook.count() > 0:
            notebook.widget(0).deleteLater()
            notebook.removeTab(0)
        del notebook._closed_pages[:]
        location_context = controller.new_options.GetDefaultLocalLocationContext()
        for name in NAMES:
            notebook.NewPageQuery(location_context, page_name=name)
    result = {'default': controller.new_options.GetInteger('close_page_focus_goes'), 'cases': []}
    for option in (0, 1):
        for closed in range(len(NAMES)):
            for selected in range(len(NAMES)):
                def run():
                    setup()
                    controller.new_options.SetInteger('close_page_focus_goes', option)
                    notebook.setCurrentIndex(selected)
                    before = current()
                    notebook._ClosePage(closed, polite=False)
                    return {'option': option, 'closed': NAMES[closed], 'selected': before, 'tabs': names(), 'current': current(), 'current_index': notebook.currentIndex()}
                result['cases'].append(qt(run))
                time.sleep(0.05)
    qt(setup)
    qt(lambda: controller.new_options.SetInteger('close_page_focus_goes', result['default']))
    return result

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
    with open(os.path.join(HERE, 'fixtures/close_focus.json'), 'w') as f:
        json.dump(result, f, indent=1, ensure_ascii=False)
        f.write('\n')
    print('wrote close_focus.json')
if __name__ == '__main__': main()

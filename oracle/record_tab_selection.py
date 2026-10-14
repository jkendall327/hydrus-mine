#!/usr/bin/env python3
"""Record selecting pages: clicking a tab (setCurrentIndex) and the
ctrl+page up / ctrl+page down keys (the main window calls
`PagesNotebook.MoveSelection(-1 / 1)` on the top notebook; the deepest
notebook goes first unless the notebook itself moved in the last three
seconds). Pages a, n[b, c], d; after each step: the page shown and the
selected tab of the top notebook and of n.

Usage: QT_QPA_PLATFORM=offscreen ~/pyenv/bin/python oracle/record_tab_selection.py
       (writes fixtures/tab_selection.json)
"""
import json
import sys
import tempfile
from pathlib import Path
HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))


def record(session):
    from hydrus.client import ClientConstants as CC, ClientLocation
    from hydrus.client.gui.pages import ClientGUIPages
    controller = session.controller
    gui = controller.gui
    context = ClientLocation.LocationContext.STATICCreateSimple(CC.LOCAL_FILE_SERVICE_KEY)

    def drive():
        top = ClientGUIPages.PagesNotebook(gui, 'oracle notebook')
        top.NewPageQuery(context, page_name='a', forced_insertion_index=0)
        nested = top.NewPagesNotebook(name='n', forced_insertion_index=1, give_it_a_blank_page=False)
        nested.NewPageQuery(context, page_name='b', forced_insertion_index=0)
        nested.NewPageQuery(context, page_name='c', forced_insertion_index=1)
        top.NewPageQuery(context, page_name='d', forced_insertion_index=2)
        nested.setCurrentIndex(0)
        steps = []

        def state(do):
            shown = top.GetCurrentMediaPage()
            steps.append({'do': do, 'shown': shown.GetName() if shown else None,
                'top': top.currentIndex(), 'nested': nested.currentIndex()})
        top.setCurrentIndex(0)
        state(['click', 0])
        top.setCurrentIndex(1)
        state(['click', 1])
        top.MoveSelection(1)
        state(['key', 'ctrl+page down'])
        top.MoveSelection(1)
        state(['key', 'ctrl+page down'])
        top.MoveSelection(-1)
        state(['key', 'ctrl+page up'])
        nested.setCurrentIndex(0)
        state(['click inner', 0])
        top.setCurrentIndex(2)
        state(['click', 2])
        top.setCurrentIndex(1)
        state(['click', 1])
        top.setCurrentIndex(0)
        state(['click', 0])
        top.MoveSelection(-1)
        state(['key', 'ctrl+page up'])
        return {'steps': steps}
    return controller.CallBlockingToQt(gui, drive)


def main():
    import hydrus_driver, record_api
    if len(sys.argv) > 1 and sys.argv[1] == '--child':
        Path(sys.argv[2]).write_text(json.dumps(hydrus_driver.run_client(record_api.unpack_fixture('basic'), record)))
        return
    with tempfile.TemporaryDirectory() as work:
        out = Path(work) / 'selection.json'
        hydrus_driver.run_in_subprocess(str(Path(__file__).resolve()), '--child', str(out))
        result = json.loads(out.read_text())
    (HERE / 'fixtures' / 'tab_selection.json').write_text(json.dumps(result, indent=2) + '\n')


if __name__ == '__main__':
    main()

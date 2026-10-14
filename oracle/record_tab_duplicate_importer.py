#!/usr/bin/env python3
"""Record duplicating a URL importer page from its tab's menu
(`_DuplicatePage`): the copy sits beside the original with the same name,
the same queued URLs (in order) and the same paused state, and it is its
own queue afterwards: a URL pended to the original does not reach the copy,
nor one pended to the copy the original.

Usage: QT_QPA_PLATFORM=offscreen ~/pyenv/bin/python oracle/record_tab_duplicate_importer.py
       (writes fixtures/tab_duplicate_importer.json)
"""
import json
import sys
import tempfile
import time
from pathlib import Path
HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))


def record(session):
    from hydrus.client import ClientConstants as CC, ClientLocation
    from hydrus.client.gui.pages import ClientGUIPages
    controller = session.controller
    gui = controller.gui
    context = ClientLocation.LocationContext.STATICCreateSimple(CC.LOCAL_FILE_SERVICE_KEY)
    qt = lambda f: controller.CallBlockingToQt(gui, f)

    def urls_of(page):
        urls = page.GetPageManager().GetVariable('urls_import')
        return [fs.file_seed_data for fs in urls.GetFileSeedCache().GetFileSeeds()]

    def paused_of(page):
        return bool(page.GetPageManager().GetVariable('urls_import').IsPaused())

    def build():
        top = ClientGUIPages.PagesNotebook(gui, 'duplicate importer')
        top.NewPageQuery(context, page_name='before', forced_insertion_index=0)
        importer = top.NewPageImportURLs(page_name='my importer')
        urls = importer.GetPageManager().GetVariable('urls_import')
        if not urls.IsPaused():
            urls.PausePlay()
        urls.PendURLs(['https://files.example/%d.jpg' % i for i in range(3)])
        top.NewPageQuery(context, page_name='after', forced_insertion_index=2)
        return top, importer
    top, importer = qt(build)
    for _ in range(200):
        if qt(lambda: importer._initialised):
            break
        time.sleep(0.025)
    steps = []

    def names():
        return [top.widget(i).GetName() for i in range(top.count())]

    names_before = qt(names)
    at = names_before.index('my importer')

    def duplicate():
        top._DuplicatePage(at)
        return top.widget(at + 1)
    copy = qt(duplicate)
    for _ in range(200):
        if qt(lambda: copy._initialised):
            break
        time.sleep(0.025)
    steps.append({'label': 'duplicated', 'importer_index': at, 'names_before': names_before, 'names': qt(names), 'original_urls': qt(lambda: urls_of(importer)),
        'copy_urls': qt(lambda: urls_of(copy)), 'original_paused': qt(lambda: paused_of(importer)),
        'copy_paused': qt(lambda: paused_of(copy)), 'selected': qt(lambda: top.currentIndex())})
    qt(lambda: importer.GetPageManager().GetVariable('urls_import').PendURLs(['https://files.example/original-only.jpg']))
    steps.append({'label': 'pended to the original', 'original_urls': qt(lambda: urls_of(importer)), 'copy_urls': qt(lambda: urls_of(copy))})
    qt(lambda: copy.GetPageManager().GetVariable('urls_import').PendURLs(['https://files.example/copy-only.jpg']))
    steps.append({'label': 'pended to the copy', 'original_urls': qt(lambda: urls_of(importer)), 'copy_urls': qt(lambda: urls_of(copy))})
    time.sleep(1)
    return {'steps': steps}


def main():
    import hydrus_driver, record_api
    if len(sys.argv) > 1 and sys.argv[1] == '--child':
        Path(sys.argv[2]).write_text(json.dumps(hydrus_driver.run_client(record_api.unpack_fixture('basic'), record)))
        return
    with tempfile.TemporaryDirectory() as work:
        out = Path(work) / 'dupe.json'
        hydrus_driver.run_in_subprocess(str(Path(__file__).resolve()), '--child', str(out))
        result = json.loads(out.read_text())
    (HERE / 'fixtures' / 'tab_duplicate_importer.json').write_text(json.dumps(result, indent=2) + '\n')


if __name__ == '__main__':
    main()

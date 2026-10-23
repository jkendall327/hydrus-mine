#!/usr/bin/env python3
"""Record when the reference highlights a new gallery query.

"If new query entered and no current highlight, highlight the new query"
(`highlight_new_query`) is read at the reference's two sites:

- `queries`: a gallery downloader page's query entry (`_PendQueries`), on a
  new page, entering 'blue_eyes' then 'red_eyes';
- `gap`: the subscription gap downloader (`CreateNewSubscriptionGapDownloader`
  -> `PendSubscriptionGapDownloader`), called twice as the "sub gap
  downloader test" popup's button calls it (downloader 'booru tag search',
  query 'skirt', file limit 2): the first makes the "subscription gap
  downloaders" page, the second adds to it.

Each with the option on and then off (the gap page closed in between).
Recorded after each entry: the page's name, its queries (text and file
limit) and which query is highlighted (its index in them, or null). The network is
paused and the page's queues start paused, so nothing is fetched.

Usage: QT_QPA_PLATFORM=offscreen python oracle/record_highlight_new_query.py
       (writes fixtures/highlight_new_query.json)
"""
import json
import os
import sys
import tempfile
import time

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)


def record(session):
    controller = session.controller
    gui = controller.gui

    def qt(f):
        return controller.CallBlockingToQt(gui, f)

    from hydrus.client.networking import ClientNetworkingGUG
    from hydrus.client.importing.options import ImportOptionsContainer

    gug = ClientNetworkingGUG.GalleryURLGenerator('booru tag search', url_template='https://booru.example/search/%tags%/1', replacement_phrase='%tags%', search_terms_separator='+', initial_search_text='tag', example_search_text='blue_eyes')
    controller.network_engine.domain_manager.SetGUGs([gug])

    def state(sidebar):
        imports = sidebar._multiple_gallery_import
        highlighted = sidebar._highlighted_gallery_import
        queries = list(imports.GetGalleryImports())
        return {
            'page': sidebar._page.GetName(),
            'queries': [[q.GetQueryText(), q.GetFileLimit()] for q in queries],
            'highlighted': next((i for (i, q) in enumerate(queries) if q is highlighted), None),
        }

    out = {'queries': [], 'gap': []}
    for on in (True, False):
        qt(lambda: controller.new_options.SetBoolean('highlight_new_query', on))

        def new_page():
            page = gui._notebook.NewPageImportGallery()
            sidebar = page.GetSidebar()
            sidebar._multiple_gallery_import.SetStartFileQueuesPaused(True)
            sidebar._multiple_gallery_import.SetStartGalleryQueuesPaused(True)
            sidebar._multiple_gallery_import.SetGUGKeyAndName((gug.GetGUGKey(), gug.GetName()))
            return sidebar

        sidebar = qt(new_page)
        steps = []
        for query in ('blue_eyes', 'red_eyes'):
            qt(lambda: sidebar._PendQueries([query]))
            # (a highlight loads on another thread)
            time.sleep(1)
            steps.append(qt(lambda: state(sidebar)))
        out['queries'].append({'option': on, 'steps': steps})
        qt(lambda: gui._notebook.CloseCurrentPage(polite=False))

        steps = []
        for _ in range(2):
            qt(lambda: gui.CreateNewSubscriptionGapDownloader((b'', 'booru tag search'), 'skirt', ImportOptionsContainer.ImportOptionsContainer(), 2))
            time.sleep(1)

            def gap_state():
                page = gui._notebook.GetOrMakeGalleryDownloaderPage(desired_page_name='subscription gap downloaders', select_page=True)
                return state(page.GetSidebar())

            steps.append(qt(gap_state))
        out['gap'].append({'option': on, 'steps': steps})
        qt(lambda: gui._notebook.CloseCurrentPage(polite=False))

    return out


def main():
    import hydrus_driver
    if len(sys.argv) > 1 and sys.argv[1] == '--child':
        import record_api
        output_path = sys.argv[2]
        result = hydrus_driver.run_client(record_api.unpack_fixture('basic'), record)
        with open(output_path, 'w') as f:
            json.dump(result, f)
        return
    with tempfile.TemporaryDirectory() as work:
        out = os.path.join(work, 'result.json')
        hydrus_driver.run_in_subprocess(os.path.abspath(__file__), '--child', out)
        with open(out) as f:
            result = json.load(f)
    with open(os.path.join(HERE, 'fixtures/highlight_new_query.json'), 'w') as f:
        json.dump(result, f, indent=1, ensure_ascii=False)
        f.write('\n')
    print('wrote highlight_new_query.json')


if __name__ == '__main__':
    main()

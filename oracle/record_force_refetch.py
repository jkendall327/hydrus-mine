#!/usr/bin/env python3
"""Record the thumbnail menu's urls > force metadata refetch.

A URL class for the fixture's example.com/post/<id> URLs is added to the
client; for a file with such a URL, and for three files together, the menu
is built (the grid's own `GetMenu`) and the "urls" > "force metadata
refetch" rows read; each row is triggered with `GetYesNo` captured (the
question, answered no, then yes): the URLs the downloader page is sent, its
name and its prefetch options.

Usage: QT_QPA_PLATFORM=offscreen python oracle/record_force_refetch.py
       (writes fixtures/force_refetch.json)
"""
import json
import os
import sys
import time

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
OUT = os.path.join(HERE, 'fixtures', 'force_refetch.json')
MANIFEST = json.load(open(os.path.join(HERE, 'fixtures', 'legacy_db', 'basic.manifest.json')))


def record(session):
    try:
        return _record(session)
    except BaseException:
        import traceback
        traceback.print_exc()
        sys.stderr.flush()
        os._exit(1)


def _record(session):
    import inspect
    from qtpy import QtWidgets as QW
    from hydrus.core import HydrusConstants as HC
    from hydrus.client import ClientConstants as CC, ClientLocation
    from hydrus.client.gui import ClientGUIDialogsQuick
    from hydrus.client.media import ClientMediaFileFilter
    from hydrus.client.networking import ClientNetworkingURLClass as U
    controller = session.controller
    gui = controller.gui
    hashes = [bytes.fromhex(f['hash']) for f in MANIFEST['files']]
    defaults = inspect.signature(ClientGUIDialogsQuick.GetYesNo).parameters

    def qt(f):
        return controller.CallBlockingToQt(gui, f)

    def setup():
        manager = controller.network_engine.domain_manager
        url_class = U.URLClass(name='example post', url_type=HC.URL_TYPE_POST, url_class_key=bytes([9]) * 32,
            url_domain_mask=U.URLDomainMask(raw_domains=['example.com']),
            path_components=[(U.ClientStrings.StringMatch(match_type=U.ClientStrings.STRING_MATCH_FIXED, match_value='post', example_string='post'), None),
                (U.ClientStrings.StringMatch(match_type=U.ClientStrings.STRING_MATCH_ANY, example_string='123'), None)],
            parameters=[], example_url='https://example.com/post/123')
        manager.SetURLClasses([url_class])
    qt(setup)
    location = ClientLocation.LocationContext.STATICCreateSimple(CC.LOCAL_FILE_SERVICE_KEY)
    page = qt(lambda: gui._notebook.NewPageQuery(location, initial_hashes=hashes))
    panel = None
    for _ in range(600):
        panel = qt(lambda: page.GetMediaResultsPanel())
        if hasattr(panel, 'GetMenu') and len(panel._sorted_media) > 0:
            break
        time.sleep(0.05)
    media = qt(lambda: list(panel._sorted_media))
    asked, sent = [], []
    answer = [False]
    old_yesno = ClientGUIDialogsQuick.GetYesNo
    old_force = gui.RedownloadURLsForceFetch

    def yesno(win, message, **kwargs):
        asked.append(dict(message=message, title=kwargs.get('title', defaults['title'].default),
            yes=kwargs.get('yes_label', defaults['yes_label'].default), no=kwargs.get('no_label', defaults['no_label'].default)))
        return QW.QDialog.DialogCode.Accepted if answer[0] else QW.QDialog.DialogCode.Rejected

    def redownload(urls):
        # (the real method runs; what it does is read off the page it makes)
        sent.append(sorted(urls))
        return old_force(urls)
    ClientGUIDialogsQuick.GetYesNo = yesno
    gui.RedownloadURLsForceFetch = redownload
    cases = []
    try:
        with_urls = [m for m in media if any(controller.network_engine.domain_manager.GetURLClass(u) is not None for u in m.GetLocationsManager().GetURLs())]
        for label, picked in (('one file', with_urls[:1]), ('three files', with_urls[:3])):
            def select():
                panel._Select(ClientMediaFileFilter.FileFilter(ClientMediaFileFilter.FILE_FILTER_NONE))
                for i, m in enumerate(picked):
                    panel._HitMedia(m, i > 0, False)

            def refetch_rows():
                select()
                menu = panel.GetMenu()
                for action in menu.actions():
                    if action.menu() is not None and action.text() == 'urls':
                        for sub in action.menu().actions():
                            if sub.menu() is not None and sub.text() == 'force metadata refetch':
                                return [a for a in sub.menu().actions() if not a.isSeparator()]
                return []
            rows = qt(lambda: [a.text() for a in refetch_rows()])
            case = {'selection': label, 'hashes': [m.GetHash().hex() for m in picked],
                'urls': [sorted(m.GetLocationsManager().GetURLs()) for m in picked], 'rows': []}
            for row in rows:
                for yes in (False, True):
                    answer[0] = yes
                    asked.clear()
                    sent.clear()
                    tabs = qt(lambda: gui._notebook.count())

                    def trigger():
                        for a in refetch_rows():
                            if a.text() == row:
                                a.trigger()
                                return
                    qt(trigger)
                    time.sleep(0.5)
                    made = None
                    if yes:
                        def look():
                            found = []
                            for p in gui._notebook.GetMediaPages(False):
                                if p.GetName() == 'forced urls downloader':
                                    found.append(p)
                            if not found:
                                return None
                            p = found[0]
                            urls = p.GetPageManager().GetVariable('urls_import')
                            options = urls._import_options_container
                            prefetch = options.GetPrefetchImportOptions() if options is not None else None
                            return dict(name=p.GetName(), queued=[fs.file_seed_data for fs in urls.GetFileSeedCache().GetFileSeeds()],
                                fetch_even_if_url_known=bool(prefetch.ShouldFetchMetadataEvenIfURLKnownAndFileAlreadyInDB()) if prefetch else None,
                                fetch_even_if_hash_known=bool(prefetch.ShouldFetchMetadataEvenIfHashKnownAndFileAlreadyInDB()) if prefetch else None,
                                tabs_added=gui._notebook.count() - tabs)
                        made = qt(look)
                    case['rows'].append({'row': row, 'answer': yes, 'asked': list(asked), 'sent': list(sent), 'page': made})
            cases.append(case)
    finally:
        ClientGUIDialogsQuick.GetYesNo = old_yesno
        gui.RedownloadURLsForceFetch = old_force
    return {'cases': cases}


def main():
    import hydrus_driver
    import record_api
    result = hydrus_driver.run_client(record_api.unpack_fixture('basic'), record)
    with open(OUT, 'w') as f:
        json.dump(result, f, indent=1, ensure_ascii=False)
        f.write('\n')
    print(f'wrote {OUT}')


if __name__ == '__main__':
    main()

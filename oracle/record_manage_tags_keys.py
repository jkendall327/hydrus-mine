#!/usr/bin/env python3
"""Real ManageTagsPanel autocomplete: the keys that act only on an empty input.

Left/Right (tab left/right) move the autocomplete's tabs when the input is
empty; Up/Down (page left/right) move the service tabs when the input and the
current list are both empty; PageUp/PageDown (media previous/next) ask the
canvas when both are empty. The commands are sent to the real dropdown with
`ProcessApplicationCommand`, in every empty/non-empty state.
"""
import json, os, sys, tempfile
HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
OUT = os.path.join(HERE, 'fixtures', 'manage_tags_keys.json')


def record(session):
    from hydrus.core import HydrusConstants as HC
    from hydrus.client import ClientConstants as CC, ClientLocation
    from hydrus.client import ClientApplicationCommand as CAC
    from hydrus.client.media import ClientMediaSingle
    from hydrus.client.metadata import ClientContentUpdates as U
    from hydrus.client.gui.metadata.ClientGUIManageTags import ManageTagsPanel
    c = session.controller
    manifest = json.load(open(os.path.join(HERE, 'fixtures/legacy_db/basic.manifest.json')))
    hashes = [bytes.fromhex(f['hash']) for f in manifest['files'][:1]]
    local = next(s.GetServiceKey() for s in c.services_manager.GetServices((HC.LOCAL_TAG,)) if s.GetName() == 'my tags')
    names = [s.GetName() for s in c.services_manager.GetServices((HC.LOCAL_TAG,))]

    def media():
        return [ClientMediaSingle.MediaSingle(r) for r in c.Read('media_results', hashes)]

    def work():
        c.new_options.SetKey('default_tag_service_tab', local)
        published = []
        old_pub = c.pub
        c.pub = lambda topic, *a, **kw: published.append(topic) if topic in ('canvas_show_next', 'canvas_show_previous') else old_pub(topic, *a, **kw)
        p = ManageTagsPanel(c.gui, ClientLocation.LocationContext.STATICCreateSimple(CC.LOCAL_FILE_SERVICE_KEY), CC.TAG_PRESENTATION_MEDIA_VIEWER_MANAGE_TAGS, media(), immediate_commit=True, canvas_key=b'k')
        commands = {'tab_left': CAC.SIMPLE_AUTOCOMPLETE_IF_EMPTY_TAB_LEFT, 'tab_right': CAC.SIMPLE_AUTOCOMPLETE_IF_EMPTY_TAB_RIGHT,
                    'page_left': CAC.SIMPLE_AUTOCOMPLETE_IF_EMPTY_PAGE_LEFT, 'page_right': CAC.SIMPLE_AUTOCOMPLETE_IF_EMPTY_PAGE_RIGHT,
                    'media_previous': CAC.SIMPLE_AUTOCOMPLETE_IF_EMPTY_MEDIA_PREVIOUS, 'media_next': CAC.SIMPLE_AUTOCOMPLETE_IF_EMPTY_MEDIA_NEXT}
        results = []
        for text in ('', 'blu'):
            for list_filled in (False, True):
                for name, action in commands.items():
                    p._tag_services.setCurrentIndex(0)
                    page = p._tag_services.currentWidget()
                    ac = page._add_tag_box
                    ac._can_intercept_unusual_key_events = True
                    ac._text_ctrl.blockSignals(True); ac._text_ctrl.setText(text); ac._text_ctrl.blockSignals(False)
                    ac._dropdown_notebook.setCurrentIndex(0)
                    box = ac._dropdown_notebook.currentWidget()
                    # a non-empty list on the first tab: put the input's own tags in it
                    from hydrus.client.search import ClientSearchPredicate as P
                    box.SetPredicates([P.Predicate(P.PREDICATE_TYPE_TAG, 'zzz')] if list_filled else [])
                    start_tab, start_page = ac._dropdown_notebook.currentIndex(), p._tag_services.currentIndex()
                    published.clear()
                    matched = ac.ProcessApplicationCommand(CAC.ApplicationCommand.STATICCreateSimpleCommand(action))
                    results.append({'input': text, 'list_filled': list_filled, 'command': name, 'matched': bool(matched),
                                    'tab': [start_tab, ac._dropdown_notebook.currentIndex()], 'service_page': [start_page, p._tag_services.currentIndex()],
                                    'published': list(published)})
        # a wraparound from the last service and the last tab
        wrap = []
        for name in ('tab_left', 'tab_right', 'page_left', 'page_right'):
            last_page = p._tag_services.count() - 1
            p._tag_services.setCurrentIndex(last_page if name == 'page_right' else 0)
            page = p._tag_services.currentWidget(); ac = page._add_tag_box
            ac._can_intercept_unusual_key_events = True
            ac._text_ctrl.blockSignals(True); ac._text_ctrl.setText(''); ac._text_ctrl.blockSignals(False)
            last_tab = ac._dropdown_notebook.count() - 1
            ac._dropdown_notebook.setCurrentIndex(last_tab if name == 'tab_right' else 0)
            ac._dropdown_notebook.currentWidget().SetPredicates([])
            before = (ac._dropdown_notebook.currentIndex(), p._tag_services.currentIndex())
            ac.ProcessApplicationCommand(CAC.ApplicationCommand.STATICCreateSimpleCommand(commands[name]))
            wrap.append({'command': name, 'before': list(before), 'after': [ac._dropdown_notebook.currentIndex(), p._tag_services.currentIndex()]})
        info = {'services': names, 'num_tabs': ac._dropdown_notebook.count(), 'tab_names': [ac._dropdown_notebook.tabText(i) for i in range(ac._dropdown_notebook.count())]}
        p.deleteLater()
        c.pub = old_pub
        return {'info': info, 'results': results, 'wrap': wrap}
    return c.CallBlockingToQt(c.gui, work)


def child(out):
    import hydrus_driver, record_api
    result = hydrus_driver.run_client(record_api.unpack_fixture('basic'), record)
    with open(out, 'w') as f:
        json.dump(result, f)


def main():
    if len(sys.argv) > 1 and sys.argv[1] == '--child':
        child(sys.argv[2]); return
    import hydrus_driver
    with tempfile.TemporaryDirectory() as directory:
        path = os.path.join(directory, 'out.json')
        hydrus_driver.run_in_subprocess(os.path.abspath(__file__), '--child', path)
        with open(path) as f:
            result = json.load(f)
    with open(OUT, 'w') as f:
        json.dump(result, f, indent=2, ensure_ascii=False); f.write('\n')
    print('wrote', OUT)


if __name__ == '__main__':
    main()

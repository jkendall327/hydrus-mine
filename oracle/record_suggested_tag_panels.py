#!/usr/bin/env python3
"""Record the real SuggestedTagsPanel with its three panels (most used, related, recent).

Fresh temporary basic client only. Two files carry `parity:present`; other files
carry a small corpus that makes the related panel suggest tags. Most used, recent
and related are all on (file lookup scripts, out of scope, are off). For each
layout and default notebook page the panel is built over the two files, the
related panel is searched with its quick duration (its worker and publisher are
forwarded synchronously), and the labels, the selected page, the three lists and
what activating the first row of each list adds (and what is then left) are
recorded.
"""
import json
import os
import sys
import tempfile
HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
OUT = os.path.join(HERE, 'fixtures', 'suggested_tag_panels.json')

def record(session):
    from qtpy import QtWidgets as QW
    from hydrus.client import ClientConstants as CC
    from hydrus.client.gui import ClientGUITagSuggestions as S
    from hydrus.client.media import ClientMediaSingle
    from hydrus.client.metadata import ClientContentUpdates as U
    from hydrus.core import HydrusConstants as HC
    c = session.controller
    local = next(s.GetServiceKey() for s in c.services_manager.GetServices((HC.LOCAL_TAG,)) if s.GetName() == 'my tags')
    manifest = json.load(open(os.path.join(HERE, 'fixtures', 'legacy_db', 'basic.manifest.json')))
    hashes = [bytes.fromhex(f['hash']) for f in manifest['files'][:6]]
    corpus = [('parity:present', [0, 1]), ('source:seed', [0, 1, 2, 3]), ('context:seed', [2, 3]),
              ('alpha:first', [0, 1, 2]), ('beta:second', [3, 4, 5]), ('alpha:third', [2, 3, 4]), ('gamma:other', [0, 1, 4, 5])]
    c.WriteSynchronous('content_updates', U.ContentUpdatePackage.STATICCreateFromContentUpdates(local, [
        U.ContentUpdate(HC.CONTENT_TYPE_MAPPINGS, HC.CONTENT_UPDATE_ADD, (tag, {hashes[i] for i in files})) for tag, files in corpus]))
    c.WriteSynchronous('push_recent_tags', local, ['parity:recent'])
    most_used = ['parity:new10', 'parity:new2', 'parity:present', 'parity:漢字']
    c.new_options.SetSuggestedTagsMostUsed(local, most_used)
    c.new_options.SetBoolean('show_related_tags', True)
    c.new_options.SetBoolean('show_file_lookup_script_tags', False)
    c.new_options.SetNoneableInteger('num_recent_tags', 20)
    c.new_options.SetInteger('suggested_tags_width', 240)
    c.new_options.SetRelatedTagsTagSliceWeights([['', 100], [':', 100]], [['', 100], [':', 100]])
    c.new_options.SetInteger('related_tags_concurrence_threshold_percent', 6)
    c.new_options.SetInteger('related_tags_search_1_duration_ms', 250)
    medias = [ClientMediaSingle.MediaSingle(r) for r in c.Read('media_results', hashes[:2])]
    def work():
        def settle():
            for _ in range(4): QW.QApplication.processEvents()
        cases = []
        old_thread, old_after = c.CallToThread, c.CallAfterQtSafe
        c.CallToThread = lambda f, *a, **kw: f(*a, **kw) if getattr(f, '__name__', '') == 'do_it' else old_thread(f, *a, **kw)
        c.CallAfterQtSafe = lambda w, f, *a, **kw: f(*a, **kw) if getattr(f, '__name__', '') == 'qt_code' else old_after(w, f, *a, **kw)
        try:
            for mode in ['notebook', 'columns']:
                for page in ['favourites', 'related', 'file_lookup_scripts', 'recent']:
                    c.new_options.SetNoneableString('suggested_tags_layout', mode)
                    c.new_options.SetString('default_suggested_tags_notebook_page', page)
                    activated = []
                    panel = S.SuggestedTagsPanel(c.gui, local, CC.TAG_PRESENTATION_SEARCH_PAGE_MANAGE_TAGS, False, lambda tags, **kw: activated.append(dict(tags=sorted(tags), only_add=kw.get('only_add'))))
                    panel.SetMedia(medias)
                    panel.resize(900, 500); panel.show(); settle()
                    panel._related_tags.RefreshQuick(); settle()
                    fav, rel, recent = panel._favourite_tags._favourite_tags, panel._related_tags._related_tags, panel._recent_tags._recent_tags
                    labels = [panel._notebook.tabText(i) for i in range(panel._notebook.count())] if panel._notebook else ['most used', 'related', 'recent']
                    selected = panel._notebook.tabText(panel._notebook.currentIndex()) if panel._notebook else None
                    lists = lambda: dict(favourites=[t.GetTag() for t in fav._ordered_terms], related=[t.GetTag() for t in rel._ordered_terms], recent=[t.GetTag() for t in recent._ordered_terms])
                    before = lists()
                    after_activation = {}
                    for name, box in [('favourites', fav), ('related', rel), ('recent', recent)]:
                        box.TakeFocusForUser(); box._Activate(False, False)
                        after_activation[name] = lists()[name]
                    cases.append(dict(layout=mode, default=page, labels=labels, selected=selected, lists=before, activated=activated, remaining=after_activation))
                    panel.hide(); panel.deleteLater()
        finally:
            c.CallToThread, c.CallAfterQtSafe = old_thread, old_after
        return dict(most_used=most_used, files=[h.hex() for h in hashes], selected=[h.hex() for h in hashes[:2]], corpus=corpus, cases=cases)
    return c.CallBlockingToQt(c.gui, work)

def main():
    import hydrus_driver
    if len(sys.argv) > 1 and sys.argv[1] == '--child':
        import record_api
        out = sys.argv[2]
        result = hydrus_driver.run_client(record_api.unpack_fixture('basic'), record)
        with open(out, 'w') as f: json.dump(result, f)
        return
    with tempfile.TemporaryDirectory() as folder:
        path = os.path.join(folder, 'result.json')
        hydrus_driver.run_in_subprocess(os.path.abspath(__file__), '--child', path)
        with open(path) as f: result = json.load(f)
    with open(OUT, 'w') as f:
        json.dump(result, f, indent=2, ensure_ascii=False); f.write('\n')
    print('wrote', OUT)
if __name__ == '__main__': main()

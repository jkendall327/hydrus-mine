#!/usr/bin/env python3
"""Real Options per-service most-used edits and suggested-tag layout/activation.

Fresh temporary basic client only. Recent fetching is forwarded synchronously to
the real reader/Qt publisher; related/lookup workers are disabled by preferences.
No parsing, list filtering, activation, sorting or options handlers are replaced.
"""
import json
import os
import sys
import tempfile
HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
OUT = os.path.join(HERE, 'fixtures', 'tag_suggestions.json')

def record(session):
    from qtpy import QtWidgets as QW
    from hydrus.client import ClientConstants as CC
    from hydrus.client.gui import ClientGUITagSuggestions as S
    from hydrus.client.gui.panels.options.TagSuggestionsPanel import TagSuggestionsPanel
    from hydrus.client.media import ClientMediaSingle
    from hydrus.client.metadata import ClientContentUpdates as U
    from hydrus.core import HydrusConstants as HC, HydrusText
    c = session.controller
    local = next(s.GetServiceKey() for s in c.services_manager.GetServices((HC.LOCAL_TAG,)) if s.GetName() == 'my tags')
    second = next(s.GetServiceKey() for s in c.services_manager.GetServices((HC.LOCAL_TAG,)) if s.GetName() == 'second tags')
    manifest = json.load(open(os.path.join(HERE, 'fixtures', 'legacy_db', 'basic.manifest.json')))
    hashes = [bytes.fromhex(f['hash']) for f in manifest['files'][:2]]
    c.WriteSynchronous('content_updates', U.ContentUpdatePackage.STATICCreateFromContentUpdates(local, [U.ContentUpdate(HC.CONTENT_TYPE_MAPPINGS, HC.CONTENT_UPDATE_ADD, ('parity:present', set(hashes)))]))
    c.WriteSynchronous('push_recent_tags', local, ['parity:recent'])
    medias = [ClientMediaSingle.MediaSingle(r) for r in c.Read('media_results', hashes)]
    def work():
        def settle():
            for _ in range(4): QW.QApplication.processEvents()
        original = c.new_options.GetSuggestedTagsMostUsed(local)
        draft = c.new_options.Duplicate()
        p = TagSuggestionsPanel(c.gui, draft)
        p._suggested_most_used_services.SetValue(local)
        p.EventSuggestedMostUsedService(None)
        p._suggested_most_used.AddTags(['parity:present', 'parity:new10', 'parity:new2', 'parity:漢字'])
        p._suggested_tags_width.setValue(240)
        p._suggested_tags_layout.SetValue('columns')
        p._default_suggested_tags_notebook_page.SetValue('recent')
        before = draft.GetSuggestedTagsMostUsed(local)
        rows = [t.GetTag() for t in p._suggested_most_used._ordered_terms]
        p._suggested_most_used_services.SetValue(second)
        p.EventSuggestedMostUsedService(None)
        p._suggested_most_used.AddTags(['parity:second', 'parity:removed'])
        p._suggested_most_used.RemoveTags(['parity:removed'])
        p._suggested_most_used_services.SetValue(local)
        p.EventSuggestedMostUsedService(None)
        retained = [t.GetTag() for t in p._suggested_most_used._ordered_terms]
        p.UpdateOptions()
        edited = dict(tags=sorted(draft.GetSuggestedTagsMostUsed(local), key=HydrusText.HumanTextSortKey), width=draft.GetInteger('suggested_tags_width'), layout=draft.GetNoneableString('suggested_tags_layout'), page=draft.GetString('default_suggested_tags_notebook_page'))
        p.hide(); p.deleteLater()
        reopened = TagSuggestionsPanel(c.gui, draft)
        reopened._suggested_most_used_services.SetValue(local)
        reopened.EventSuggestedMostUsedService(None)
        reopened_tags = [t.GetTag() for t in reopened._suggested_most_used._ordered_terms]
        reopened._suggested_most_used.AddTags(['parity:cancelled'])
        reopened.deleteLater()
        c.new_options.SetSuggestedTagsMostUsed(local, edited['tags'])
        c.new_options.SetBoolean('show_related_tags', False)
        c.new_options.SetBoolean('show_file_lookup_script_tags', False)
        c.new_options.SetNoneableInteger('num_recent_tags', 20)
        c.new_options.SetInteger('suggested_tags_width', 240)
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
                    panel.resize(740, 400); panel.show(); settle()
                    fav = panel._favourite_tags._favourite_tags
                    recent = panel._recent_tags._recent_tags
                    labels = [panel._notebook.tabText(i) for i in range(panel._notebook.count())] if panel._notebook else ['most used', 'recent']
                    selected = panel._notebook.tabText(panel._notebook.currentIndex()) if panel._notebook else None
                    visible = [t.GetTag() for t in fav._ordered_terms]
                    fav.TakeFocusForUser()
                    fav._Activate(False, False)
                    cases.append(dict(layout=mode, default=page, labels=labels, selected=selected, minimum_width=fav.minimumWidth(), favourites=visible, recent=[t.GetTag() for t in recent._ordered_terms], activated=activated, remaining=[t.GetTag() for t in fav._ordered_terms]))
                    if mode == 'columns' and page == 'recent': panel.grab().save(OUT.replace('.json', '.png'))
                    panel.hide(); panel.deleteLater()
        finally:
            c.CallToThread, c.CallAfterQtSafe = old_thread, old_after
        return dict(second=sorted(draft.GetSuggestedTagsMostUsed(second)), retained=retained, original=list(original), before_apply=list(before), option_rows=rows, edited=edited, reopened=reopened_tags, cancelled=sorted(draft.GetSuggestedTagsMostUsed(local), key=HydrusText.HumanTextSortKey), cases=cases)
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
        json.dump(result, f, indent=2); f.write('\n')
    print('wrote', OUT)
if __name__ == '__main__': main()

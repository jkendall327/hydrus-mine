#!/usr/bin/env python3
"""Record the tag editing options' three write-autocomplete decoration switches.

For every combination of "expand parents", "show parent info" and "show
sibling info" (eight), ticks the real Options > tag editing panel's checkboxes
and applies them with its own `UpdateOptions`, then builds a new real write
autocomplete (which reads those options when it is made) and runs the real
`WriteFetch` for a few texts, recording every rendered row. The corpus is the
one of `record_write_tag_autocomplete.py`: `parity:amber old` is a sibling of
`parity:amber`, whose parent is `parity:colour`, whose parent is `parity:root`.
"""
import json, os, sys
HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
OUT = os.path.join(HERE, 'fixtures', 'autocomplete_decoration_flags.json')
TEXTS = ['parity:amb', 'parity:amber old', 'parity:col']


def record(session):
    from hydrus.client import ClientConstants as CC, ClientLocation, ClientThreading
    from hydrus.client.gui.search import ClientGUIACDropdown as A
    from hydrus.client.metadata import ClientContentUpdates as U
    from hydrus.client.search import ClientSearchAutocomplete as SA, ClientSearchFileSearchContext as SC
    from hydrus.core import HydrusConstants as HC
    c = session.controller
    qt = lambda f: c.CallBlockingToQt(c.gui, f)
    local = qt(lambda: next(s.GetServiceKey() for s in c.services_manager.GetServices((HC.LOCAL_TAG,)) if s.GetName() == 'my tags'))
    manifest = json.load(open(os.path.join(HERE, 'fixtures', 'legacy_db', 'basic.manifest.json')))
    hashes = [bytes.fromhex(f['hash']) for f in manifest['files'][:4]]
    corpus = [('parity:amber', hashes[:3]), ('parity:amber old', hashes[:1]), ('parity:amethyst', hashes[1:2]), ('parity:amaranth', hashes[2:3])]
    updates = [U.ContentUpdate(HC.CONTENT_TYPE_MAPPINGS, HC.CONTENT_UPDATE_ADD, (tag, set(hs))) for tag, hs in corpus]
    updates += [U.ContentUpdate(HC.CONTENT_TYPE_TAG_SIBLINGS, HC.CONTENT_UPDATE_ADD, ('parity:amber old', 'parity:amber')), U.ContentUpdate(HC.CONTENT_TYPE_TAG_PARENTS, HC.CONTENT_UPDATE_ADD, ('parity:amber', 'parity:colour')), U.ContentUpdate(HC.CONTENT_TYPE_TAG_PARENTS, HC.CONTENT_UPDATE_ADD, ('parity:colour', 'parity:root'))]
    c.WriteSynchronous('content_updates', U.ContentUpdatePackage.STATICCreateFromContentUpdates(local, updates))
    for _ in range(12):
        if not c.WriteSynchronous('sync_tag_display_maintenance', local, 0.5): break
    old_thread = c.CallToThread
    c.CallToThread = lambda func, *args, **kw: None if func == A.WriteFetch else old_thread(func, *args, **kw)
    location = ClientLocation.LocationContext.STATICCreateSimple(CC.LOCAL_FILE_SERVICE_KEY)
    cases = []
    for expand in (False, True):
        for parents in (False, True):
            for siblings in (False, True):
                def set_options():
                    from hydrus.client.gui.panels.options.TagEditingPanel import TagEditingPanel
                    panel = TagEditingPanel(c.gui, c.new_options)
                    panel._expand_parents_on_storage_autocomplete_taglists.setChecked(expand)
                    panel._show_parent_decorators_on_storage_autocomplete_taglists.setChecked(parents)
                    panel._show_sibling_decorators_on_storage_autocomplete_taglists.setChecked(siblings)
                    panel.UpdateOptions()
                    panel.deleteLater()
                    return A.AutoCompleteDropdownTagsWrite(c.gui, lambda tags: None, location, local)
                ac = qt(set_options)
                for text in TEXTS:
                    def prepare():
                        ac._text_ctrl.setText(text)
                        ac.CancelCurrentResultsFetchJob()
                        return ac._GetParsedAutocompleteText(), SC.FileSearchContext(location_context=ac._location_context_button.GetValue(), tag_context=ac._tag_context_button.GetValue())
                    parsed, context = qt(prepare)
                    captured = {}
                    def prefetch(*args): pass
                    def results(job, parsed, cache, matches): captured['matches'] = matches
                    old_after = c.CallAfterQtSafe
                    c.CallAfterQtSafe = lambda target, func, *args, **kw: func(*args, **kw) if target == ac and func in (prefetch, results) else old_after(target, func, *args, **kw)
                    try: A.WriteFetch(ac, ClientThreading.JobStatus(), prefetch, results, parsed, context, SA.PredicateResultsCacheInit())
                    finally: c.CallAfterQtSafe = old_after
                    def show():
                        box = ac._search_results_list
                        box.SetPredicates([]); box.SetPredicates(captured['matches'])
                        rows = []
                        for term in box._ordered_terms:
                            # the list's own switches, as the dropdown set them from the options
                            texts = term.GetRowsOfPresentationTextsWithNamespaces(True, box._show_sibling_decorators, ' → ', None, box._show_parent_decorators, box._extra_parent_rows_allowed)
                            rows.append({'tag': term.GetPredicate().GetValue(), 'rows': [''.join(t[0] for t in row) for row in texts]})
                        return {'list_switches': [box._extra_parent_rows_allowed, box._show_parent_decorators, box._show_sibling_decorators], 'rows': rows}
                    shown = qt(show)
                    cases.append({'expand': expand, 'parents': parents, 'siblings': siblings, 'text': text, **shown})
                qt(lambda: ac.deleteLater())
    c.CallToThread = old_thread
    with open(OUT, 'w') as f:
        json.dump({'corpus': [{'tag': t, 'hashes': [h.hex() for h in hs]} for t, hs in corpus], 'siblings': [['parity:amber old', 'parity:amber']], 'parents': [['parity:amber', 'parity:colour'], ['parity:colour', 'parity:root']], 'cases': cases}, f, indent=2, ensure_ascii=False)
        f.write('\n')


if __name__ == '__main__':
    import shutil, record_api, hydrus_driver
    db_dir = record_api.unpack_fixture('basic')
    try:
        hydrus_driver.run_client(db_dir, record)
    finally:
        shutil.rmtree(db_dir)

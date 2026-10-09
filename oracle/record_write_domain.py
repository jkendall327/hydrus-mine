#!/usr/bin/env python3
"""Record the tag autocomplete options' write tag service and file domain override.

For `my tags`, the real `EditTagAutocompleteOptionsPanel` is given each write
tag domain (its own service, the other local tag service, all known tags),
with the file-domain override off and on (my files, trash, all known files);
its own `GetValue` goes into the client's tag display manager, as the dialog's
OK does. A new real write autocomplete for `my tags`, launched from "my
files" as Manage tags is, then reports the search domain it starts with and
the rows of a real `WriteFetch` for a few `wd:` prefixes, with their counts.
"""
import json, os, shutil, sys
HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
OUT = os.path.join(HERE, 'fixtures', 'write_domain.json')

TEXTS = ['wd:bot', 'wd:min', 'wd:dow']

# (service, tag, file indices)
MAPPINGS = [
    ('my tags', 'wd:mine', [0, 1]),
    ('my tags', 'wd:both', [0]),
    ('downloader tags', 'wd:both', [0, 1, 2]),
    ('downloader tags', 'wd:down', [2]),
]


def record(session):
    from hydrus.client import ClientConstants as CC, ClientLocation, ClientThreading
    from hydrus.client.gui.search import ClientGUIACDropdown as A
    from hydrus.client.metadata import ClientContentUpdates as U
    from hydrus.client.search import ClientSearchAutocomplete as SA, ClientSearchFileSearchContext as SC
    from hydrus.core import HydrusConstants as HC
    c = session.controller
    qt = lambda f: c.CallBlockingToQt(c.gui, f)
    services = {s.GetName(): s.GetServiceKey() for s in c.services_manager.GetServices((HC.LOCAL_TAG,))}
    manifest = json.load(open(os.path.join(HERE, 'fixtures', 'legacy_db', 'basic.manifest.json')))
    hashes = [bytes.fromhex(f['hash']) for f in manifest['files']]
    for name, tag, files in MAPPINGS:
        c.WriteSynchronous('content_updates', U.ContentUpdatePackage.STATICCreateFromContentUpdates(
            services[name], [U.ContentUpdate(HC.CONTENT_TYPE_MAPPINGS, HC.CONTENT_UPDATE_ADD, (tag, {hashes[i] for i in files}))]))
    # one tagged file goes to the trash
    c.WriteSynchronous('content_updates', U.ContentUpdatePackage.STATICCreateFromContentUpdates(
        CC.LOCAL_FILE_SERVICE_KEY, [U.ContentUpdate(HC.CONTENT_TYPE_FILES, HC.CONTENT_UPDATE_DELETE, {hashes[1]})]))
    old_thread = c.CallToThread
    c.CallToThread = lambda func, *args, **kw: None if func == A.WriteFetch else old_thread(func, *args, **kw)
    launched = ClientLocation.LocationContext.STATICCreateSimple(CC.LOCAL_FILE_SERVICE_KEY)
    domains = [('my tags', services['my tags']), ('downloader tags', services['downloader tags']), ('all known tags', CC.COMBINED_TAG_SERVICE_KEY)]
    locations = [('my files', CC.LOCAL_FILE_SERVICE_KEY), ('trash', CC.TRASH_SERVICE_KEY), ('all known files', CC.COMBINED_FILE_SERVICE_KEY)]
    cases = []

    def name_of(key):
        return c.services_manager.GetName(key)

    for domain_name, domain in domains:
        for override in [None] + locations:
            def make():
                from hydrus.client.gui.metadata import ClientGUITagDisplayOptions as O
                options = c.tag_display_manager.GetTagAutocompleteOptions(services['my tags'])
                panel = O.EditTagAutocompleteOptionsPanel(c.gui, options)
                panel._write_autocomplete_tag_domain.SetValue(domain)
                panel._override_write_autocomplete_location_context.setChecked(override is not None)
                if override is not None:
                    panel._write_autocomplete_location_context.SetValue(ClientLocation.LocationContext.STATICCreateSimple(override[1]))
                value = panel.GetValue()
                panel.deleteLater()
                c.tag_display_manager.SetTagAutocompleteOptions(value)
                return A.AutoCompleteDropdownTagsWrite(c.gui, lambda tags: None, launched, services['my tags'])
            ac = qt(make)
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
                    rows = [''.join(t[0] for t in term.GetRowsOfPresentationTextsWithNamespaces(True, True, ' → ', None, True, True)[0]) for term in box._ordered_terms]
                    location = ac._location_context_button.GetValue()
                    tag_context = ac._tag_context_button.GetValue()
                    return {
                        'location': sorted(name_of(k) for k in location.current_service_keys),
                        'tag_service': name_of(tag_context.service_key),
                        'display_service': name_of(tag_context.display_service_key),
                        'location_label': ac._location_context_button.text(),
                        'tag_label': ac._tag_context_button.text(),
                        'rows': rows,
                    }
                shown = qt(show)
                cases.append({'domain': domain_name, 'override': None if override is None else override[0], 'text': text, **shown})
            qt(lambda: ac.deleteLater())
    c.CallToThread = old_thread
    with open(OUT, 'w') as f:
        json.dump({'mappings': [[n, t, [hashes[i].hex() for i in fs]] for n, t, fs in MAPPINGS], 'trashed': [hashes[1].hex()], 'cases': cases}, f, indent=2, ensure_ascii=False)
        f.write('\n')


if __name__ == '__main__':
    import record_api, hydrus_driver
    db_dir = record_api.unpack_fixture('basic')
    try:
        hydrus_driver.run_client(db_dir, record)
    finally:
        shutil.rmtree(db_dir)

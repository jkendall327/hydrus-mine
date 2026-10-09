#!/usr/bin/env python3
"""Record the blacklist-only tag filter editor's sibling-aware live test.

Siblings are written to two real local tag services and synced, so the
reference's own `tag_siblings_lookup` (all real tag services combined) gives
each test tag its chains. The real `EditTagFilterPanel` (`only_show_blacklist`)
is made for a few blacklists and each test text is typed into its test box;
its own `_UpdateTest` worker runs at once and the result label and its style
are recorded. The same texts are also run through the panel without the
blacklist-only switch, where siblings are not consulted.
"""
import json, os, shutil, sys
HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
OUT = os.path.join(HERE, 'fixtures', 'tag_filter_sibling_testing.json')

# (service, old, new)
SIBLINGS = [
    ('my tags', 'sibtest:alias', 'sibtest:ideal'),
    ('my tags', 'sibtest:other alias', 'sibtest:ideal'),
    ('downloader tags', 'sibtest:alias', 'sibtest:elsewhere'),
    ('my tags', 'sibling blocked', 'sibtest:plain ideal'),
]

BLACKLISTS = [
    ['sibtest:other alias'],
    ['sibtest:elsewhere'],
    ['blocked', 'sibling blocked'],
    ['sibtest:'],
    [],
]

TEXTS = [
    'sibtest:ideal',
    'sibtest:alias',
    'sibtest:other alias',
    'sibtest:elsewhere',
    'sibtest:plain ideal',
    'character:blocked',
    'blocked',
    'never seen tag',
    'sibtest:ideal\nnever seen tag',
    'sibtest:alias\nsibtest:ideal',
    'never seen tag\nNever Seen Tag  ',
    'sibtest:ideal\nsibtest:elsewhere\nnever seen tag',
    '\n\n',
]


def record(session):
    c = session.controller
    from hydrus.core import HydrusConstants as HC, HydrusTags
    from hydrus.client.metadata import ClientContentUpdates as U
    services = {s.GetName(): s.GetServiceKey() for s in c.services_manager.GetServices((HC.LOCAL_TAG,))}
    for name, old, new in SIBLINGS:
        c.WriteSynchronous('content_updates', U.ContentUpdatePackage.STATICCreateFromContentUpdates(
            services[name], [U.ContentUpdate(HC.CONTENT_TYPE_TAG_SIBLINGS, HC.CONTENT_UPDATE_ADD, (old, new))]))
    session.sync_tag_display()

    def work():
        from hydrus.client.gui import ClientGUIAsync
        from hydrus.client.gui.metadata import ClientGUITagFilter

        class Now:
            def __init__(self, win, work_callable, publish_callable, **kwargs):
                self._work, self._publish = work_callable, publish_callable
            def start(self):
                self._publish(self._work())
        ClientGUIAsync.AsyncQtJob = Now
        out = []
        for blacklist in BLACKLISTS:
            tag_filter = HydrusTags.TagFilter()
            for rule in blacklist:
                tag_filter.SetRule(rule, HC.FILTER_BLACKLIST)
            for blacklist_only in (True, False):
                panel = ClientGUITagFilter.EditTagFilterPanel(c.gui, tag_filter, only_show_blacklist=blacklist_only, namespaces=['character', 'sibtest'])
                results = []
                for text in TEXTS:
                    panel._test_input.setPlainText(text)
                    label = panel._test_result_st
                    results.append({'text': text, 'result': label.text(), 'style': label.objectName()})
                panel._test_input.setPlainText('')
                out.append({'blacklist': blacklist, 'blacklist_only': blacklist_only, 'tests': results, 'empty': panel._test_result_st.text()})
                panel.deleteLater()
        return out

    result = c.CallBlockingToQt(c.gui, work)
    with open(OUT, 'w') as f:
        json.dump({'siblings': SIBLINGS, 'cases': result}, f, indent=2, ensure_ascii=False)
        f.write('\n')


if __name__ == '__main__':
    import record_api, hydrus_driver
    db_dir = record_api.unpack_fixture('basic')
    try:
        hydrus_driver.run_client(db_dir, record)
    finally:
        shutil.rmtree(db_dir)

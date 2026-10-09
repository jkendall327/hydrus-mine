#!/usr/bin/env python3
"""Record sibling/parent source precedence: the application queues and what they do.

`my tags` and `downloader tags` hold disagreeing siblings and different
parents. For each case the real `EditTagDisplayApplication` panel's `my tags`
page has its sibling and parent queues set (as its add/remove/up/down buttons
leave them), its own `GetValue` is written as the dialog's OK does, the
display caches are synced, and the reference's own lookup gives, for `my tags`,
each tag's ideal, its sibling chain and its parents (ancestors) and children.
"""
import json, os, shutil, sys
HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
OUT = os.path.join(HERE, 'fixtures', 'relationship_application.json')

# (service, kind, child/old, parent/new)
PAIRS = [
    ('my tags', 'siblings', 'prec:kid', 'prec:child'),
    ('downloader tags', 'siblings', 'prec:kid', 'prec:other child'),
    ('downloader tags', 'siblings', 'prec:child', 'prec:downloader ideal'),
    ('my tags', 'parents', 'prec:child', 'prec:own parent'),
    ('downloader tags', 'parents', 'prec:child', 'prec:other parent'),
    ('downloader tags', 'parents', 'prec:other child', 'prec:other child parent'),
    ('downloader tags', 'parents', 'prec:downloader ideal', 'prec:ideal parent'),
    ('downloader tags', 'parents', 'prec:other parent', 'prec:grandparent'),
]

TAGS = ['prec:kid', 'prec:child', 'prec:other child', 'prec:downloader ideal', 'prec:own parent', 'prec:other parent']

# (name, sibling queue, parent queue) for `my tags`; None leaves it as it is
CASES = [
    ('default', None, None),
    ('parents own then downloader', ['my tags'], ['my tags', 'downloader tags']),
    ('parents downloader then own', ['my tags'], ['downloader tags', 'my tags']),
    ('parents downloader only', ['my tags'], ['downloader tags']),
    ('parents none', ['my tags'], []),
    ('siblings downloader first', ['downloader tags', 'my tags'], ['my tags', 'downloader tags']),
    ('siblings own first', ['my tags', 'downloader tags'], ['downloader tags']),
    ('nothing applied', [], []),
]


def record(session):
    c = session.controller
    from hydrus.core import HydrusConstants as HC
    from hydrus.client import ClientConstants as CC
    from hydrus.client.metadata import ClientContentUpdates as U, ClientTags
    services = {s.GetName(): s.GetServiceKey() for s in c.services_manager.GetServices((HC.LOCAL_TAG,))}
    names = {k: n for n, k in services.items()}
    for name, kind, a, b in PAIRS:
        content_type = HC.CONTENT_TYPE_TAG_SIBLINGS if kind == 'siblings' else HC.CONTENT_TYPE_TAG_PARENTS
        c.WriteSynchronous('content_updates', U.ContentUpdatePackage.STATICCreateFromContentUpdates(
            services[name], [U.ContentUpdate(content_type, HC.CONTENT_UPDATE_ADD, (a, b))]))
    session.sync_tag_display()
    out = []
    for name, siblings, parents in CASES:
        def panel_value():
            from hydrus.client.gui.metadata import ClientGUITagDisplayMaintenanceEdit as A
            current = c.Read('tag_display_application')
            panel = A.EditTagDisplayApplication(c.gui, *current)
            page = next(p for p in panel._tag_services.GetPages() if p.GetServiceKey() == services['my tags'])
            if siblings is not None:
                page._sibling_service_keys_listbox.SetData([services[n] for n in siblings])
            if parents is not None:
                page._parent_service_keys_listbox.SetData([services[n] for n in parents])
            value = panel.GetValue()
            panel.deleteLater()
            return value
        sibling_map, parent_map = c.CallBlockingToQt(c.gui, panel_value)
        c.WriteSynchronous('tag_display_application', sibling_map, parent_map)
        session.sync_tag_display()
        lookup = c.Read('tag_siblings_and_parents_lookup', ClientTags.TAG_DISPLAY_DISPLAY_ACTUAL, TAGS)
        tags = {}
        for tag in TAGS:
            chain, ideal, descendants, ancestors = lookup[tag][services['my tags']]
            tags[tag] = {'ideal': ideal, 'chain': sorted(chain), 'parents': sorted(ancestors), 'children': sorted(descendants)}
        out.append({
            'case': name,
            'siblings': [names.get(k, k.hex()) for k in sibling_map[services['my tags']]],
            'parents': [names.get(k, k.hex()) for k in parent_map[services['my tags']]],
            'tags': tags,
        })
    with open(OUT, 'w') as f:
        json.dump({'pairs': PAIRS, 'cases': out}, f, indent=2, ensure_ascii=False)
        f.write('\n')


if __name__ == '__main__':
    import record_api, hydrus_driver
    db_dir = record_api.unpack_fixture('basic')
    try:
        hydrus_driver.run_client(db_dir, record)
    finally:
        shutil.rmtree(db_dir)

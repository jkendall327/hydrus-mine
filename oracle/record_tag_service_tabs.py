#!/usr/bin/env python3
"""Record the tag-service tabs of Manage tags and the siblings/parents dialogs.

On the `repositories` fixture (two local tag services and a tag repository),
with a few known mappings added:

- Manage tags (the real `ManageTagsPanel`) opened on one file, on two files
  and on a file the added mappings do not touch: its tab texts ("name (count)"), the tab that is
  current for each `default_tag_service_tab` (each service, and a key that is
  no tab), and what choosing another tab saves with
  `save_default_tag_service_tab_on_change` on and off. Tags staged on one
  tab stay staged on it while another tab is used, and the tab texts as the
  panel shows them after staging, and after a viewer moves on to another file
  (`CanvasHasNewMedia`, immediate commit as from the viewer).
- The real `ManageTagSiblings` and `ManageTagParents` dialogs: tab order and
  the same default-tab and save-on-change cases.
"""
import json
import os
import shutil
import sys
HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
from hydrus_driver import run_client
import record_api
OUT = os.path.join(HERE, 'fixtures', 'tag_service_tabs.json')

# (service name, tag, file indices)
MAPPINGS = [
    ('my tags', 'tabs:one', [0, 1]),
    ('my tags', 'tabs:two', [0]),
    ('my tags', 'tabs:three', [1]),
    ('downloader tags', 'tabs:down', [0]),
]


def record(session):
    c = session.controller
    from hydrus.core import HydrusConstants as HC
    from hydrus.client import ClientConstants as CC, ClientLocation
    from hydrus.client.metadata import ClientContentUpdates as U
    manifest = json.load(open(os.path.join(HERE, 'fixtures/legacy_db/repositories.manifest.json')))
    hashes = [bytes.fromhex(f['hash']) for f in manifest['files']]
    services = {s.GetName(): s.GetServiceKey() for s in c.services_manager.GetServices((HC.LOCAL_TAG, HC.TAG_REPOSITORY))}
    names = {k: n for n, k in services.items()}
    for name, tag, files in MAPPINGS:
        c.WriteSynchronous('content_updates', U.ContentUpdatePackage.STATICCreateFromContentUpdates(
            services[name], [U.ContentUpdate(HC.CONTENT_TYPE_MAPPINGS, HC.CONTENT_UPDATE_ADD, (tag, {hashes[i] for i in files}))]))
    # a file none of the mappings above touch
    untagged = 2

    def work():
        from qtpy import QtWidgets as QW
        from hydrus.client.media import ClientMediaSingle
        from hydrus.client.gui.metadata.ClientGUIManageTags import ManageTagsPanel
        from hydrus.client.gui.metadata import ClientGUIManageTagSiblings as S, ClientGUIManageTagParents as P
        from hydrus.client.gui import ClientGUIAsync

        class Job:
            def __init__(self, win, w, p, **kw): self.w, self.p = w, p
            def start(self): pass
        class Updater:
            def __init__(self, *args, **kwargs): pass
            def update(self): pass
        ClientGUIAsync.AsyncQtJob = Job
        ClientGUIAsync.AsyncQtUpdater = Updater

        def media(indices):
            results = {r.GetHash(): r for r in c.Read('media_results', [hashes[i] for i in indices])}
            return [ClientMediaSingle.MediaSingle(results[hashes[i]]) for i in indices]

        def settle():
            for _ in range(5):
                QW.QApplication.processEvents()

        def tabs(notebook):
            return [notebook.tabText(i) for i in range(notebook.count())]

        def default_name():
            key = c.new_options.GetKey('default_tag_service_tab')
            return names.get(key, key.hex())

        out = {'untagged_file': untagged, 'mappings': MAPPINGS}
        defaults = [('my tags', services['my tags']), ('downloader tags', services['downloader tags']), ('a tag repository', services['a tag repository']), ('all known tags', CC.COMBINED_TAG_SERVICE_KEY)]

        def default_cases(make, notebook_of):
            cases = []
            for label, key in defaults:
                for save in (False, True):
                    c.new_options.SetBoolean('save_default_tag_service_tab_on_change', save)
                    c.new_options.SetKey('default_tag_service_tab', key)
                    panel = make()
                    notebook = notebook_of(panel)
                    settle()
                    opened = notebook.currentIndex()
                    after_open = default_name()
                    # choose the last tab, then the first
                    chosen = []
                    for index in (notebook.count() - 1, 0):
                        notebook.setCurrentIndex(index)
                        settle()
                        chosen.append({'index': index, 'current': notebook.currentIndex(), 'default': default_name()})
                    cases.append({'default': label, 'save_on_change': save, 'opened': opened, 'default_after_open': after_open, 'chosen': chosen})
                    panel.deleteLater()
            return cases

        location = ClientLocation.LocationContext.STATICCreateSimple(CC.LOCAL_FILE_SERVICE_KEY)
        def manage(indices, immediate=False, canvas_key=None):
            return ManageTagsPanel(c.gui, location, CC.TAG_PRESENTATION_SEARCH_PAGE_MANAGE_TAGS, media(indices), immediate_commit=immediate, canvas_key=canvas_key)

        c.new_options.SetKey('default_tag_service_tab', services['my tags'])
        c.new_options.SetBoolean('save_default_tag_service_tab_on_change', False)
        opened = {}
        for label, indices in (('one', [0]), ('two', [0, 1]), ('untouched', [untagged])):
            panel = manage(indices)
            settle()
            opened[label] = {'files': indices, 'tabs': tabs(panel._tag_services)}
            panel.deleteLater()
        out['manage_tags_opened'] = opened
        out['manage_tags_defaults'] = default_cases(lambda: manage([0]), lambda p: p._tag_services)

        # staging on one tab survives using another
        c.new_options.SetKey('default_tag_service_tab', services['my tags'])
        panel = manage([0])
        settle()
        notebook = panel._tag_services
        pages = {names[notebook.widget(i).GetServiceKey()]: notebook.widget(i) for i in range(notebook.count())}
        def terms(page):
            return sorted(t.GetTag() for t in page._tags_box._ordered_terms)
        def staged(page):
            return sorted([[cu.GetAction(), cu.GetRow()[0]] for package in page._pending_content_update_packages for key, updates in package.IterateContentUpdates() for cu in updates])
        steps = []
        def state(step):
            steps.append({'step': step, 'current': notebook.currentIndex(), 'tabs': tabs(notebook), 'pages': {n: {'terms': terms(p), 'staged': staged(p), 'changed': p.HasChanges()} for n, p in pages.items()}})
        state('opened')
        pages['my tags'].AddTags({'tabs:staged mine'})
        state('added on my tags')
        notebook.setCurrentWidget(pages['downloader tags']); settle()
        pages['downloader tags'].AddTags({'tabs:staged down'})
        state('added on downloader tags')
        notebook.setCurrentWidget(pages['my tags']); settle()
        state('back on my tags')
        out['manage_tags_staging'] = steps
        panel.deleteLater()

        # from the viewer: immediate commit, then the viewer shows another file
        canvas_key = os.urandom(32)
        panel = manage([0], immediate=True, canvas_key=canvas_key)
        settle()
        viewer = {'opened': tabs(panel._tag_services)}
        panel.CanvasHasNewMedia(canvas_key, media([1])[0])
        settle()
        viewer['next file'] = tabs(panel._tag_services)
        panel.CanvasHasNewMedia(canvas_key, media([untagged])[0])
        settle()
        viewer['untouched file'] = tabs(panel._tag_services)
        out['manage_tags_viewer'] = viewer
        panel.deleteLater()

        for name, cls in (('siblings', S.ManageTagSiblings), ('parents', P.ManageTagParents)):
            c.new_options.SetKey('default_tag_service_tab', services['my tags'])
            c.new_options.SetBoolean('save_default_tag_service_tab_on_change', False)
            panel = cls(c.gui)
            settle()
            out[name + '_tabs'] = tabs(panel._tag_services)
            panel.deleteLater()
            out[name + '_defaults'] = default_cases(lambda: cls(c.gui), lambda p: p._tag_services)
        return out

    result = c.CallBlockingToQt(c.gui, work)
    with open(OUT, 'w') as f:
        json.dump(result, f, indent=2, ensure_ascii=False)
        f.write('\n')


if __name__ == '__main__':
    db_dir = record_api.unpack_fixture('repositories')
    try:
        run_client(db_dir, record)
    finally:
        shutil.rmtree(db_dir)

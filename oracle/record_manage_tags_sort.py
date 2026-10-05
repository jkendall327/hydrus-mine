#!/usr/bin/env python3
"""Real Qt Manage Tags sort defaults, independent controls and captured live sorts.

Synthetic mappings and a cross-namespace sibling are stored through actual content
updates. Real tag-list decorator work is published synchronously before snapshots;
all orderings come from the actual Manage Tags sorter and storage list. No network.
"""
import json, sys, tempfile
from pathlib import Path
HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))

def record(session):
    from qtpy import QtWidgets as QW
    from hydrus.client import ClientConstants as CC, ClientLocation
    from hydrus.client.gui.panels.options.TagSortPanel import TagSortPanel
    from hydrus.client.gui.metadata.ClientGUIManageTags import ManageTagsPanel
    from hydrus.client.gui.search import ClientGUIACDropdown as A
    from hydrus.client.media import ClientMediaSingle
    from hydrus.client.metadata import ClientContentUpdates as U, ClientTagSorting as S
    from hydrus.core import HydrusConstants as HC
    c = session.controller
    qt = lambda f: c.CallBlockingToQt(c.gui, f)
    codes = [CC.TAG_PRESENTATION_SEARCH_PAGE_MANAGE_TAGS, CC.TAG_PRESENTATION_MEDIA_VIEWER_MANAGE_TAGS]
    fields = ['_default_tag_sort_search_page_manage_tags', '_default_tag_sort_media_viewer_manage_tags']
    local = next(s.GetServiceKey() for s in c.services_manager.GetServices((HC.LOCAL_TAG,)) if s.GetName() == 'my tags')
    manifest = json.loads((HERE/'fixtures/legacy_db/basic.manifest.json').read_text())
    hashes = [bytes.fromhex(f['hash']) for f in manifest['files'][:3]]
    corpus = [('creator:parity zz alias', hashes), ('creator:parity 10 delta', hashes[:2]), ('character:parity 03 zebra', hashes[:1]), ('foo:parity 12 amber', hashes), ('foo:parity 2 beta', hashes[:2]), ('parity alpha', hashes[:1]), ('parity omega', hashes)]
    pair = ('creator:parity zz alias', 'character:parity 02 ideal')
    updates = [U.ContentUpdate(HC.CONTENT_TYPE_MAPPINGS, HC.CONTENT_UPDATE_ADD, (tag, set(files))) for tag, files in corpus]
    updates.append(U.ContentUpdate(HC.CONTENT_TYPE_TAG_SIBLINGS, HC.CONTENT_UPDATE_ADD, pair))
    c.WriteSynchronous('content_updates', U.ContentUpdatePackage.STATICCreateFromContentUpdates(local, updates))
    for _ in range(12):
        if not c.WriteSynchronous('sync_tag_display_maintenance', local, 0.5): break
    def controls():
        draft = c.new_options.Duplicate()
        values = lambda: [draft.GetDefaultTagSort(code).ToDictForAPI() for code in codes]
        initial = values()
        panel = TagSortPanel(c.gui, draft)
        panel.resize(780, 520); panel.show(); QW.QApplication.processEvents()
        panel.grab().save(str(HERE / "fixtures/manage_tags_sort_options.png"))
        labels = [w.text() for w in panel.findChildren(QW.QLabel) if 'manage tags dialogs' in w.text()]
        selected = [S.TagSort(S.SORT_BY_HUMAN_SUBTAG, CC.SORT_DESC, False, S.GROUP_BY_NAMESPACE_AZ), S.TagSort(S.SORT_BY_COUNT, CC.SORT_ASC, True, S.GROUP_BY_NOTHING)]
        for field, sort in zip(fields, selected): getattr(panel, field).SetValue(sort)
        before = values(); panel.deleteLater(); cancelled = values()
        panel = TagSortPanel(c.gui, draft)
        for field, sort in zip(fields, selected): getattr(panel, field).SetValue(sort)
        panel.UpdateOptions(); applied = values(); panel.deleteLater()
        panel = TagSortPanel(c.gui, draft)
        reopened = [getattr(panel, field).GetValue().ToDictForAPI() for field in fields]
        # Independent text/count choices survive switching types in one control.
        control = getattr(panel, fields[0])
        control._sort_type.SetValue(S.SORT_BY_HUMAN_TAG); control._UpdateControlsAfterSortTypeChanged()
        control._sort_order_text.SetValue(CC.SORT_DESC)
        control._sort_type.SetValue(S.SORT_BY_COUNT); control._UpdateControlsAfterSortTypeChanged()
        control._sort_order_count.SetValue(CC.SORT_ASC)
        count = control.GetValue().ToDictForAPI()
        control._sort_type.SetValue(S.SORT_BY_HUMAN_TAG); control._UpdateControlsAfterSortTypeChanged()
        text = control.GetValue().ToDictForAPI(); panel.deleteLater()
        return dict(initial=initial, labels=labels, before_apply=before, cancelled=cancelled, applied=applied, reopened=reopened, remembered_orders=[count,text])
    options = qt(controls)
    media = [ClientMediaSingle.MediaSingle(result) for result in c.Read('media_results', hashes)]
    old_thread, old_after = c.CallToThread, c.CallAfterQtSafe
    c.CallToThread = lambda func,*args,**kw: None if func == A.WriteFetch or hasattr(getattr(func,'__self__',None),'_win') else old_thread(func,*args,**kw)
    c.CallAfterQtSafe = lambda win,func,*args,**kw: func(*args,**kw) if getattr(func,'__name__','') in ('setCurrentWidget','SelectPage','connect') else old_after(win,func,*args,**kw)
    names = {tag for tag,_ in corpus}
    def make(code):
        c.new_options.SetKey('default_tag_service_tab', local)
        panel = ManageTagsPanel(c.gui, ClientLocation.LocationContext.STATICCreateSimple(CC.LOCAL_FILE_SERVICE_KEY), code, media if code == codes[0] else media[:1])
        pages = panel._tag_services
        pages.setCurrentWidget(next(pages.widget(i) for i in range(pages.count()) if pages.widget(i).GetServiceKey() == local))
        owner = pages.currentWidget(); box = owner._tags_box
        with box._async_text_info_lock: box._pending_async_text_info_terms.update(box._ordered_terms)
        updater = box._async_text_info_updater
        updater._publish_callable(updater._work_callable(updater._pre_work_callable()))
        owner._tags_box_sorter.EventSort()
        return panel, owner
    def snapshot(owner):
        control = owner._tags_box_sorter._tag_sort
        return dict(sort=control.GetValue().ToDictForAPI(), list_sort=owner._tags_box._tag_sort.ToDictForAPI(), rows=[term.GetTag() for term in owner._tags_box._ordered_terms if term.GetTag() in names], best_tags={term.GetTag():term.GetBestTag() for term in owner._tags_box._ordered_terms if term.GetTag() in names}, decorators=owner._tags_box._show_sibling_decorators, siblings_visible=not control._use_siblings.isHidden(), grouping_visible=not control._group_by.isHidden())
    cases=[]
    def consumers():
        for sort_type in [S.SORT_BY_HUMAN_TAG,S.SORT_BY_HUMAN_SUBTAG,S.SORT_BY_COUNT]:
            for ascending in [True,False]:
                for siblings in [True,False]:
                    for grouping in [S.GROUP_BY_NOTHING,S.GROUP_BY_NAMESPACE_AZ,S.GROUP_BY_NAMESPACE_USER]:
                        value = S.TagSort(sort_type,CC.SORT_ASC if ascending else CC.SORT_DESC,siblings,grouping)
                        for code in codes:
                            c.new_options.SetDefaultTagSort(code,value)
                            panel, owner = make(code)
                            cases.append(dict(context=code, **snapshot(owner)))
                            panel.deleteLater()
        original = S.TagSort(S.SORT_BY_HUMAN_TAG,CC.SORT_DESC,False,S.GROUP_BY_NOTHING)
        changed = S.TagSort(S.SORT_BY_COUNT,CC.SORT_DESC,True,S.GROUP_BY_NAMESPACE_USER)
        lifetime=[]
        for code in codes:
            c.new_options.SetDefaultTagSort(code,original)
            panel, owner = make(code); before=snapshot(owner)
            owner._tags_box_sorter._tag_sort.SetValue(S.TagSort(S.SORT_BY_HUMAN_SUBTAG,CC.SORT_ASC,True,S.GROUP_BY_NOTHING));owner._tags_box_sorter.EventSort()
            local_sort=snapshot(owner)
            c.new_options.SetDefaultTagSort(code,changed);owner._tags_box.NotifyNewOptions();after=snapshot(owner)
            successor, new_owner=make(code)
            lifetime.append(dict(context=code,before=before,local=local_sort,after_options=after,reopened=snapshot(new_owner)))
            panel.deleteLater();successor.deleteLater()
        return lifetime
    try: lifetime=qt(consumers)
    finally: c.CallToThread,c.CallAfterQtSafe=old_thread,old_after
    return dict(options=options,files=[h.hex() for h in hashes],corpus=[dict(tag=tag,hashes=[h.hex() for h in files]) for tag,files in corpus],siblings=[list(pair)],parents=[],tag_service=local.hex(),cases=cases,lifetime=lifetime)

def main():
    import hydrus_driver, record_api
    if len(sys.argv)>1:
        out=Path(sys.argv[2])
        value=hydrus_driver.run_client(record_api.unpack_fixture('basic'),record)
        out.write_text(json.dumps(value));return
    with tempfile.TemporaryDirectory() as directory:
        out=Path(directory)/'out.json'
        hydrus_driver.run_in_subprocess(str(Path(__file__).resolve()),'--child',str(out))
        value=json.loads(out.read_text())
    (HERE/'fixtures/manage_tags_sort.json').write_text(json.dumps(value,indent=2)+'\n')
    print('wrote manage_tags_sort.json')
if __name__=='__main__':main()

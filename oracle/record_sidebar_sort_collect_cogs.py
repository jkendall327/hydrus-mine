#!/usr/bin/env python3
"""Real search sidebar cog actions, media consumers, and default-collect drafts.

Uses an actual fixed-hash search Page and its own MediaSortControl/MediaCollectControl.
Actual QAction activations broadcast to its actual media results panel. Only cog
popup presentation is observed for mouse-route recording. No sort/collect business
handler, tag mapping, or media content is replaced or written.
"""
import json, sys, tempfile
from pathlib import Path
HERE=Path(__file__).resolve().parent
sys.path.insert(0,str(HERE))

def record(session):
    def drive():
        from qtpy import QtCore as QC,QtWidgets as QW,QtTest as QT
        from hydrus.core import HydrusConstants as HC, HydrusSerialisable
        from hydrus.client import ClientConstants as CC,ClientLocation
        from hydrus.client.gui.panels.options.FileSortCollectPanel import FileSortCollectPanel
        from hydrus.client.gui.widgets import ClientGUIMenuButton
        from hydrus.client.media import ClientMediaSort,ClientMediaCollect
        from hydrus.client.search import ClientSearchTagContext
        from record_main_menu import tree
        from record_media_collect import media_facts,sort_facts
        c=session.controller;gui=c.gui;original=c.new_options.Duplicate()
        page=None;panels=[]
        def settle():QT.QTest.qWait(50);QW.QApplication.processEvents()
        def context(tc):return dict(service=tc.service_key.hex(),display_service=tc.display_service_key.hex(),include_current=tc.include_current_tags,include_pending=tc.include_pending_tags)
        def sf(sort):f=sort_facts(sort);f['tag_context']=context(sort.tag_context);return f
        def cf(collect):return dict(namespaces=collect.namespaces,ratings=[k.hex() for k in collect.rating_service_keys],unmatched=collect.collect_unmatched,tag_context=context(collect.tag_context))
        def menu(control,sort=False):
            result=QW.QMenu(control)
            button=control._tag_gubbins_cog_icon_button if sort else control._cog_icon_button
            ClientGUIMenuButton.PopulateMenuFromTemplateItems(result,button._menu_template_items)
            return result
        def choose(control,group,label,sort=False):
            result=menu(control,sort)
            group_action=next(a for a in result.actions() if a.text()==group)
            next(a for a in group_action.menu().actions() if a.text()==label).trigger()
            result.deleteLater();settle()
        try:
            c.new_options.SetBoolean('save_page_sort_on_change',False)
            fallback=ClientMediaSort.MediaSort(('system',CC.SORT_FILES_BY_HASH),CC.SORT_ASC)
            c.new_options.SetFallbackSort(fallback)
            manifest=json.loads((HERE/'fixtures/legacy_db/basic.manifest.json').read_text())
            results=c.Read('media_results',[bytes.fromhex(f['hash']) for f in manifest['files']])
            results=[m for m in results if CC.LOCAL_FILE_SERVICE_KEY in m.GetLocationsManager().GetCurrent()]
            hashes=[m.GetHash() for m in results]
            location=ClientLocation.LocationContext.STATICCreateSimple(CC.LOCAL_FILE_SERVICE_KEY)
            page=gui.NewPageQuery(location,initial_hashes=hashes,page_name='synthetic sort and collect cogs',select_page=True)
            for _ in range(40):
                settle()
                if len(page.GetMediaResultsPanel().GetFlatMedia())==len(hashes):break
            panel=page.GetMediaResultsPanel()
            assert len(panel.GetFlatMedia())==len(hashes)
            sidebar=page.GetSidebar();sort=sidebar._media_sort_widget;collect=sidebar._media_collect_widget
            services=c.services_manager.GetServices([HC.LOCAL_TAG,HC.TAG_REPOSITORY,HC.COMBINED_TAG])
            search_context=context(sidebar._tag_autocomplete.GetFileSearchContext().GetTagContext())
            locked=sidebar._page_manager.GetVariable('system_hash_locked')
            independent=ClientSearchTagContext.TagContext(include_current_tags=False,include_pending_tags=False)
            states=[];cases=[]
            for kind,sort_type in [('filesize',('system',CC.SORT_FILES_BY_FILESIZE)),('num_tags',('system',CC.SORT_FILES_BY_NUM_TAGS)),('namespaces',('namespaces',(('series','creator','title','volume','chapter','page'),1)))]:
                sort.SetSort(ClientMediaSort.MediaSort(sort_type,CC.SORT_ASC,tag_context=independent.Duplicate()))
                m=menu(sort,True);states.append(dict(kind=kind,visible=not sort._tag_gubbins_cog_icon_button.isHidden(),menu=tree(m)));m.deleteLater()
                if kind=='filesize':continue
                for service in services:
                    for view in ([1,3,2] if kind=='namespaces' else [1]):
                        collect.SetCollect(ClientMediaCollect.MediaCollect())
                        sort.SetSort(ClientMediaSort.MediaSort(sort_type,CC.SORT_ASC,tag_context=independent.Duplicate()))
                        choose(sort,'tag service',service.GetName(),True)
                        if kind=='namespaces':choose(sort,'ADVANCED: tag display type',{1:'display tags',3:'multiple media view tags',2:'single media view tags'}[view],True)
                        m=menu(sort,True)
                        cases.append(dict(kind=kind,service=service.GetName(),sort=sf(sort.GetSort()),menu=tree(m),media=[media_facts(m) for m in panel._sorted_media]));m.deleteLater()
            collects=[]
            for service in services:
                for unmatched in (True,False):
                    collect.SetCollect(ClientMediaCollect.MediaCollect(namespaces=['series'],collect_unmatched=not unmatched,tag_context=independent.Duplicate()))
                    choose(collect,'tag service',service.GetName())
                    choose(collect,'unmatched files','collect into one group' if unmatched else 'leave separate')
                    m=menu(collect)
                    collects.append(dict(service=service.GetName(),sort=sf(sort.GetSort()),collect=cf(collect.GetValue()),menu=tree(m),media=[media_facts(m) for m in panel._sorted_media]));m.deleteLater()
            # The actual QPushButton accepts left activation; right clicks do not open.
            from hydrus.client.gui import ClientGUICore as CGC
            core=CGC.core();old_popup=core.PopupMenu;mouse=[]
            try:
                opened=[]
                def observe(owner,m):
                    opened.append(tree(m))
                    m.ensurePolished();m.adjustSize()
                    m.grab().save(str(HERE/f'fixtures/sidebar_{role}_cog.png'))
                    for i,a in enumerate(m.actions()):
                        if a.menu() is not None:
                            child=a.menu();child.ensurePolished();child.adjustSize()
                            child.grab().save(str(HERE/f'fixtures/sidebar_{role}_cog_{i}.png'))
                core.PopupMenu=observe
                for role,control,which in [('sort',sort,True),('collect',collect,False)]:
                    button=control._tag_gubbins_cog_icon_button if which else control._cog_icon_button
                    for name,mouse_button in [('right',QC.Qt.MouseButton.RightButton),('left',QC.Qt.MouseButton.LeftButton)]:
                        opened.clear();QT.QTest.mouseClick(button,mouse_button);settle()
                        mouse.append(dict(role=role,button=name,menus=list(opened)))
            finally:core.PopupMenu=old_popup
            options=[]
            for service in (services[0],services[-1]):
                c.new_options.SetDefaultCollect(ClientMediaCollect.MediaCollect(namespaces=['series'],collect_unmatched=False,tag_context=independent.Duplicate()))
                p=FileSortCollectPanel(gui,c.new_options);panels.append(p)
                control=p._default_media_collect
                before=cf(c.new_options.GetDefaultCollect())
                choose(control,'tag service',service.GetName())
                m=menu(control);checked=tree(m);m.deleteLater()
                staged=cf(c.new_options.GetDefaultCollect());draft=cf(control.GetValue())
                p.UpdateOptions()
                encoded=HydrusSerialisable.CreateFromSerialisableTuple(c.new_options.GetSerialisableTuple())
                reopened=FileSortCollectPanel(gui,c.new_options);panels.append(reopened)
                options.append(dict(service=service.GetName(),before=before,staged=staged,draft=draft,menu=checked,saved=cf(c.new_options.GetDefaultCollect()),round_trip=cf(encoded.GetDefaultCollect()),reopened=cf(reopened._default_media_collect.GetValue())))
            p=FileSortCollectPanel(gui,c.new_options);panels.append(p)
            before_cancel=cf(c.new_options.GetDefaultCollect())
            choose(p._default_media_collect,'tag service',services[0].GetName())
            cancel_draft=cf(p._default_media_collect.GetValue());panels.remove(p);p.deleteLater();settle()
            after_cancel=cf(c.new_options.GetDefaultCollect())
            gui.grab().save(str(HERE/'fixtures/sidebar_sort_collect_cogs.png'))
            return dict(search_context=search_context,locked=locked,services=[dict(name=s.GetName(),key=s.GetServiceKey().hex(),type=s.GetServiceType()) for s in services],files=[h.hex() for h in hashes],fallback=sf(fallback),states=states,sorts=cases,collects=collects,mouse=mouse,options=options,cancel_before=before_cancel,cancel_draft=cancel_draft,cancel_after=after_cancel)
        finally:
            for p in panels:p.deleteLater()
            c.new_options=original
    return session.controller.CallBlockingToQt(session.controller.gui,drive)

def main():
    import hydrus_driver,record_api
    if len(sys.argv)>1 and sys.argv[1]=='--child':
        dest=Path(sys.argv[2]);dest.write_text(json.dumps(hydrus_driver.run_client(record_api.unpack_fixture('basic'),record)));return
    with tempfile.TemporaryDirectory() as d:
        dest=Path(d)/'out.json';hydrus_driver.run_in_subprocess(str(Path(__file__).resolve()),'--child',str(dest));result=json.loads(dest.read_text())
    (HERE/'fixtures/sidebar_sort_collect_cogs.json').write_text(json.dumps(result,indent=2)+'\n')
if __name__=='__main__':main()

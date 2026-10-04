#!/usr/bin/env python3
"""Record real opening-default display modes, filtered/raw rows and captured defaults.

Actual TagPresentationPanel choices stage until UpdateOptions; actual query-page
sidebar and media-viewer hover lists capture their opening mode. A synthetic alias,
parent, separate view filters and namespace/underscore presentation distinguish all
four choices. Storage decoration workers publish synchronously through real code.
"""
import json,os,sys,tempfile,time
HERE=os.path.dirname(os.path.abspath(__file__));sys.path.insert(0,HERE)
OUT=os.path.join(HERE,'fixtures','tag_list_display_types.json')
FIELDS={'sidebar':('tag_list_tag_display_type_sidebar','_tag_list_tag_display_type_sidebar'),'viewer':('tag_list_tag_display_type_media_viewer_hover','_tag_list_tag_display_type_media_viewer_hover')}
def record(session):
    from hydrus.client import ClientConstants as CC,ClientLocation
    from hydrus.client.gui.panels.options.TagPresentationPanel import TagPresentationPanel
    from hydrus.client.gui.lists.ClientGUIListBoxes import ListBoxTagsMediaHoverFrame
    from hydrus.client.metadata import ClientContentUpdates as U
    from hydrus.core import HydrusConstants as HC,HydrusTags
    c=session.controller;qt=lambda f:c.CallBlockingToQt(c.gui,f)
    local=next(s.GetServiceKey() for s in c.services_manager.GetServices((HC.LOCAL_TAG,)) if s.GetName()=='my tags')
    manifest=json.load(open(os.path.join(HERE,'fixtures','legacy_db','basic.manifest.json')))
    hashes=[bytes.fromhex(f['hash']) for f in manifest['files'][:3]]
    corpus=[('parity:mode old_tag',hashes[:2]),('parity:mode ideal_tag',hashes[:1]),('parity:mode selection_hidden',hashes[:2]),('parity:mode single_hidden',hashes[:2])]
    siblings=[['parity:mode old_tag','parity:mode ideal_tag']];parents=[['parity:mode ideal_tag','category:mode parent_tag']]
    updates=[U.ContentUpdate(HC.CONTENT_TYPE_MAPPINGS,HC.CONTENT_UPDATE_ADD,(tag,set(files))) for tag,files in corpus]
    for kind,pairs in [(HC.CONTENT_TYPE_TAG_SIBLINGS,siblings),(HC.CONTENT_TYPE_TAG_PARENTS,parents)]:
        updates.extend(U.ContentUpdate(kind,HC.CONTENT_UPDATE_ADD,tuple(pair)) for pair in pairs)
    c.WriteSynchronous('content_updates',U.ContentUpdatePackage.STATICCreateFromContentUpdates(local,updates))
    for _ in range(12):
        if not c.WriteSynchronous('sync_tag_display_maintenance',local,0.5):break
    def controls():
        draft=c.new_options.Duplicate();panel=TagPresentationPanel(c.gui,draft)
        shown={name:getattr(panel,field).GetValue() for name,(_,field) in FIELDS.items()}
        choices={name:[(getattr(panel,field).itemText(i),getattr(panel,field).itemData(i)) for i in range(getattr(panel,field).count())] for name,(_,field) in FIELDS.items()}
        for _,(_,field) in FIELDS.items():getattr(panel,field).SetValue(0)
        before={name:draft.GetInteger(key) for name,(key,_) in FIELDS.items()};panel.deleteLater()
        cancelled={name:draft.GetInteger(key) for name,(key,_) in FIELDS.items()}
        panel=TagPresentationPanel(c.gui,draft)
        for _,(_,field) in FIELDS.items():getattr(panel,field).SetValue(0)
        panel.UpdateOptions();applied={name:draft.GetInteger(key) for name,(key,_) in FIELDS.items()};panel.deleteLater()
        return dict(shown=shown,choices=choices,before_apply=before,cancelled=cancelled,applied=applied)
    options=qt(controls)
    c.new_options.SetBoolean('show_namespaces',False);c.new_options.SetBoolean('replace_tag_underscores_with_spaces',True)
    c.new_options.SetBoolean('show_parent_decorators_on_storage_taglists',True);c.new_options.SetBoolean('show_sibling_decorators_on_storage_taglists',True)
    filters={'selection':'parity:mode selection_hidden','single':'parity:mode single_hidden'}
    for code,key in [(3,'selection'),(2,'single')]:
        filter=HydrusTags.TagFilter();filter.SetRule(filters[key],HC.FILTER_BLACKLIST);c.tag_display_manager.SetTagFilter(code,local,filter)
    media=c.Read('media_results',hashes)
    for result in media:result.GetTagsManager().NewTagDisplayRules()
    location=ClientLocation.LocationContext.STATICCreateSimple(CC.LOCAL_FILE_SERVICE_KEY)
    old_thread=c.CallToThread
    c.CallToThread=lambda func,*args,**kw:None if getattr(getattr(func,'__self__',None),'_name',None)=='tags list box text info updater' else old_thread(func,*args,**kw)
    def rows(box):
        if box._has_async_text_info:
            with box._async_text_info_lock:box._pending_async_text_info_terms.update(box._ordered_terms)
            worker=box._async_text_info_updater;worker._publish_callable(worker._work_callable(worker._pre_work_callable()))
        out=[]
        for term in box._ordered_terms:
            if term.GetTag().startswith(('parity:mode','category:mode')):
                out.append({'tag':term.GetTag(),'rows':[''.join(text for text,_colour in row) for row in box._GetRowsOfTextsAndColours(term)]})
        return out
    cases=[]
    try:
        for expanded in [False,True]:
            c.new_options.SetBoolean('expand_parents_on_storage_taglists',expanded)
            for code in [3,2,1,0]:
                for key,_ in FIELDS.values():c.new_options.SetInteger(key,code)
                page=qt(lambda:c.gui._notebook.NewPageQuery(location,initial_hashes=hashes,do_sort=False))
                for _ in range(300):
                    panel=qt(page.GetMediaResultsPanel)
                    if hasattr(panel,'_sorted_media') and len(panel._sorted_media)==3:break
                    time.sleep(.05)
                else:raise RuntimeError('real query page did not load: '+repr(qt(lambda:(type(panel).__name__,len(panel._sorted_media) if hasattr(panel,'_sorted_media') else None))))
                sidebar=qt(lambda:page.GetSidebar()._current_selection_tags_list)
                hover=qt(lambda:ListBoxTagsMediaHoverFrame(c.gui,b'parity-mode-viewer',location))
                qt(lambda:sidebar.SetTagsByMediaResults(media));qt(lambda:hover.SetTagsByMediaResults(media[:1]))
                before=qt(lambda:dict(sidebar=rows(sidebar),viewer=rows(hover)))
                for key,_ in FIELDS.values():c.new_options.SetInteger(key,(code+1)%4)
                qt(lambda:sidebar.ForceTagRecalc());qt(lambda:hover.ForceTagRecalc())
                after=qt(lambda:dict(sidebar=rows(sidebar),viewer=rows(hover)))
                cases.append(dict(mode=code,expanded=expanded,sidebar_mode=sidebar._tag_display_type,viewer_mode=hover._tag_display_type,rows=before,after_default_change=after))
                qt(hover.deleteLater)
    finally:c.CallToThread=old_thread
    return dict(options=options,files=[h.hex() for h in hashes],corpus=[dict(tag=t,hashes=[h.hex() for h in fs]) for t,fs in corpus],siblings=siblings,parents=parents,filters=filters,cases=cases)
def child(out):
    import hydrus_driver,record_api
    result=hydrus_driver.run_client(record_api.unpack_fixture('basic'),record)
    with open(out,'w') as f:json.dump(result,f)
def main():
    if len(sys.argv)>1 and sys.argv[1]=='--child':child(sys.argv[2]);return
    import hydrus_driver
    with tempfile.TemporaryDirectory() as d:
        path=os.path.join(d,'out.json');hydrus_driver.run_in_subprocess(os.path.abspath(__file__),'--child',path)
        with open(path) as f:result=json.load(f)
    with open(OUT,'w') as f:json.dump(result,f,indent=2,ensure_ascii=False);f.write('\n')
    print('wrote',OUT)
if __name__=='__main__':main()

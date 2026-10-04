#!/usr/bin/env python3
"""Drive Tag Editing defaults and actual Manage Tags service layouts/tag rows.

Synthetic raw mappings, sibling and transitive parent relationships use the real
DB. The actual storage-list decorator worker/publisher is run synchronously to
capture real Qt rows without worker timing changing the snapshot. Options-page
staging/cancel and UpdateOptions are recorded separately from runtime consumers.
Inherited-row activation invokes the real enter callback and recorded add action,
then captures the updated current counts and inherited storage decorations.
"""
import itertools, json, os, sys, tempfile, time
HERE=os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0,HERE)
OUT=os.path.join(HERE,'fixtures','tag_dialog_preferences.json')
FIELDS={
 'listbook':('use_listbook_for_tag_service_panels','_use_listbook_for_tag_service_panels'),
 'parents':('show_parent_decorators_on_storage_taglists','_show_parent_decorators_on_storage_taglists'),
 'expanded':('expand_parents_on_storage_taglists','_expand_parents_on_storage_taglists'),
 'siblings':('show_sibling_decorators_on_storage_taglists','_show_sibling_decorators_on_storage_taglists'),
}
def record(session):
    from hydrus.client import ClientConstants as CC,ClientLocation
    from hydrus.client.gui import ClientGUIDialogsQuick
    from hydrus.client.gui.panels.options.TagEditingPanel import TagEditingPanel
    from hydrus.client.gui.metadata.ClientGUIManageTags import ManageTagsPanel
    from hydrus.client.gui.metadata.ClientGUIManageTagSiblings import ManageTagSiblings
    from hydrus.client.gui.metadata.ClientGUIManageTagParents import ManageTagParents
    from hydrus.client.gui.search import ClientGUIACDropdown as A
    from hydrus.client.media import ClientMediaSingle
    from hydrus.client.metadata import ClientContentUpdates as U
    from hydrus.core import HydrusConstants as HC
    c=session.controller;qt=lambda f:c.CallBlockingToQt(c.gui,f)
    local=next(s.GetServiceKey() for s in c.services_manager.GetServices((HC.LOCAL_TAG,)) if s.GetName()=='my tags')
    manifest=json.load(open(os.path.join(HERE,'fixtures','legacy_db','basic.manifest.json')))
    hashes=[bytes.fromhex(f['hash']) for f in manifest['files'][:3]]
    corpus=[('parity:amber',hashes[:2]),('parity:amber old',hashes[1:2])]
    updates=[U.ContentUpdate(HC.CONTENT_TYPE_MAPPINGS,HC.CONTENT_UPDATE_ADD,(tag,set(files))) for tag,files in corpus]
    updates.extend([U.ContentUpdate(HC.CONTENT_TYPE_TAG_SIBLINGS,HC.CONTENT_UPDATE_ADD,('parity:amber old','parity:amber')),U.ContentUpdate(HC.CONTENT_TYPE_TAG_PARENTS,HC.CONTENT_UPDATE_ADD,('parity:amber','category:colour')),U.ContentUpdate(HC.CONTENT_TYPE_TAG_PARENTS,HC.CONTENT_UPDATE_ADD,('category:colour','series:root'))])
    c.WriteSynchronous('content_updates',U.ContentUpdatePackage.STATICCreateFromContentUpdates(local,updates))
    for _ in range(12):
        if not c.WriteSynchronous('sync_tag_display_maintenance',local,0.5):break
    def values(options):return {name:options.GetBoolean(key) for name,(key,_) in FIELDS.items()}
    def staged_options():
        original=values(c.new_options);draft=c.new_options.Duplicate();panel=TagEditingPanel(c.gui,draft)
        controls={name:getattr(panel,field).isChecked() for name,(_,field) in FIELDS.items()}
        for _,(_,field) in FIELDS.items():getattr(panel,field).setChecked(not getattr(panel,field).isChecked())
        before=values(draft);panel.deleteLater();cancelled=values(draft)
        panel=TagEditingPanel(c.gui,draft)
        for name,(_,field) in FIELDS.items():getattr(panel,field).setChecked(not original[name])
        panel.UpdateOptions();applied=values(draft);panel.deleteLater()
        return {'controls':controls,'before_apply':before,'cancelled':cancelled,'applied':applied,'global_after':values(c.new_options)}
    options=qt(staged_options)
    media=[ClientMediaSingle.MediaSingle(result) for result in c.Read('media_results',hashes)]
    old_thread=c.CallToThread;old_after=c.CallAfterQtSafe
    # Let notebook initial service selection run synchronously, avoiding a timer
    # race; only real functions that perform selection/connection are forwarded.
    c.CallAfterQtSafe=lambda win,func,*args,**kw:func(*args,**kw) if getattr(func,'__name__','') in ('setCurrentWidget','SelectPage','connect') else old_after(win,func,*args,**kw)
    c.CallToThread=lambda func,*args,**kw:None if func==A.WriteFetch or hasattr(getattr(func,'__self__',None),'_win') else old_thread(func,*args,**kw)
    cases=[]
    def snapshot(prefs):
        for name,(key,_) in FIELDS.items():c.new_options.SetBoolean(key,prefs[name])
        c.new_options.SetKey('default_tag_service_tab',local)
        panel=ManageTagsPanel(c.gui,ClientLocation.LocationContext.STATICCreateSimple(CC.LOCAL_FILE_SERVICE_KEY),CC.TAG_PRESENTATION_SEARCH_PAGE_MANAGE_TAGS,media)
        pages=panel._tag_services
        pages.setCurrentWidget(next(pages.widget(i) for i in range(pages.count()) if pages.widget(i).GetServiceKey()==local))
        box=pages.currentWidget()._tags_box
        with box._async_text_info_lock:box._pending_async_text_info_terms.update(box._ordered_terms)
        updater=box._async_text_info_updater
        updater._publish_callable(updater._work_callable(updater._pre_work_callable()))
        rows=[]
        for term in box._ordered_terms:
            if not term.GetTag().startswith('parity:'):continue
            rows.append({'tag':term.GetTag(),'rows':[''.join(text for text,_colour in row) for row in box._GetRowsOfTextsAndColours(term)]})
        topology={'manage':pages.__class__.__name__}
        for name,cls in [('siblings',ManageTagSiblings),('parents',ManageTagParents)]:
            other=cls(c.gui,[]);topology[name]=other._tag_services.__class__.__name__;other.deleteLater()
        result={'preferences':prefs,'topology':topology,'services':[c.services_manager.GetName(pages.widget(i).GetServiceKey()) for i in range(pages.count())],'rows':rows}
        if prefs['parents'] and prefs['expanded']:
            term=next(term for term in box._ordered_terms if term.GetTag()=='parity:amber old')
            physical=box._terms_to_positional_indices[term]+1
            logical,_=box._GetLogicalIndicesFromPositionalIndex(physical)
            box._selected_terms.clear();box._Select(logical)
            entered=[];asked=[];old_enter=box._enter_func;old_select=ClientGUIDialogsQuick.SelectFromListButtons
            def enter(tags):
                entered.extend(sorted(tags))
                old_enter(tags)
            def select(parent,title,choices,**kwargs):
                chosen=next(choice for choice in choices if choice[1][0]==HC.CONTENT_UPDATE_ADD)
                asked.append({'title':title,'message':kwargs.get('message'),'choices':[choice[0] for choice in choices],'chosen':chosen[0]})
                return chosen[1]
            try:
                box._enter_func=enter
                ClientGUIDialogsQuick.SelectFromListButtons=select
                activated=box._Activate(False,False)
                with box._async_text_info_lock:box._pending_async_text_info_terms.update(box._ordered_terms)
                updater._publish_callable(updater._work_callable(updater._pre_work_callable()))
                after_rows=[{'tag':term.GetTag(),'rows':[''.join(text for text,_colour in row) for row in box._GetRowsOfTextsAndColours(term)]} for term in box._ordered_terms if term.GetTag().startswith('parity:')]
            finally:
                box._enter_func=old_enter
                ClientGUIDialogsQuick.SelectFromListButtons=old_select
            result['parent_activation']={'physical_row':physical,'logical_tag':term.GetTag(),'passed_tags':entered,'activated':activated,'asked':asked,'rows_after_activation':after_rows}
        panel.deleteLater();return result
    try:
        for bits in itertools.product((False,True),repeat=4):
            cases.append(qt(lambda bits=bits:snapshot(dict(zip(FIELDS,bits)))))
    finally:c.CallToThread=old_thread;c.CallAfterQtSafe=old_after
    return {'options':options,'files':[h.hex() for h in hashes],'corpus':[{'tag':tag,'hashes':[h.hex() for h in files]} for tag,files in corpus],'siblings':[['parity:amber old','parity:amber']],'parents':[['parity:amber','category:colour'],['category:colour','series:root']],'cases':cases}
def child(out):
    import hydrus_driver,record_api
    result=hydrus_driver.run_client(record_api.unpack_fixture('basic'),record)
    with open(out,'w') as f:json.dump(result,f)
def main():
    if len(sys.argv)>1 and sys.argv[1]=='--child':child(sys.argv[2]);return
    import hydrus_driver
    with tempfile.TemporaryDirectory() as directory:
        path=os.path.join(directory,'out.json');hydrus_driver.run_in_subprocess(os.path.abspath(__file__),'--child',path)
        with open(path) as f:result=json.load(f)
    with open(OUT,'w') as f:json.dump(result,f,indent=2,ensure_ascii=False);f.write('\n')
    print('wrote',OUT)
if __name__=='__main__':main()

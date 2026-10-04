#!/usr/bin/env python3
"""Record real write-autocomplete logical multi-selection and batch activation.

Actual WriteFetch populates the real Qt predicate list. Plain/Ctrl/Shift and
Ctrl+Shift hits drive Qt's existing reversible selection algorithm; inherited
physical rows map to the originating logical tag. Activation calls the real
list's handler and write-dropdown broadcast, including clearing its input. The
real multi-tag menu records copy payloads and AND/OR/each-page/duplicate launches.
"""
import json,os,sys,tempfile
HERE=os.path.dirname(os.path.abspath(__file__));sys.path.insert(0,HERE)
OUT=os.path.join(HERE,'fixtures','write_tag_selection.json')
def record(session):
    from hydrus.client import ClientConstants as CC,ClientLocation,ClientThreading
    from hydrus.client.gui.search import ClientGUIACDropdown as A
    from hydrus.client.metadata import ClientContentUpdates as U
    from hydrus.client.search import ClientSearchAutocomplete as SA,ClientSearchFileSearchContext as SC
    from hydrus.core import HydrusConstants as HC
    c=session.controller;qt=lambda f:c.CallBlockingToQt(c.gui,f)
    service=next(s.GetServiceKey() for s in c.services_manager.GetServices((HC.LOCAL_TAG,)) if s.GetName()=='my tags')
    manifest=json.load(open(os.path.join(HERE,'fixtures','legacy_db','basic.manifest.json')))
    hashes=[bytes.fromhex(f['hash']) for f in manifest['files'][:4]]
    corpus=[('parity:multi alpha',hashes[:3]),('parity:multi beta',hashes[1:2]),('parity:multi gamma',hashes[2:3]),('parity:multi alpha old',hashes[:1])]
    siblings=[['parity:multi alpha old','parity:multi alpha']];parents=[['parity:multi alpha','category:multi parent']]
    updates=[U.ContentUpdate(HC.CONTENT_TYPE_MAPPINGS,HC.CONTENT_UPDATE_ADD,(tag,set(files))) for tag,files in corpus]
    for kind,pairs in [(HC.CONTENT_TYPE_TAG_SIBLINGS,siblings),(HC.CONTENT_TYPE_TAG_PARENTS,parents)]:updates.extend(U.ContentUpdate(kind,HC.CONTENT_UPDATE_ADD,tuple(pair)) for pair in pairs)
    c.WriteSynchronous('content_updates',U.ContentUpdatePackage.STATICCreateFromContentUpdates(service,updates))
    for _ in range(12):
        if not c.WriteSynchronous('sync_tag_display_maintenance',service,.5):break
    location=ClientLocation.LocationContext.STATICCreateSimple(CC.LOCAL_FILE_SERVICE_KEY)
    old_thread=c.CallToThread;c.CallToThread=lambda func,*args,**kw:None if func==A.WriteFetch else old_thread(func,*args,**kw)
    entered=[]
    ac=qt(lambda:A.AutoCompleteDropdownTagsWrite(c.gui,lambda tags:entered.append(sorted(tags)),location,service,show_paste_button=True))
    def prepare():
        c.new_options.SetBoolean('ac_select_first_with_count',False)
        ac._search_results_list.SetExtraParentRowsAllowed(True);ac._search_results_list.SetParentDecoratorsAllowed(True)
        ac._text_ctrl.setText('parity:multi');ac.CancelCurrentResultsFetchJob()
        return ac._GetParsedAutocompleteText(),SC.FileSearchContext(location_context=ac._location_context_button.GetValue(),tag_context=ac._tag_context_button.GetValue())
    parsed,context=qt(prepare);captured={}
    def prefetched(*args):pass
    def results(job,parsed,cache,matches):captured['matches']=matches
    old_after=c.CallAfterQtSafe;c.CallAfterQtSafe=lambda target,func,*args,**kw:func(*args,**kw) if target==ac and func in (prefetched,results) else old_after(target,func,*args,**kw)
    try:A.WriteFetch(ac,ClientThreading.JobStatus(),prefetched,results,parsed,context,SA.PredicateResultsCacheInit())
    finally:c.CallAfterQtSafe=old_after
    def replay():
        box=ac._search_results_list;box.SetPredicates([]);box.SetPredicates(captured['matches'])
        tags=[term.GetPredicate().GetValue() for term in box._ordered_terms]
        rows=[]
        for term in box._ordered_terms:
            for text in box._GetRowsOfTextsAndColours(term):rows.append(dict(tag=term.GetPredicate().GetValue(),text=''.join(t for t,_ in text)))
        def snapshot(action,**extra):
            return dict(action=action,selected=[tag for tag,term in zip(tags,box._ordered_terms) if term in box._selected_terms],last_hit=box._last_hit_logical_index,**extra)
        steps=[snapshot('initial')]
        for tag,ctrl,shift in [('parity:multi',False,False),('parity:multi alpha',True,False),('parity:multi beta',True,False),('parity:multi alpha',False,False),('parity:multi alpha',True,False),('parity:multi gamma',False,True),('parity:multi beta',False,True),('parity:multi',True,True),('parity:multi gamma',False,False),('parity:multi beta',True,False)]:
            logical=tags.index(tag);box._Hit(shift,ctrl,logical);steps.append(snapshot('click',tag=tag,ctrl=ctrl,shift=shift))
        alpha=box._ordered_terms[tags.index('parity:multi alpha')]
        physical=box._terms_to_positional_indices[alpha]+1;logical,_=box._GetLogicalIndicesFromPositionalIndex(physical)
        box._Hit(False,True,logical);steps.append(snapshot('parent_click',tag='parity:multi alpha',physical=physical,ctrl=True,shift=False))
        from qtpy import QtCore
        from hydrus.client.gui import ClientGUICore as CGC,ClientGUIAsync
        captured_menu={};copied=[];launches=[]
        old_popup=CGC.core().PopupMenu;old_pub=c.pub;old_start=ClientGUIAsync.AsyncQtJob.start
        CGC.core().PopupMenu=lambda win,menu:captured_menu.update(menu=menu)
        ClientGUIAsync.AsyncQtJob.start=lambda job:job._publish_callable(job._work_callable())
        def publish(topic,*args,**kw):
            if topic=='clipboard':copied.append(args[1])
            elif topic in ('new_page_query','new_page_duplicates'):
                launches.append(dict(topic=topic,current=[key.hex() for key in sorted(args[0].current_service_keys)],deleted=[key.hex() for key in sorted(args[0].deleted_service_keys)],predicates=[p.ToString() for p in kw['initial_predicates']]))
            else:old_pub(topic,*args,**kw)
        c.pub=publish
        def paths(menu,prefix=()):
            out=[]
            for action in menu.actions():
                if action.isSeparator():continue
                path=prefix+(action.text(),)
                out.extend(paths(action.menu(),path) if action.menu() is not None else [(path,action)])
            return out
        menus=[]
        try:
            box.ShowMenuFromSignal(QtCore.QPoint(0,0));actions=paths(captured_menu['menu'])
            menus.append(dict(action='open',paths=[list(path) for path,_ in actions]))
            for path,action in actions:
                if path[0]=='copy':
                    copied.clear();action.trigger();menus.append(dict(action='copy',label=path[-1],copied=list(copied)))
                elif path[0]=='open':
                    launches.clear();action.trigger();menus.append(dict(action='launch',label=path[-1],launched=list(launches)))
        finally:
            CGC.core().PopupMenu=old_popup;c.pub=old_pub;ClientGUIAsync.AsyncQtJob.start=old_start
        activated=box._Activate(False,False);steps.append(dict(action='activate',activated=activated,entered=list(entered),text=ac._text_ctrl.text()))
        return dict(tags=tags,rows=rows,steps=steps,menus=menus)
    try:out=qt(replay)
    finally:c.CallToThread=old_thread
    return dict(files=[h.hex() for h in hashes],corpus=[dict(tag=t,hashes=[h.hex() for h in fs]) for t,fs in corpus],siblings=siblings,parents=parents,**out)
def child(out):
    import hydrus_driver,record_api
    result=hydrus_driver.run_client(record_api.unpack_fixture('basic'),record)
    with open(out,'w') as f:json.dump(result,f)
def main():
    if len(sys.argv)>1 and sys.argv[1]=='--child':child(sys.argv[2]);return
    import hydrus_driver
    with tempfile.TemporaryDirectory() as d:
        p=os.path.join(d,'out.json');hydrus_driver.run_in_subprocess(os.path.abspath(__file__),'--child',p)
        with open(p) as f:result=json.load(f)
    with open(OUT,'w') as f:json.dump(result,f,indent=2,ensure_ascii=False);f.write('\n')
    print('wrote',OUT)
if __name__=='__main__':main()

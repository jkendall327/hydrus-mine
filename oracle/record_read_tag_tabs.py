#!/usr/bin/env python3
"""Record real read-autocomplete favourites/children and search broadcasts.

Synthetic local mappings and sibling/parent chains run through the actual DB.
Children AsyncQtJob work/publish and favourite decoration lookup run synchronously
only to make snapshots stable; their actual queries and UI consumers are used.
Tabs retain their unfiltered tag lists, typing switches back to search, and
chosen favourites/children update the real file-search context. The favourite
selection also records its actual file-query result count. Current/pending
flags, zero-count descendants, a finite/zero/unlimited cap, service changes and
predicate removals are captured. GetPredicates returns a set; only that set is
sorted for stable evidence. Result row order is preserved. The typing snapshot
records the pending fetch/tab switch, not final query results. No system-predicate
editor is replaced. Stored system-looking/wildcard favourites also drive the
real children context, exclusion/removal, and restored negative-tag contexts;
a separate cleaned-spelling chain records the DB lookup
cleaning boundary, which differs from literal predicate activation.
"""
import json, os, sys, tempfile
HERE=os.path.dirname(os.path.abspath(__file__));sys.path.insert(0,HERE)
OUT=os.path.join(HERE,'fixtures','read_tag_tabs.json')
def record(session):
    from hydrus.client import ClientConstants as CC,ClientLocation
    from hydrus.client.gui import ClientGUIAsync
    from hydrus.client.gui.search import ClientGUIACDropdown as A
    from hydrus.client.metadata import ClientContentUpdates as U
    from hydrus.client.search import ClientSearchFileSearchContext as F,ClientSearchTagContext as T,ClientSearchPredicate as P
    from hydrus.core import HydrusConstants as HC
    c=session.controller;qt=lambda f:c.CallBlockingToQt(c.gui,f)
    service=next(s.GetServiceKey() for s in c.services_manager.GetServices((HC.LOCAL_TAG,)) if s.GetName()=='my tags')
    manifest=json.load(open(os.path.join(HERE,'fixtures','legacy_db','basic.manifest.json')))
    hashes=[bytes.fromhex(f['hash']) for f in manifest['files'][:4]]
    corpus=[('parity:tabs alpha',hashes[:3]),('parity:tabs beta',hashes[1:2]),('parity:tabs alpha old',hashes[:1])]
    parents=[['parity:tabs alpha','parity:tabs root'],['parity:tabs beta','parity:tabs root'],['parity:tabs zero','parity:tabs root'],['parity:tabs deep','parity:tabs alpha']]
    literal_sources=[('system:inbox','parity:literal system child','parity:literal system leaf'),('parity:literal*root','parity:literal wildcard child','parity:literal wildcard leaf')]
    for parent,child,leaf in literal_sources:
        corpus.extend([(child,hashes[:2]),(leaf,hashes[:1])])
        parents.extend([[child,parent],[leaf,child]])
    # Qt GetTagId cleans both relation writes and descendant lookups. This
    # inbox chain must appear even while the active predicate is system:inbox.
    corpus.append(('parity:cleaned spelling trap',hashes[:3]))
    parents.append(['parity:cleaned spelling trap','inbox'])
    siblings=[['parity:tabs alpha old','parity:tabs alpha']]
    updates=[U.ContentUpdate(HC.CONTENT_TYPE_MAPPINGS,HC.CONTENT_UPDATE_ADD,(tag,set(files))) for tag,files in corpus]
    for kind,pairs in [(HC.CONTENT_TYPE_TAG_SIBLINGS,siblings),(HC.CONTENT_TYPE_TAG_PARENTS,parents)]:updates.extend(U.ContentUpdate(kind,HC.CONTENT_UPDATE_ADD,tuple(pair)) for pair in pairs)
    c.WriteSynchronous('content_updates',U.ContentUpdatePackage.STATICCreateFromContentUpdates(service,updates))
    for _ in range(12):
        if not c.WriteSynchronous('sync_tag_display_maintenance',service,.5):break
    boxes=[]
    old_thread=c.CallToThread
    c.CallToThread=lambda func,*args,**kw:None if func==A.ReadFetch or getattr(getattr(func,'__self__',None),'_win',None) in boxes else old_thread(func,*args,**kw)
    context=F.FileSearchContext(location_context=ClientLocation.LocationContext.STATICCreateSimple(CC.LOCAL_FILE_SERVICE_KEY),tag_context=T.TagContext(service_key=service))
    ac=qt(lambda:A.AutoCompleteDropdownTagsRead(c.gui,b'parity read tabs',context,synchronised=False))
    boxes.extend([ac._favourites_list,ac._children_list])
    def replay():
        events=[]
        favourites=['parity:tabs root','parity:tabs alpha old','parity:tabs unseen']
        c.new_options.SetStringList('favourite_tags',favourites)
        def snap(action,**extra):
            box=ac._dropdown_notebook.currentWidget()
            rows=[]
            for term in box._ordered_terms:
                rows.append(dict(tag=term.GetPredicate().GetValue(),rows=[''.join(t for t,_ in row) for row in box._GetRowsOfTextsAndColours(term)]))
            search=ac.GetFileSearchContext()
            events.append(dict(action=action,tab=ac._dropdown_notebook.currentIndex(),tab_label=ac._dropdown_notebook.tabText(ac._dropdown_notebook.currentIndex()),text=ac._text_ctrl.text(),rows=rows,predicates=sorted(p.ToString() for p in search.GetPredicates()),**extra))
        def favourites_lookup():
            box=ac._favourites_list
            with box._async_text_info_lock:box._pending_async_text_info_terms.update(box._ordered_terms)
            job=box._async_text_info_updater;job._publish_callable(job._work_callable(job._pre_work_callable()))
        ac.RefreshFavouriteTags();ac._dropdown_notebook.setCurrentWidget(ac._favourites_list);favourites_lookup();snap('favourites',favourites=favourites)
        ac._text_ctrl.setText('draft content');ac._dropdown_notebook.setCurrentWidget(ac._favourites_list);snap('favourites_with_text')
        ac.BroadcastChoices({P.Predicate(P.PREDICATE_TYPE_TAG,'parity:tabs root')});snap('choose_favourite',query_count=len(c.Read('file_query_ids',ac.GetFileSearchContext())))
        old_start=ClientGUIAsync.AsyncQtJob.start;ClientGUIAsync.AsyncQtJob.start=lambda job:job._publish_callable(job._work_callable())
        try:
            for limit in [40,1,0,None]:
                c.new_options.SetNoneableInteger('num_to_show_in_ac_dropdown_children_tab',limit)
                ac._dropdown_notebook.setCurrentWidget(ac._children_list);ac._children_list.NotifyNeedsUpdating();ac._UpdateChildrenListIfNeeded();snap('children',limit=limit)
            ac.BroadcastChoices({P.Predicate(P.PREDICATE_TYPE_TAG,'parity:tabs alpha')});ac._UpdateChildrenListIfNeeded();snap('choose_child')
            ac._predicates_listbox.EnterPredicates({P.Predicate(P.PREDICATE_TYPE_TAG,'parity:tabs alpha')});ac._UpdateChildrenListIfNeeded();snap('remove_child')
            ac._text_ctrl.setText('parity:tabs');snap('type_returns_to_search',pending_results=True)
            ac._text_ctrl.clear();ac._dropdown_notebook.setCurrentWidget(ac._children_list)
            ac._tag_context_button.SetIncludeCurrent(False);ac._tag_context_button.SetIncludePending(False)
            ac._children_list.NotifyNeedsUpdating();ac._UpdateChildrenListIfNeeded();snap('children_without_search_tag_flags')
            ac.SetTagServiceKey(CC.COMBINED_TAG_SERVICE_KEY);ac._children_list.NotifyNeedsUpdating();ac._UpdateChildrenListIfNeeded();snap('all_known_children')
            literal_cases=[]
            for parent,child,leaf in literal_sources:
                c.new_options.SetStringList('favourite_tags',[parent])
                literal=A.AutoCompleteDropdownTagsRead(c.gui,b'parity literal children',context,synchronised=False)
                boxes.extend([literal._favourites_list,literal._children_list])
                steps=[]
                def literal_snap(action):
                    box=literal._dropdown_notebook.currentWidget()
                    steps.append(dict(action=action,tab=literal._dropdown_notebook.currentIndex(),text=literal._text_ctrl.text(),rows=[dict(tag=t.GetPredicate().GetValue(),rows=[''.join(v for v,_ in r) for r in box._GetRowsOfTextsAndColours(t)]) for t in box._ordered_terms],active=sorted([dict(kind='tag' if p.GetType()==P.PREDICATE_TYPE_TAG else str(p.GetType()),value=p.GetValue(),inclusive=p.IsInclusive()) for p in literal.GetFileSearchContext().GetPredicates()],key=lambda p:p['value']),context_tags=sorted(literal._current_context_tags)))
                literal.RefreshFavouriteTags();literal._dropdown_notebook.setCurrentWidget(literal._favourites_list)
                literal_snap('favourite')
                literal._favourites_list._SelectAll();literal._favourites_list._Activate(False,False)
                literal._dropdown_notebook.setCurrentWidget(literal._children_list);literal._UpdateChildrenListIfNeeded()
                literal_snap('activate_parent')
                at=next(i for i,t in enumerate(literal._children_list._ordered_terms) if t.GetPredicate().GetValue()==child)
                literal._children_list._Hit(False,False,at);literal._children_list._Activate(False,False);literal._UpdateChildrenListIfNeeded()
                literal_snap('activate_child')
                literal._predicates_listbox.EnterPredicates({P.Predicate(P.PREDICATE_TYPE_TAG,child)});literal._UpdateChildrenListIfNeeded()
                literal_snap('remove_child')
                literal._predicates_listbox.EnterPredicates({P.Predicate(P.PREDICATE_TYPE_TAG,parent)});literal._UpdateChildrenListIfNeeded()
                literal_snap('remove_parent')
                restored=context.Duplicate();restored.SetPredicates([P.Predicate(P.PREDICATE_TYPE_TAG,parent,inclusive=False)])
                literal.SetFileSearchContext(restored);literal._dropdown_notebook.setCurrentWidget(literal._children_list);literal._UpdateChildrenListIfNeeded()
                literal_snap('restore_negative_parent')
                literal_cases.append(dict(parent=parent,child=child,leaf=leaf,events=steps))
                literal.deleteLater()
        finally:ClientGUIAsync.AsyncQtJob.start=old_start
        ac.deleteLater()
        return events,literal_cases
    try:events,literal_cases=qt(replay)
    finally:c.CallToThread=old_thread
    return dict(corpus=[dict(tag=t,hashes=[h.hex() for h in hs]) for t,hs in corpus],parents=parents,siblings=siblings,events=events,literal_cases=literal_cases)
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

#!/usr/bin/env python3
"""Record actual read/write favourite and children multi-selection consumers.

The basic fixture receives synthetic local mappings and sibling/parent chains.
Actual Qt list _Hit and _Activate handlers drive reversible Ctrl/Shift selection,
normal predicate batches and Shift OR drafts. Real read-list QAction dispatch
records favourite add/removal questions, No/Yes, refresh, tab retention, and a
new dropdown opened after editing. Write lists broadcast selected favourites
and children to their real external entry callback. Only asynchronous scheduling
is made synchronous; queries, list handlers and broadcast consumers are real.
No remote data or file deletion is involved.
"""
import json, os, sys, tempfile
HERE=os.path.dirname(os.path.abspath(__file__));sys.path.insert(0,HERE)
OUT=os.path.join(HERE,'fixtures','autocomplete_tab_selection.json')
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
    siblings=[['parity:tabs alpha old','parity:tabs alpha']]
    updates=[U.ContentUpdate(HC.CONTENT_TYPE_MAPPINGS,HC.CONTENT_UPDATE_ADD,(tag,set(files))) for tag,files in corpus]
    for kind,pairs in [(HC.CONTENT_TYPE_TAG_SIBLINGS,siblings),(HC.CONTENT_TYPE_TAG_PARENTS,parents)]:updates.extend(U.ContentUpdate(kind,HC.CONTENT_UPDATE_ADD,tuple(pair)) for pair in pairs)
    c.WriteSynchronous('content_updates',U.ContentUpdatePackage.STATICCreateFromContentUpdates(service,updates))
    for _ in range(12):
        if not c.WriteSynchronous('sync_tag_display_maintenance',service,.5):break
    boxes=[]
    old_thread=c.CallToThread
    c.CallToThread=lambda func,*args,**kw:None if func in (A.ReadFetch,A.WriteFetch) or getattr(getattr(func,'__self__',None),'_win',None) in boxes else old_thread(func,*args,**kw)
    context=F.FileSearchContext(location_context=ClientLocation.LocationContext.STATICCreateSimple(CC.LOCAL_FILE_SERVICE_KEY),tag_context=T.TagContext(service_key=service))
    ac=qt(lambda:A.AutoCompleteDropdownTagsRead(c.gui,b'parity read tabs',context,synchronised=False))
    boxes.extend([ac._favourites_list,ac._children_list])
    def replay():
        from qtpy import QtCore, QtWidgets
        from hydrus.client.gui import ClientGUICore, ClientGUIDialogsQuick, ClientGUIAsync
        from hydrus.client.gui.lists import ClientGUIListBoxes
        events=[]; captured={}; answer={'yes':False}; asked=[]
        old_popup=ClientGUICore.core().PopupMenu;old_yes=ClientGUIDialogsQuick.GetYesNo;old_start=ClientGUIAsync.AsyncQtJob.start
        ClientGUICore.core().PopupMenu=lambda win,menu:captured.update(menu=menu)
        ClientGUIDialogsQuick.GetYesNo=lambda win,message,*args,**kwargs:(asked.append(message) or (QtWidgets.QDialog.DialogCode.Accepted if answer['yes'] else QtWidgets.QDialog.DialogCode.Rejected))
        ClientGUIAsync.AsyncQtJob.start=lambda job:job._publish_callable(job._work_callable())
        favourites=['parity:tabs root','parity:tabs alpha','parity:tabs unseen']
        c.new_options.SetStringList('favourite_tags',favourites)
        ac.RefreshFavouriteTags();ac._dropdown_notebook.setCurrentWidget(ac._favourites_list)
        def snap(action,**extra):
            box=ac._dropdown_notebook.currentWidget()
            events.append(dict(action=action,tab=ac._dropdown_notebook.currentIndex(),rows=[t.GetPredicate().GetValue() for t in box._ordered_terms],selected=[t.GetPredicate().GetValue() for t in box._ordered_terms if t in box._selected_terms],predicates=sorted(p.ToString() for p in ac.GetFileSearchContext().GetPredicates()),favourites=sorted(c.new_options.GetStringList('favourite_tags')),text=ac._text_ctrl.text(),**extra))
        def hit(i,ctrl=False,shift=False):
            ac._dropdown_notebook.currentWidget()._Hit(shift,ctrl,i);snap('hit',index=i,ctrl=ctrl,shift=shift)
        def paths(menu):
            for action in menu.actions():
                if action.isSeparator():continue
                if action.menu() is not None:yield from paths(action.menu())
                else:yield action
        try:
            snap('favourites')
            hit(0);hit(2,True);hit(1,False,True);hit(0,True,True)
            ac._favourites_list._Activate(False,False);snap('activate_favourites')
            ac._predicates_listbox.EnterPredicates(set(ac.GetFileSearchContext().GetPredicates()));snap('clear_search')
            ac._favourites_list._SelectAll();snap('select_all')
            ac._favourites_list._Activate(False,True);snap('shift_activate_favourites',or_terms=sorted(p.ToString() for p in ac._under_construction_or_predicate.GetValue()))
            ac._CancelORConstruction();snap('cancel_or')
            ac._favourites_list._DeselectAll();snap('deselect');hit(1)
            for yes in [False,True]:
                answer['yes']=yes;asked.clear();captured.clear()
                ac._favourites_list.ShowMenuFromSignal(QtCore.QPoint(0,0))
                menu=captured['menu'];label='remove "parity:tabs root" from favourites'
                next(a for a in paths(menu) if a.text()==label).trigger()
                ac.RefreshFavouriteTags();snap('remove_favourite',yes=yes,questions=list(asked),label=label)
            ac.BroadcastChoices({P.Predicate(P.PREDICATE_TYPE_TAG,'parity:tabs root')})
            ac._dropdown_notebook.setCurrentWidget(ac._children_list);ac._children_list.NotifyNeedsUpdating();ac._UpdateChildrenListIfNeeded();snap('children')
            hit(0);hit(2,True);ac._children_list._Activate(False,False);snap('activate_children',query_count=len(c.Read('file_query_ids',ac.GetFileSearchContext())))
            ac._children_list._DeselectAll();snap('deselect');hit(0)
            captured.clear();ac._children_list.ShowMenuFromSignal(QtCore.QPoint(0,0));menu=captured['menu']
            chosen=ac._children_list._ordered_terms[0].GetPredicate().GetValue();label=f'add "{chosen}" to favourites'
            next(a for a in paths(menu) if a.text()==label).trigger();ac.RefreshFavouriteTags();snap('add_child_favourite',label=label)
            ac._dropdown_notebook.setCurrentWidget(ac._favourites_list);snap('return_favourites')
            ac._dropdown_notebook.setCurrentWidget(ac._children_list);snap('return_children')
            reopened=A.AutoCompleteDropdownTagsRead(c.gui,b'parity reopened tabs',context,synchronised=False)
            events.append(dict(action='reopen',rows=[t.GetPredicate().GetValue() for t in reopened._favourites_list._ordered_terms]))
            reopened.deleteLater()
            entered=[]
            write=A.AutoCompleteDropdownTagsWrite(c.gui,lambda tags:entered.append(sorted(tags)),context.GetLocationContext(),service,show_paste_button=True)
            write.SetContextTags(['parity:tabs root'])
            for tab in [write._favourites_list,write._children_list]:
                write._dropdown_notebook.setCurrentWidget(tab)
                write._UpdateChildrenListIfNeeded()
                tab._DeselectAll();tab._Hit(False,False,0);tab._Hit(False,True,1)
                selected=sorted(tab._GetTagsFromTerms(tab._selected_terms))
                tab._Activate(False,False)
                events.append(dict(action='write_activate',tab=write._dropdown_notebook.currentIndex(),selected=selected,entered=entered[-1],text=write._text_ctrl.text()))
            write.deleteLater()
        finally:
            ClientGUICore.core().PopupMenu=old_popup;ClientGUIDialogsQuick.GetYesNo=old_yes;ClientGUIAsync.AsyncQtJob.start=old_start
            ac.deleteLater()
        return events
    try:events=qt(replay)
    finally:c.CallToThread=old_thread
    return dict(corpus=[dict(tag=t,hashes=[h.hex() for h in hs]) for t,hs in corpus],parents=parents,siblings=siblings,events=events)
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

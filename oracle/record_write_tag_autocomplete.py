#!/usr/bin/env python3
"""Drive real write-autocomplete fetch, highlighted choices and paste questions.

Synthetic parity tags are seeded through real mapping/sibling/parent updates.
WriteFetch runs the real database query and predicate insertion pipeline; its
results are installed in the real Qt list and every rendered row is recorded.
"""
import json, os, sys, tempfile
HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
OUT = os.path.join(HERE, 'fixtures', 'write_tag_autocomplete.json')
def record(session):
    from qtpy import QtWidgets as W
    from hydrus.client import ClientConstants as CC, ClientLocation, ClientThreading
    from hydrus.client.gui.search import ClientGUIACDropdown as A
    from hydrus.client.gui import ClientGUIDialogsQuick as Q
    from hydrus.client.metadata import ClientContentUpdates as U
    from hydrus.client.search import ClientSearchAutocomplete as SA, ClientSearchFileSearchContext as SC
    from hydrus.core import HydrusConstants as HC
    c = session.controller
    qt = lambda f: c.CallBlockingToQt(c.gui, f)
    local = qt(lambda: next(s.GetServiceKey() for s in c.services_manager.GetServices((HC.LOCAL_TAG,)) if s.GetName() == 'my tags'))
    manifest = json.load(open(os.path.join(HERE, 'fixtures', 'legacy_db', 'basic.manifest.json')))
    hashes = [bytes.fromhex(f['hash']) for f in manifest['files'][:4]]
    corpus = [('parity:amber',hashes[:3]),('parity:amber old',hashes[:1]),('parity:amethyst',hashes[1:2]),('parity:amaranth',hashes[2:3])]
    updates = [U.ContentUpdate(HC.CONTENT_TYPE_MAPPINGS,HC.CONTENT_UPDATE_ADD,(tag,set(hs))) for tag,hs in corpus]
    updates += [U.ContentUpdate(HC.CONTENT_TYPE_TAG_SIBLINGS,HC.CONTENT_UPDATE_ADD,('parity:amber old','parity:amber')),U.ContentUpdate(HC.CONTENT_TYPE_TAG_PARENTS,HC.CONTENT_UPDATE_ADD,('parity:amber','parity:colour')),U.ContentUpdate(HC.CONTENT_TYPE_TAG_PARENTS,HC.CONTENT_UPDATE_ADD,('parity:colour','parity:root'))]
    c.WriteSynchronous('content_updates', U.ContentUpdatePackage.STATICCreateFromContentUpdates(local,updates))
    for _ in range(12):
        if not c.WriteSynchronous('sync_tag_display_maintenance',local,0.5): break
    choices=[]; pasted=[]; asked=[]; clipboard={'text':''}; answer={'yes':False}
    old_clipboard=c.GetClipboardText;c.GetClipboardText=lambda:clipboard['text']
    Q.GetYesNo=lambda parent,message,**kw:(asked.append({'message':message,'title':kw.get('title')}),W.QDialog.DialogCode.Accepted if answer['yes'] else W.QDialog.DialogCode.Rejected)[1]
    old_thread=c.CallToThread
    c.CallToThread=lambda func,*args,**kw:None if func == A.WriteFetch else old_thread(func,*args,**kw)
    location=ClientLocation.LocationContext.STATICCreateSimple(CC.LOCAL_FILE_SERVICE_KEY)
    ac=qt(lambda:A.AutoCompleteDropdownTagsWrite(c.gui,lambda tags:choices.append(sorted(tags)),location,local,show_paste_button=True))
    qt(lambda:ac.tagsPasted.connect(lambda tags:pasted.append(sorted(tags))))
    def controls():
        from hydrus.client.gui.panels.options.TagEditingPanel import TagEditingPanel
        panel=TagEditingPanel(c.gui,c.new_options)
        fields=['_ac_select_first_with_count','_skip_yesno_on_write_autocomplete_multiline_paste','_show_parent_decorators_on_storage_autocomplete_taglists','_expand_parents_on_storage_autocomplete_taglists','_show_sibling_decorators_on_storage_autocomplete_taglists']
        values=[getattr(panel,name).isChecked() for name in fields]
        height=panel._ac_write_list_height_num_chars
        result={'values':values,'height':height.value(),'minimum':height.minimum(),'maximum':height.maximum()}
        panel.deleteLater()
        return result
    option_controls=qt(controls)
    queries=[]
    def fetch(text,first=False,expand=True,parents=True,siblings=True,service=None):
        def prepare():
            c.new_options.SetBoolean('ac_select_first_with_count',first)
            ac._search_results_list.SetExtraParentRowsAllowed(expand)
            ac._search_results_list.SetParentDecoratorsAllowed(parents)
            ac._search_results_list.SetSiblingDecoratorsAllowed(siblings)
            ac._text_ctrl.setText(text)
            if service is not None:
                ctx=ac._tag_context_button.GetValue();ctx.service_key=service;ac._tag_context_button.SetValue(ctx)
            ac.CancelCurrentResultsFetchJob()
            return ac._GetParsedAutocompleteText(),SC.FileSearchContext(location_context=ac._location_context_button.GetValue(),tag_context=ac._tag_context_button.GetValue())
        parsed,context=qt(prepare)
        captured={}
        def prefetch(*args):pass
        def results(job,parsed,cache,matches):captured['matches']=matches
        old_after=c.CallAfterQtSafe
        c.CallAfterQtSafe=lambda target,func,*args,**kw:func(*args,**kw) if target == ac and func in (prefetch,results) else old_after(target,func,*args,**kw)
        try:A.WriteFetch(ac,ClientThreading.JobStatus(),prefetch,results,parsed,context,SA.PredicateResultsCacheInit())
        finally:c.CallAfterQtSafe=old_after
        matches=captured['matches']
        def show():
            box=ac._search_results_list;box.SetPredicates([]);box.SetPredicates(matches)
            rows=[]
            for term in box._ordered_terms:
                texts=term.GetRowsOfPresentationTextsWithNamespaces(True,siblings,' → ',None,parents,expand)
                rows.append({'tag':term.GetPredicate().GetValue(),'count':term.GetPredicate().GetCount().HasNonZeroCount(),'rows':[''.join(t[0] for t in row) for row in texts]})
            selected=sorted(t.GetPredicate().GetValue() for t in box._selected_terms)
            return {'text':text,'first_with_count':first,'expanded':expand,'parents':parents,'siblings':siblings,'service':c.services_manager.GetName(context.GetTagContext().service_key),'suggestions':rows,'selected':selected}
        queries.append(qt(show))
    fetch('parity:amb');fetch('parity:amb',True);fetch('parity:am');fetch('parity:amber old',False,False);fetch('parity:amber old',False,False,False,False)
    fetch('parity:amb',False,False,True,True,CC.COMBINED_TAG_SERVICE_KEY)
    def tabs():
        from hydrus.client.gui import ClientGUIAsync
        from hydrus.client.gui.panels.options.TagsPanel import TagsPanel
        events=[]
        favourite_tags=['parity:root','parity:amber old','parity:favorite new']
        c.new_options.SetStringList('favourite_tags',favourite_tags)
        def rows(box):
            result=[]
            for term in box._ordered_terms:
                texts=term.GetRowsOfPresentationTextsWithNamespaces(True,box._show_sibling_decorators,' → ',None,box._show_parent_decorators,box._extra_parent_rows_allowed)
                result.append({'tag':term.GetTag() if hasattr(term,'GetTag') else term.GetPredicate().GetValue(),'rows':[''.join(t[0] for t in row) for row in texts]})
            return result
        old_threads=c.CallToThread
        favourite_box=ac._favourites_list
        c.CallToThread=lambda func,*args,**kw:None if getattr(getattr(func,'__self__',None),'_win',None) is favourite_box else old_threads(func,*args,**kw)
        try:
            for service in (local,CC.COMBINED_TAG_SERVICE_KEY):
                favourite_box.SetTagServiceKey(service);ac.RefreshFavouriteTags()
                with favourite_box._async_text_info_lock:favourite_box._pending_async_text_info_terms.update(favourite_box._ordered_terms)
                updater=favourite_box._async_text_info_updater
                updater._publish_callable(updater._work_callable(updater._pre_work_callable()))
                events.append({'tab':'favourites','service':c.services_manager.GetName(service),'tags':favourite_tags,'rows':rows(favourite_box)})
        finally:c.CallToThread=old_threads
        # Run the real children worker/publisher synchronously, so no thread race
        # changes the snapshot. It still performs real descendants/count DB reads.
        old_start=ClientGUIAsync.AsyncQtJob.start
        ClientGUIAsync.AsyncQtJob.start=lambda job:job._publish_callable(job._work_callable())
        try:
            ac._children_list.SetTagServiceKey(local)
            for context,limit in [(['parity:root'],40),(['parity:root'],1),(['parity:root'],None),(['parity:root','parity:colour'],40)]:
                c.new_options.SetNoneableInteger('num_to_show_in_ac_dropdown_children_tab',limit)
                ac._children_list.NotifyNeedsUpdating();ac._children_list.UpdateChildrenIfNeeded(context)
                events.append({'tab':'children','context':context,'limit':limit,'rows':rows(ac._children_list)})
        finally:ClientGUIAsync.AsyncQtJob.start=old_start
        c.new_options.SetNoneableInteger('num_to_show_in_ac_dropdown_children_tab',40)
        panel=TagsPanel(c.gui,c.new_options)
        control=panel._num_to_show_in_ac_dropdown_children_tab
        bounds={'value':control.GetValue(),'min':control._number_value.minimum(),'max':control._number_value.maximum()}
        panel.deleteLater()
        return events,bounds
    tab_events,children_control=qt(tabs)
    def context_menus():
        from qtpy import QtCore
        from hydrus.client.gui import ClientGUICore as CGC
        from hydrus.client.gui.lists import ClientGUIListBoxes as L
        events=[]; captured={}; copied=[]
        box=ac._search_results_list
        box.SetParentDecoratorsAllowed(True);box.SetExtraParentRowsAllowed(True);box.SetSiblingDecoratorsAllowed(True)
        old_popup=CGC.core().PopupMenu
        old_pub=c.pub
        CGC.core().PopupMenu=lambda win,menu:captured.update(menu=menu)
        launches=[]
        def publish(topic,*args,**kw):
            if topic == 'clipboard':copied.append(args[1])
            elif topic in ('new_page_query','new_page_duplicates'):
                launches.append({'topic':topic,'current':[key.hex() for key in sorted(args[0].current_service_keys)],'deleted':[key.hex() for key in sorted(args[0].deleted_service_keys)],'predicates':[p.ToString() for p in kw['initial_predicates']],'page_name':kw['page_name'],'activate_window':kw['activate_window']})
            else:old_pub(topic,*args,**kw)
        c.pub=publish
        def paths(menu,prefix=()):
            result=[]
            for action in menu.actions():
                if action.isSeparator():continue
                path=prefix+(action.text(),)
                result.extend(paths(action.menu(),path) if action.menu() is not None else [(path,action)])
            return result
        try:
            term=next(t for t in box._ordered_terms if t.GetPredicate().GetValue() == 'parity:amber old')
            box._selected_terms={term}
            box.ShowMenuFromSignal(QtCore.QPoint(0,0))
            menu=captured['menu'];actions=paths(menu)
            events.append({'action':'open','paths':[list(path) for path,action in actions]})
            for label in ['parity:amber old','amber old','amber_old','parity:amber old with counts','amber old with counts','parity:amber old and 2 parents','all tags','all subtags']:
                action=next(action for path,action in actions if path == ('copy',label));copied.clear();action.trigger()
                events.append({'action':'copy','label':label,'copied':list(copied)})
            for prefix in ['open a new search page for ','open a new duplicate filter page for ']:
                path,action=next((path,action) for path,action in actions if path[0] == 'open' and path[-1].startswith(prefix));label=path[-1];launches.clear();action.trigger()
                events.append({'action':'launch','label':label,'launched':list(launches)})
            def fav_menu():
                menu=W.QMenu(box);L.AddTagFavouritesMenu(box,menu,'parity:menu new');return menu
            for label,yes in [('add "parity:menu new" to favourites',False),('remove "parity:menu new" from favourites',False),('remove "parity:menu new" from favourites',True),('add "parity:menu new" to most used for "my tags"',False),('remove "parity:menu new" from most used for "my tags"',False),('remove "parity:menu new" from most used for "my tags"',True)]:
                answer['yes']=yes;asked.clear();menu=fav_menu();action=next(action for path,action in paths(menu) if path[-1] == label);action.trigger()
                events.append({'action':'favourite','label':label,'answer':yes,'asked':list(asked),'favourites':c.new_options.GetStringList('favourite_tags'),'most_used':sorted(c.new_options.GetSuggestedTagsMostUsed(local))})
            for label,setter,flag in [('collapse parent rows',box.SetExtraParentRowsAllowed,False),('hide parent decorators',box.SetParentDecoratorsAllowed,False),('hide sibling decorators',box.SetSiblingDecoratorsAllowed,False)]:
                menu=captured['menu'];action=next(action for path,action in paths(menu) if path == (label,));action.trigger();box.ShowMenuFromSignal(QtCore.QPoint(0,0))
                events.append({'action':'decorator','label':label,'paths':[list(path) for path,action in paths(captured['menu'])]})
        finally:
            c.pub=old_pub;CGC.core().PopupMenu=old_popup
        return events
    # Restore the local counted suggestions before driving the real tag menu.
    fetch('parity:amber old',False,True,True,True,local)
    menu_events=qt(context_menus)
    paste_events=[]
    for text,skip,yes,button in [(' Parity:Amber \nparity:new\nparity:new\n\n',False,False,False),(' Parity:Amber \nparity:new\nparity:new\n\n',False,True,False),('parity:skip a\nparity:skip b',True,False,False),('parity:button a\nparity:button b',False,False,True),('parity:single',False,False,False)]:
        clipboard['text']=text;answer['yes']=yes;qt(lambda:c.new_options.SetBoolean('skip_yesno_on_write_autocomplete_multiline_paste',skip))
        consumed=qt(lambda:ac._Paste() if button else ac._TryToProcessAPasteEvent())
        paste_events.append({'text':text,'skip':skip,'answer':yes,'button':button,'consumed':consumed,'asked':list(asked),'pasted':list(pasted)})
        asked.clear();pasted.clear()
    def relation_inputs():
        from hydrus.client.gui.metadata.ClientGUIManageTagSiblings import ManageTagSiblings
        from hydrus.client.gui.metadata.ClientGUIManageTagParents import ManageTagParents
        results=[]
        for kind, cls in [('siblings',ManageTagSiblings),('parents',ManageTagParents)]:
            panel=cls._Panel(c.gui,local)
            inputs=(panel._old_input,panel._new_input) if kind == 'siblings' else (panel._children_input,panel._parents_input)
            def snapshot(action):
                left=panel._old_siblings.GetTags() if kind == 'siblings' else panel._children.GetTags()
                right=[] if kind == 'siblings' and panel._current_new is None else ([panel._current_new] if kind == 'siblings' else panel._parents.GetTags())
                results.append({'kind':kind,'action':action,'left':sorted(left),'right':sorted(right)})
            left_text='parity:paste left a\nparity:paste left b'
            right_text='parity:paste right' if kind == 'siblings' else 'parity:paste right a\nparity:paste right b'
            clipboard['text']=left_text;inputs[0]._Paste();snapshot('paste_left')
            inputs[0]._Paste();snapshot('repeat_left')
            clipboard['text']=right_text;inputs[1]._Paste();snapshot('paste_right')
            inputs[1]._Paste();snapshot('repeat_right')
            clipboard['text']='parity:paste left a';inputs[1]._Paste();snapshot('move_to_right')
            panel.deleteLater()
        return results
    relationship_inputs=qt(relation_inputs)
    def detached_tag_lists():
        from hydrus.client.gui import ClientGUIDialogs
        from hydrus.client.metadata import ClientTags
        events=[]
        for kind,key,display in [('additional',local,ClientTags.TAG_DISPLAY_STORAGE),('whitelist',CC.COMBINED_TAG_SERVICE_KEY,ClientTags.TAG_DISPLAY_DISPLAY_ACTUAL)]:
            initial=['parity:caller initial']
            dialog=ClientGUIDialogs.DialogInputTags(c.gui,key,display,initial)
            def snapshot(action):events.append({'kind':kind,'action':action,'tags':sorted(dialog.GetTags()),'caller':list(initial)})
            snapshot('initial')
            clipboard['text']='parity:caller initial\nparity:child new';dialog._tag_autocomplete._Paste();snapshot('paste')
            dialog._tag_autocomplete._Paste();snapshot('repeat_paste')
            dialog.EnterTags({'parity:caller initial'});snapshot('typed_toggle')
            dialog.reject();snapshot('cancel');dialog.deleteLater()
        return events
    detached_inputs=qt(detached_tag_lists)
    qt(ac.deleteLater);c.CallToThread=old_thread;c.GetClipboardText=old_clipboard
    return {'menus':menu_events,'tabs':tab_events,'children_control':children_control,'detached_inputs':detached_inputs,'relationship_inputs':relationship_inputs,'controls':option_controls,'corpus':[{'tag':tag,'hashes':[h.hex() for h in hs]} for tag,hs in corpus],'queries':queries,'paste':paste_events}
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

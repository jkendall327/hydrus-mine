#!/usr/bin/env python3
"""Record real write-autocomplete logical multi-selection and batch activation.

Actual WriteFetch populates the real Qt predicate list. Plain/Ctrl/Shift and
Ctrl+Shift hits drive Qt's existing reversible selection algorithm; inherited
physical rows map to the originating logical tag. Activation calls the real
list's handler and write-dropdown broadcast, including clearing its input. The
real multi-tag menu records copy payloads and AND/OR/each-page/duplicate launches.
Maintenance records the actual question and accepted/declined write dispatch;
the database repair job is intercepted at dispatch, not run by this recorder.
Ctrl+V events use the real QLineEdit clipboard handler and selection range to
record normal paste after declining the multiline-tag question. Real result-list
key events also record logical wrap/range/navigation and selected clipboard
output, with an explicit three-physical-row page size. Focused Escape events
and actual mouse press/move/release handlers record deselection and reversible
add/remove dragging, including an inherited parent row. The actual tagsPasted
signal is also consumed twice by a single-file Manage Tags child, recording its
all-file storage counts and preserved input independently from suggestion labels.
Successive replacements,
normal typing, Undo/Redo and redo invalidation use real input key events and
capture both draft text and restored selection/cursor.
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
    entered=[];paste_signals=[]
    ac=qt(lambda:A.AutoCompleteDropdownTagsWrite(c.gui,lambda tags:entered.append(sorted(tags)),location,service,show_paste_button=True))
    qt(lambda:ac.tagsPasted.connect(lambda tags:paste_signals.append(sorted(tags))))
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
            from qtpy import QtWidgets
            from hydrus.client.gui import ClientGUIDialogsQuick
            old_yes_no=ClientGUIDialogsQuick.GetYesNo;old_write=c.Write
            try:
                for yes in (False,True):
                    asked=[];writes=[]
                    def question(parent,message,**kw):
                        asked.append(dict(message=message,**kw));return QtWidgets.QDialog.DialogCode.Accepted if yes else QtWidgets.QDialog.DialogCode.Rejected
                    def write(name,*args,**kw):
                        if name=='regenerate_tag_mappings_tags':writes.append(dict(name=name,tags=sorted(args[0])))
                        else:return old_write(name,*args,**kw)
                    ClientGUIDialogsQuick.GetYesNo=question;c.Write=write
                    next(action for path,action in actions if path==('maintenance','regenerate tag display')).trigger()
                    menus.append(dict(action='regenerate',answer=yes,asked=asked,writes=writes))
            finally:ClientGUIDialogsQuick.GetYesNo=old_yes_no;c.Write=old_write
        finally:
            CGC.core().PopupMenu=old_popup;c.pub=old_pub;ClientGUIAsync.AsyncQtJob.start=old_start
        activated=box._Activate(False,False);steps.append(dict(action='activate',activated=activated,entered=list(entered),text=ac._text_ctrl.text()))
        from qtpy import QtGui
        from hydrus.client.gui import ClientGUIDialogsQuick
        old_yes_no=ClientGUIDialogsQuick.GetYesNo;paste_events=[]
        try:
            for text,anchor,length,pasted,yes in [('draft content',5,0,'parity:paste one\nparity:paste two',False),('draft content',0,5,'parity:paste one\nparity:paste two',False),('draft content',5,0,'parity:single',False),('draft content',0,5,'parity:paste one\nparity:paste two',True)]:
                asked=[];entered.clear();paste_signals.clear()
                def question(parent,message,**kw):
                    asked.append(dict(message=message,**kw));return QtWidgets.QDialog.DialogCode.Accepted if yes else QtWidgets.QDialog.DialogCode.Rejected
                ClientGUIDialogsQuick.GetYesNo=question
                ac._text_ctrl.setText(text);ac._text_ctrl.setSelection(anchor,length)
                QtWidgets.QApplication.clipboard().setText(pasted)
                event=QtGui.QKeyEvent(QtCore.QEvent.Type.KeyPress,QtCore.Qt.Key.Key_V,QtCore.Qt.KeyboardModifier.ControlModifier,'v')
                QtWidgets.QApplication.sendEvent(ac._text_ctrl,event)
                after=ac._text_ctrl.text();ac._text_ctrl.undo();undo=ac._text_ctrl.text();ac._text_ctrl.redo();redo=ac._text_ctrl.text()
                paste_events.append(dict(text=text,anchor=anchor,length=length,pasted=pasted,answer=yes,asked=asked,after=after,undo=undo,redo=redo,entered=list(entered),pasted_tags=list(paste_signals)))
        finally:ClientGUIDialogsQuick.GetYesNo=old_yes_no
        box.SetPredicates([]);box.SetPredicates(captured['matches'])
        box._num_rows_per_page=3
        keyboard=[snapshot('initial')];copies=[];old_pub=c.pub
        def publish(topic,*args,**kw):
            if topic=='clipboard':copies.append(args[1])
            else:old_pub(topic,*args,**kw)
        c.pub=publish
        try:
            for name,ctrl,shift in [('Up',False,False),('Down',False,False),('Down',False,True),('Down',False,True),('Up',False,True),('End',True,True),('Home',False,True),('Home',True,False),('End',False,False),('PageUp',False,False),('PageDown',False,False),('A',True,False),('C',True,False),('C',True,True),('P',True,False),('N',True,False)]:
                modifiers=QtCore.Qt.KeyboardModifier.NoModifier
                if ctrl:modifiers|=QtCore.Qt.KeyboardModifier.ControlModifier
                if shift:modifiers|=QtCore.Qt.KeyboardModifier.ShiftModifier
                copies.clear()
                event=QtGui.QKeyEvent(QtCore.QEvent.Type.KeyPress,getattr(QtCore.Qt.Key,'Key_'+name),modifiers,name if len(name)==1 else '')
                box.keyPressEvent(event)
                keyboard.append(snapshot('key',key=name,ctrl=ctrl,shift=shift,copied=list(copies)))
        finally:c.pub=old_pub
        from qtpy import QtWidgets
        ac.show();box.show();box.setFocus();QtWidgets.QApplication.processEvents()
        assert box.hasFocus(), 'real result widget did not receive focus'
        escape=[snapshot('initial',focused=box.hasFocus())]
        for _ in range(2):
            event=QtGui.QKeyEvent(QtCore.QEvent.Type.KeyPress,QtCore.Qt.Key.Key_Escape,QtCore.Qt.KeyboardModifier.NoModifier)
            box.keyPressEvent(event)
            escape.append(snapshot('escape',accepted=event.isAccepted(),focused=box.hasFocus()))
        box.SetPredicates([]);box.SetPredicates(captured['matches']);box.verticalScrollBar().setValue(0)
        drag=[snapshot('initial')]
        from hydrus.client.gui import QtPorting as QP
        height=box.fontMetrics().height()
        for action,physical,ctrl in [('press',0,False),('drag',6,False),('drag',2,False),('release',2,False),('select_all',0,False),('press',1,True),('drag',6,False),('drag',3,False),('release',3,False)]:
            if action=='select_all':
                box.keyPressEvent(QtGui.QKeyEvent(QtCore.QEvent.Type.KeyPress,QtCore.Qt.Key.Key_A,QtCore.Qt.KeyboardModifier.ControlModifier,'a'))
                drag.append(snapshot(action));continue
            modifiers=QtCore.Qt.KeyboardModifier.ControlModifier if ctrl else QtCore.Qt.KeyboardModifier.NoModifier
            kind={'press':QtCore.QEvent.Type.MouseButtonPress,'drag':QtCore.QEvent.Type.MouseMove,'release':QtCore.QEvent.Type.MouseButtonRelease}[action]
            y=physical*height+height//2
            if action=='drag':y-=QP.ScrollAreaVisibleRect(box).y()
            button=QtCore.Qt.MouseButton.NoButton if action=='drag' else QtCore.Qt.MouseButton.LeftButton
            buttons=QtCore.Qt.MouseButton.NoButton if action=='release' else QtCore.Qt.MouseButton.LeftButton
            event=QtGui.QMouseEvent(kind,QtCore.QPointF(2,y),QtCore.QPointF(2,y),button,buttons,modifiers)
            logical=box._GetLogicalIndexUnderMouse(event)
            expected,_=box._GetLogicalIndicesFromPositionalIndex(physical)
            assert logical==expected,(action,physical,logical,expected)
            if action=='drag':box.mouseMoveEvent(event)
            else:QtWidgets.QApplication.sendEvent(box.widget(),event)
            drag.append(snapshot(action,physical=physical,ctrl=ctrl,deselection=box._this_drag_is_a_deselection if action=='drag' else None))
        # Follow the actual tagsPasted signal into the single-file Manage Tags
        # consumer, preserving the existing input while the paste only adds.
        from hydrus.client.gui.metadata.ClientGUIManageTags import ManageTagsPanel
        from hydrus.client.media import ClientMediaSingle
        media=ClientMediaSingle.MediaSingle(c.Read('media_results',[hashes[0]])[0])
        manage=ManageTagsPanel(c.gui,location,CC.TAG_PRESENTATION_SEARCH_PAGE_MANAGE_TAGS,[media])
        managed=next(page for page in manage._tag_services.GetPages() if page.GetServiceKey()==service)
        pasted=paste_events[-1]['pasted_tags'][0]
        managed._add_tag_box._text_ctrl.setText(paste_events[-1]['text'])
        managed._add_tag_box.tagsPasted.emit(pasted)
        managed._add_tag_box.tagsPasted.emit(pasted)
        clipboard_rows=[{'tag':term.GetTag(),'label':''.join(text for text,_ in managed._tags_box._GetRowsOfTextsAndColours(term)[0])} for term in managed._tags_box._ordered_terms if term.GetTag() in pasted]
        manage_clipboard={'text':managed._add_tag_box._text_ctrl.text(),'rows':clipboard_rows}
        manage.deleteLater()
        # Drive reference replacement history and fresh typing through real keys.
        history=[]
        try:
            ClientGUIDialogsQuick.GetYesNo=lambda *args,**kwargs:QtWidgets.QDialog.DialogCode.Rejected
            sequences=[
                ('successive_replacements','draft content',[('paste','first replacement',0,5),('paste','second replacement',6,11),('undo',),('undo',),('redo',),('redo',)]),
                ('typing_invalidates_redo','draft content',[('paste','first replacement',0,5),('paste','second replacement',6,11),('undo',),('type','x'),('redo',),('undo',)]),
                ('typing_after_replacement','draft content',[('paste','first replacement',0,5),('type','x'),('undo',),('undo',),('redo',),('redo',)]),
                ('replace_suffix','draft content',[('paste','suffix',6,7),('undo',),('redo',)]),
            ]
            for name,text,actions in sequences:
                ac._text_ctrl.setText(text)
                history_steps=[]
                for action in actions:
                    kind=action[0]
                    if kind=='paste':
                        _,pasted,anchor,length=action
                        ac._text_ctrl.setSelection(anchor,length)
                        QtWidgets.QApplication.clipboard().setText(pasted)
                        key=QtCore.Qt.Key.Key_V;modifiers=QtCore.Qt.KeyboardModifier.ControlModifier;value='v'
                    elif kind in ('undo','redo'):
                        key=QtCore.Qt.Key.Key_Z;modifiers=QtCore.Qt.KeyboardModifier.ControlModifier;value='z'
                        if kind=='redo':modifiers|=QtCore.Qt.KeyboardModifier.ShiftModifier
                    else:
                        value=action[1];key=QtCore.Qt.Key.Key_X;modifiers=QtCore.Qt.KeyboardModifier.NoModifier
                    QtWidgets.QApplication.sendEvent(ac._text_ctrl,QtGui.QKeyEvent(QtCore.QEvent.Type.KeyPress,key,modifiers,value))
                    history_steps.append({'action':list(action),'text':ac._text_ctrl.text(),'cursor':ac._text_ctrl.cursorPosition(),'selection_start':ac._text_ctrl.selectionStart(),'selected':ac._text_ctrl.selectedText()})
                history.append({'name':name,'initial':text,'steps':history_steps})
        finally:ClientGUIDialogsQuick.GetYesNo=old_yes_no
        return dict(tags=tags,rows=rows,steps=steps,menus=menus,normal_paste=paste_events,normal_paste_history=history,manage_clipboard=manage_clipboard,keyboard=dict(page_rows=3,steps=keyboard),escape=escape,drag=drag)
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

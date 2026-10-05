#!/usr/bin/env python3
"""Actual Qt external command argument queue and clipboard child routes.

Drive the real queue buttons and QList keyboard selection/copy/delete, including
adjacent selected rows at both reorder boundaries, multi-selected first-row edit,
cleaned Unicode input, cancelled questions and raw split-space clipboard templates.
Only dialogs and clipboard transport are scripted; editing, validation, examples
and handlers are unchanged. No external command or network request is executed.
"""
import json,sys,tempfile
from pathlib import Path
HERE=Path(__file__).resolve().parent;sys.path.insert(0,str(HERE))
def record(session):
    def drive():
        from qtpy import QtCore as QC,QtWidgets as QW,QtTest as QT
        from hydrus.client.gui.panels.options.ExternalProgramsPanel import EditProcessCallExecutableAndParametersPanel as Panel
        from hydrus.client.gui import ClientGUIDialogsQuick as Q,ClientGUIDialogsMessage as M
        from hydrus.core import HydrusExceptions as E
        c=session.controller;old_yes=Q.GetYesNo;old_text=Q.EnterText;old_clip=c.GetClipboardText;old_pub=c.pub;old_error=M.ShowCritical
        answer={'yes':False,'text':'','cancel':False};raw={'text':''};questions=[];entries=[];copies=[];errors=[];panels=[]
        def yes(_p,message,**kw):questions.append(dict(message=message,**kw));return QW.QDialog.DialogCode.Accepted if answer['yes'] else QW.QDialog.DialogCode.Rejected
        def text(_p,message,**kw):
            entries.append(dict(message=message,**kw))
            if answer['cancel']:raise E.CancelledException('scripted cancel')
            return answer['text']
        def clip():
            if raw['text'] is None:raise E.DataMissing('No text on the clipboard!')
            return raw['text']
        def pub(topic,*args,**kw):
            if topic=='clipboard':copies.append(list(args))
            else:old_pub(topic,*args,**kw)
        Q.GetYesNo=yes;Q.EnterText=text;c.GetClipboardText=clip;c.pub=pub;M.ShowCritical=lambda _p,title,message:errors.append(dict(title=title,message=message))
        try:
            p=Panel(c.gui,'owned-program',['zero','one','two','three']);panels.append(p);p.resize(760,590);p.show();QW.QApplication.processEvents();queue=p._executable_parameter_templates;box=queue._listbox
            events=[]
            def snap(action,**extra):events.append(dict(action=action,rows=queue.GetData(),selected=box.GetSelectedIndices(),current=box.currentRow(),value=p.GetValue(),example=p._example_full_command.text(),questions=list(questions),entries=list(entries),copies=list(copies),**extra));questions.clear();entries.clear();copies.clear()
            def select(indices):
                box.clearSelection()
                for i in indices:box.item(i).setSelected(True)
            snap('initial')
            select([0,1]);snap('select_top_pair');queue._Up();snap('up_at_top');queue._Down();snap('down_after_top')
            select([2,3]);queue._Down();snap('down_at_bottom');queue._Up();snap('up_after_bottom')
            select([1,3]);answer['text']='  edited\t日本😀  \nignored';queue._Edit();snap('edit_first_of_multiple')
            answer['cancel']=True;queue._Edit();snap('cancel_edit');answer['cancel']=False
            answer['text']='  added  value  ';queue._Add();snap('add_unselected')
            answer['yes']=False;queue._Delete();snap('decline_delete');answer['yes']=True;queue._Delete();snap('accept_delete')
            # Fresh list: real pointer and keyboard routes, retained focus after updates.
            queue.SetData(['alpha','beta','gamma','delta']);QW.QApplication.processEvents()
            pos=box.visualItemRect(box.item(1)).center();QT.QTest.mouseClick(box.viewport(),QC.Qt.MouseButton.LeftButton,QC.Qt.KeyboardModifier.NoModifier,pos);snap('mouse_beta')
            for name,key,mods in [('shift_down',QC.Qt.Key.Key_Down,QC.Qt.KeyboardModifier.ShiftModifier),('ctrl_home',QC.Qt.Key.Key_Home,QC.Qt.KeyboardModifier.ControlModifier),('ctrl_space',QC.Qt.Key.Key_Space,QC.Qt.KeyboardModifier.ControlModifier),('copy_after_ctrl_space',QC.Qt.Key.Key_C,QC.Qt.KeyboardModifier.ControlModifier),('select_all',QC.Qt.Key.Key_A,QC.Qt.KeyboardModifier.ControlModifier),('copy_selected',QC.Qt.Key.Key_C,QC.Qt.KeyboardModifier.ControlModifier),('copy_insert',QC.Qt.Key.Key_Insert,QC.Qt.KeyboardModifier.ControlModifier),('plain_end',QC.Qt.Key.Key_End,QC.Qt.KeyboardModifier.NoModifier),('shift_up',QC.Qt.Key.Key_Up,QC.Qt.KeyboardModifier.ShiftModifier),('delete_key',QC.Qt.Key.Key_Delete,QC.Qt.KeyboardModifier.NoModifier)]:
                QT.QTest.keyClick(box,key,mods);snap(name)
            # A real clicked current row during boundary reorder, then keyboard range.
            queue.SetData(['alpha','beta','gamma','delta']);QW.QApplication.processEvents()
            QT.QTest.mouseClick(box.viewport(),QC.Qt.MouseButton.LeftButton,QC.Qt.KeyboardModifier.NoModifier,box.visualItemRect(box.item(1)).center());snap('click_before_reorder')
            queue._Up();snap('clicked_up');QT.QTest.keyClick(box,QC.Qt.Key.Key_Down,QC.Qt.KeyboardModifier.ShiftModifier);snap('shift_after_reorder')
            queue.SetData(['alpha','beta','gamma','delta']);QW.QApplication.processEvents()
            QT.QTest.mouseClick(box.viewport(),QC.Qt.MouseButton.LeftButton,QC.Qt.KeyboardModifier.NoModifier,box.visualItemRect(box.item(1)).center());queue._Down();snap('clicked_down')
            edge_histories=[]
            for edge in ['reverse_edit','select_all_contract','prior_shift_select_all','ctrl_home_shift','delete_current_shift','delete_noncurrent_shift']:
                ep=Panel(c.gui,'owned-program',['alpha','beta','gamma','delta']);panels.append(ep);ep.resize(760,590);ep.show();QW.QApplication.processEvents();eq=ep._executable_parameter_templates;eb=eq._listbox;steps=[]
                def edge_snap(action):
                    steps.append(dict(action=action,rows=eq.GetData(),selected=eb.GetSelectedIndices(),current=eb.currentRow(),value=ep.GetValue(),example=ep._example_full_command.text(),questions=list(questions),entries=list(entries),copies=list(copies)));questions.clear();entries.clear();copies.clear()
                def click(row,control=False):
                    QT.QTest.mouseClick(eb.viewport(),QC.Qt.MouseButton.LeftButton,QC.Qt.KeyboardModifier.ControlModifier if control else QC.Qt.KeyboardModifier.NoModifier,eb.visualItemRect(eb.item(row)).center());edge_snap('ctrl_click_1' if control else 'click_'+str(row))
                def key(name,code,mods):QT.QTest.keyClick(eb,code,mods);edge_snap(name)
                edge_snap('initial');click(3 if edge=='reverse_edit' else 1)
                if edge=='reverse_edit':
                    click(1,True);answer['text']='first-added 日本😀';eq._Edit();edge_snap('edit_selection_first')
                else:
                    if edge=='prior_shift_select_all':key('shift_down',QC.Qt.Key.Key_Down,QC.Qt.KeyboardModifier.ShiftModifier)
                    if edge in ('ctrl_home_shift','delete_noncurrent_shift'):key('ctrl_home',QC.Qt.Key.Key_Home,QC.Qt.KeyboardModifier.ControlModifier)
                    if edge in ('select_all_contract','prior_shift_select_all'):key('select_all',QC.Qt.Key.Key_A,QC.Qt.KeyboardModifier.ControlModifier)
                    if edge in ('delete_current_shift','delete_noncurrent_shift'):answer['yes']=True;eq._Delete();edge_snap('delete')
                    key('shift_down',QC.Qt.Key.Key_Down,QC.Qt.KeyboardModifier.ShiftModifier)
                edge_histories.append(dict(name=edge,steps=steps))
            clipboard=[]
            cases=['owned-program','owned-program one',' owned-program  one   two ','owned-program profile="My Profile" 日本😀','owned-program a\tb\nc',' \x1fowned-program \x1f日本😀\x1f ', '', '  ', 'owned-program '+' '.join('arg'+str(i) for i in range(30)), 'owned-program '+' '.join('x'*70+str(i) for i in range(30))]
            for value in cases:
                for accepted in [False,True]:
                    paste=Panel(c.gui,'before',['before']);panels.append(paste);raw['text']=value;answer['yes']=accepted;paste._Paste();paste._Copy()
                    clipboard.append(dict(raw=value,accepted=accepted,questions=list(questions),copies=list(copies),value=paste.GetValue(),raw_arguments=paste._executable_parameter_templates.GetData(),example=paste._example_full_command.text()));questions.clear();copies.clear()
            raw['text']=None;paste=Panel(c.gui,'before',['before']);panels.append(paste);paste._Paste();unavailable=dict(errors=list(errors),value=paste.GetValue());errors.clear()
            p.grab().save(str(HERE/'fixtures/external_command.png'))
            return dict(queue=events,queue_edges=edge_histories,clipboard=clipboard,unavailable=unavailable)
        finally:
            Q.GetYesNo=old_yes;Q.EnterText=old_text;c.GetClipboardText=old_clip;c.pub=old_pub;M.ShowCritical=old_error
            for p in panels:p.hide();p.deleteLater()
    return session.controller.CallBlockingToQt(session.controller.gui,drive)
def main():
    import hydrus_driver,record_api
    if len(sys.argv)>1 and sys.argv[1]=='--child':
        out=sys.argv[2];result=hydrus_driver.run_client(record_api.unpack_fixture('basic'),record);Path(out).write_text(json.dumps(result));return
    with tempfile.TemporaryDirectory() as folder:
        out=Path(folder)/'result.json';hydrus_driver.run_in_subprocess(str(Path(__file__).resolve()),'--child',str(out));result=json.loads(out.read_text())
    (HERE/'fixtures/external_command.json').write_text(json.dumps(result,indent=2,ensure_ascii=False)+'\n');print('wrote external_command.json')
if __name__=='__main__':main()

#!/usr/bin/env python3
"""Record Options namespace queue Add/Edit text questions and advanced tag views.

Runs real QueueListBox selection, reordering, delete confirmations, prompt
cleaning/escaped hyphens, cancellation and staged Apply/reopen. Dialog answers
are scripted; namespace editor and queue handlers run unchanged.
"""
import json
import sys
import tempfile
from pathlib import Path
HERE=Path(__file__).resolve().parent
sys.path.insert(0,str(HERE))


def record(session):
    from qtpy import QtWidgets as QW
    from hydrus.core import HydrusExceptions
    from hydrus.client.gui import ClientGUIDialogsQuick
    from hydrus.client.gui.panels.options.FileSortCollectPanel import FileSortCollectPanel
    controller,gui=session.controller,session.controller.gui
    old=(ClientGUIDialogsQuick.EnterText,ClientGUIDialogsQuick.SelectFromListButtons,ClientGUIDialogsQuick.GetYesNo)
    previous=controller.new_options.GetBoolean('advanced_mode')
    def drive():
        options=controller.new_options.Duplicate();panel=FileSortCollectPanel(gui,options)
        queue=panel._namespace_file_sort_by
        calls=[];texts=[];views=[];answers=[]
        def text(parent,message,default='',**kwargs):
            value=texts.pop(0);calls.append({'kind':'text','message':message,'default':default,'allow_blank':kwargs.get('allow_blank',True),'answer':value})
            if value is None:raise HydrusExceptions.CancelledException()
            return value
        def view(parent,title,choice_tuples=None,**kwargs):
            value=views.pop(0);calls.append({'kind':'view','title':title,'message':kwargs.get('message'),'choices':[list(c) for c in choice_tuples],'answer':value})
            if value is None:raise HydrusExceptions.CancelledException()
            return choice_tuples[value][1]
        def answer(parent,message,*args,**kwargs):
            accepted=answers.pop(0);calls.append({'kind':'question','message':message,'answer':accepted})
            return QW.QDialog.DialogCode.Accepted if accepted else QW.QDialog.DialogCode.Rejected
        ClientGUIDialogsQuick.EnterText=text;ClientGUIDialogsQuick.SelectFromListButtons=view;ClientGUIDialogsQuick.GetYesNo=answer
        def state():return {'data':[[list(n),v] for n,v in queue.GetData()], 'labels':[queue._listbox.item(i).text() for i in range(queue._listbox.count())],
                           'selected':[i for i in range(queue._listbox.count()) if queue._listbox.item(i).isSelected()]}
        initial=state();steps=[]
        def step(action,selected=None,edited=None,tag_view=None,advanced=False,accept=None):
            controller.new_options.SetBoolean('advanced_mode',advanced)
            if selected is not None:
                queue._listbox.clearSelection()
                for i in selected:queue._listbox.item(i).setSelected(True)
            if action in ('_Add','_Edit'):texts.append(edited)
            if action in ('_Add','_Edit') and advanced and edited is not None and edited.strip('- '):views.append(tag_view)
            if accept is not None:answers.append(accept)
            calls.clear();getattr(queue,action)()
            steps.append({'action':action,'selection':selected,'text':edited,'view':tag_view,'advanced':advanced,'accepted':accept,'calls':list(calls),'state':state()})
        step('_Add',edited=None)
        step('_Add',edited='---')
        step('_Add',edited=' CREATOR\\-ID - SERIES - PAGE ')
        step('_Add',edited='creator\\-id-series-page')
        step('_Edit',selected=[0],edited=None)
        step('_Edit',selected=[0],edited='TITLE-SERIES',advanced=True,tag_view=None)
        step('_Edit',selected=[0],edited='TITLE-SERIES',advanced=True,tag_view=1)
        step('_Add',edited='title-page',advanced=True,tag_view=2)
        step('_Edit',selected=[0,1],edited='character-title')
        step('_Up',selected=[queue._listbox.count()-1])
        step('_Down',selected=[0,1])
        step('_Delete',selected=[0,1],accept=False)
        step('_Delete',selected=[0,1],accept=True)
        isolated=options.GetDefaultNamespaceSorts()[0].sort_type[1] != queue.GetData()[0]
        panel.UpdateOptions()
        applied=[[list(sort.sort_type[1][0]),sort.sort_type[1][1]] for sort in options.GetDefaultNamespaceSorts()]
        reopened=FileSortCollectPanel(gui,options)
        reopened_data=[[list(n),v] for n,v in reopened._namespace_file_sort_by.GetData()]
        panel.deleteLater();reopened.deleteLater()
        return {'initial':initial,'steps':steps,'draft_isolated':isolated,'applied':applied,'reopened':reopened_data}
    try:return controller.CallBlockingToQt(gui,drive)
    finally:
        ClientGUIDialogsQuick.EnterText,ClientGUIDialogsQuick.SelectFromListButtons,ClientGUIDialogsQuick.GetYesNo=old
        controller.new_options.SetBoolean('advanced_mode',previous)


def main():
    import hydrus_driver
    import record_api
    if len(sys.argv)>1 and sys.argv[1]=='--child':
        destination=Path(sys.argv[2]);result=hydrus_driver.run_client(record_api.unpack_fixture('basic'),record);destination.write_text(json.dumps(result));return
    with tempfile.TemporaryDirectory() as work:
        destination=Path(work)/'namespace.json';hydrus_driver.run_in_subprocess(str(Path(__file__).resolve()),'--child',str(destination));result=json.loads(destination.read_text())
    (HERE/'fixtures/namespace_sorts.json').write_text(json.dumps(result,indent=2)+'\n')
if __name__=='__main__':main()

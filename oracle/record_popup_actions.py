#!/usr/bin/env python3
"""Actual popup clipboard, current callable and yes/no handlers on private jobs."""
import json,sys,tempfile
from pathlib import Path
HERE=Path(__file__).resolve().parent
sys.path.insert(0,str(HERE))

def record(session):
    def drive():
        from hydrus.client import ClientThreading
        from hydrus.client.gui import ClientGUIPopupMessages as P
        from hydrus.core import HydrusData
        from qtpy import QtWidgets as W
        c=session.controller
        published=[];called=[];states=[];panels=[]
        original=c.pub
        def publish(topic,*args,**kwargs):
            if topic=='clipboard':published.append(list(args))
            else:original(topic,*args,**kwargs)
        c.pub=publish
        def snapshot(label,p,j):
            states.append(dict(label=label,question_hidden=p._text_yes_no.isHidden(),question=p._text_yes_no.text(),yes_hidden=p._yes.isHidden(),no_hidden=p._no.isHidden(),clipboard_hidden=p._copy_to_clipboard_button.isHidden(),clipboard_label=p._copy_to_clipboard_button.text(),callable_hidden=p._user_callable_button.isHidden(),callable_label=p._user_callable_button.text(),paused=j.IsPaused(),done=j.IsDone(),dismissed=j.IsDismissed(),cancelled=j.IsCancelled(),answer=j.GetIfHasVariable('popup_yes_no_answer'),published=list(published),called=list(called)))
        try:
            j=ClientThreading.JobStatus(pausable=True,cancellable=True)
            p=P.PopupMessage(c.gui,j);panels.append(p);p.UpdateMessage();snapshot('absent',p,j)
            payload='Full clipboard\n'+('雪🙂'*700)
            j.SetVariable('popup_clipboard',('copy full payload',payload))
            j.SetVariable('popup_yes_no_question','Question\n'+('é'*1030))
            call=HydrusData.Call(lambda:called.append('first'));call.SetLabel('repeat command');j.SetUserCallable(call)
            p.UpdateMessage();snapshot('all actions',p,j)
            p._copy_to_clipboard_button.click();p._user_callable_button.click();p._user_callable_button.click();snapshot('copy full and callable twice',p,j)
            call=HydrusData.Call(lambda:called.append('replacement'));call.SetLabel('replacement command');j.SetUserCallable(call)
            j.SetVariable('popup_clipboard',('replacement clipboard','replacement text'))
            # Handlers read the current JobStatus, even before another UI update.
            p._copy_to_clipboard_button.click();p._user_callable_button.click();snapshot('current values before refresh',p,j)
            p.PausePlay();p.UpdateMessage();snapshot('paused only question hides',p,j)
            p.PausePlay();p.UpdateMessage();snapshot('resume refresh labels',p,j)
            p.Cancel();p.UpdateMessage();p._user_callable_button.click();snapshot('cancel retains callable',p,j)
            j.DeleteVariable('popup_clipboard');j.DeleteVariable('user_callable');j.DeleteVariable('popup_yes_no_question');p.UpdateMessage();snapshot('variables removed',p,j)
            for answer in (True,False):
                j=ClientThreading.JobStatus();j.SetVariable('popup_yes_no_question','Accept this?')
                p=P.PopupMessage(c.gui,j);panels.append(p);p.UpdateMessage();snapshot('answer before '+str(answer),p,j)
                (p._yes if answer else p._no).click();snapshot('answer after '+str(answer),p,j)
            return dict(payload=payload,states=states)
        finally:
            c.pub=original
            for p in panels:p.deleteLater()
            W.QApplication.processEvents()
    return session.controller.CallBlockingToQt(session.controller.gui,drive)

def main():
    import hydrus_driver,record_api
    if len(sys.argv)>1 and sys.argv[1]=='--child':
        Path(sys.argv[2]).write_text(json.dumps(hydrus_driver.run_client(record_api.unpack_fixture('basic'),record)))
        return
    with tempfile.TemporaryDirectory() as directory:
        path=Path(directory)/'result.json'
        hydrus_driver.run_in_subprocess(str(Path(__file__).resolve()),'--child',str(path))
        result=json.loads(path.read_text())
    (HERE/'fixtures/popup_actions.json').write_text(json.dumps(result,indent=2)+'\n')
    print('wrote popup_actions.json')
if __name__=='__main__':main()

#!/usr/bin/env python3
"""Record actual startup loading for blank, missing, last and named sessions.
Uses real _InitialiseSession with a fresh notebook, real controller/DB saves,
clean and bad-shutdown branches and synchronous initial scheduling. Records exact
recovery questions, scripted yes/no/close answers and one real fifteen-second
auto-yes dialog, plus loaded trees, selected pages and autosave scheduling.
"""
import json
import sys
import tempfile
from pathlib import Path
HERE=Path(__file__).resolve().parent
sys.path.insert(0,str(HERE))


def record(session):
    from hydrus.core import HydrusConstants as HC
    from hydrus.client import ClientConstants as CC,ClientLocation
    from hydrus.client.gui.pages import ClientGUIPages
    from hydrus.client.gui import ClientGUIDialogsQuick
    from qtpy import QtWidgets as QW
    controller,gui=session.controller,session.controller.gui
    old_question=ClientGUIDialogsQuick.GetYesNo
    old_root=gui._notebook;old_default=HC.options['default_gui_session'];old_bad=controller.LastShutdownWasBad;old_later=controller.CallLaterQtSafe
    context=ClientLocation.LocationContext.STATICCreateSimple(CC.LOCAL_FILE_SERVICE_KEY)
    def tree(notebook):
        return [{'name':p.GetName(),'children':tree(p)} if isinstance(p,ClientGUIPages.PagesNotebook) else {'name':p.GetName()} for p in notebook.GetPages()]
    def drive():
        named=ClientGUIPages.PagesNotebook(gui,'source')
        nested=named.NewPagesNotebook(name='saved notebook',give_it_a_blank_page=False)
        nested.NewPageQuery(context,page_name='inside one')
        named.NewPageQuery(context,page_name='saved outside')
        controller.SaveGUISession(named.GetCurrentGUISession('startup work',False,True))
        last=ClientGUIPages.PagesNotebook(gui,'last source')
        last.NewPageQuery(context,page_name='last inside')
        controller.SaveGUISession(last.GetCurrentGUISession(CC.LAST_SESSION_SESSION_NAME,False,True))
        def run(startup,bad=False,answer=None):
            calls=[];questions=[]
            fresh=ClientGUIPages.PagesNotebook(gui,'fresh startup')
            gui._notebook=fresh
            HC.options['default_gui_session']=startup
            def later(win,delay,label,callback,*args,**kwargs):
                if label=='load initial session':
                    calls.append({'delay':delay,'label':label});return callback(*args,**kwargs)
                if label=='auto save session':calls.append({'delay':delay,'label':label});return None
                return old_later(win,delay,label,callback,*args,**kwargs)
            def question(win,message,**kwargs):
                questions.append({'message':message,**kwargs})
                if answer=='timeout':return old_question(win,message,**kwargs)
                return QW.QDialog.DialogCode.Accepted if answer=='yes' else QW.QDialog.DialogCode.Rejected
            ClientGUIDialogsQuick.GetYesNo=question
            controller.CallLaterQtSafe=later;controller.LastShutdownWasBad=lambda:bad
            gui._InitialiseSession()
            shown=fresh.GetCurrentMediaPage()
            return {'startup':startup,'bad':bad,'answer':answer,'questions':questions,'tree':tree(fresh),'shown':None if shown is None else shown.GetName(),'calls':calls}
        rows=[run(startup) for startup in [None,'missing startup',CC.LAST_SESSION_SESSION_NAME,'startup work']]
        recovery=[run(startup,True,answer) for startup,answer in [(CC.LAST_SESSION_SESSION_NAME,'yes'),(CC.LAST_SESSION_SESSION_NAME,'no'),('startup work','yes'),('startup work','no'),('startup work','close'),(CC.LAST_SESSION_SESSION_NAME,'timeout'),(None,'no'),('missing startup','no')]]
        return {'steps':rows,'recovery_steps':recovery,'named_tree':tree(named),'last_tree':tree(last)}
    try:return controller.CallBlockingToQt(gui,drive)
    finally:
        gui._notebook=old_root;HC.options['default_gui_session']=old_default;controller.LastShutdownWasBad=old_bad;controller.CallLaterQtSafe=old_later;ClientGUIDialogsQuick.GetYesNo=old_question


def main():
    import hydrus_driver,record_api
    if len(sys.argv)>1 and sys.argv[1]=='--child':
        destination=Path(sys.argv[2])
        result=hydrus_driver.run_client(record_api.unpack_fixture('basic'),record)
        destination.write_text(json.dumps(result))
        return
    with tempfile.TemporaryDirectory() as work:
        out=Path(work)/'creation.json'
        hydrus_driver.run_in_subprocess(str(Path(__file__).resolve()),'--child',str(out))
        result=json.loads(out.read_text())
    (HERE/'fixtures/session_startup.json').write_text(json.dumps(result,indent=2)+'\n')
if __name__=='__main__':main()

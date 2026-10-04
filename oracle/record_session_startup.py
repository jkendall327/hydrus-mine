#!/usr/bin/env python3
"""Record actual startup loading for blank, missing, last and named sessions.
Uses real _InitialiseSession with a fresh notebook, real controller/DB saves,
clean-shutdown branch and synchronous initial scheduling only. Outputs include
saved/loaded trees, current media page and configured autosave scheduling.
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
    controller,gui=session.controller,session.controller.gui
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
        rows=[]
        for startup in [None,'missing startup',CC.LAST_SESSION_SESSION_NAME,'startup work']:
            calls=[]
            fresh=ClientGUIPages.PagesNotebook(gui,'fresh startup')
            gui._notebook=fresh
            HC.options['default_gui_session']=startup
            def later(win,delay,label,callback,*args,**kwargs):
                if label=='load initial session':
                    calls.append({'delay':delay,'label':label});return callback(*args,**kwargs)
                if label=='auto save session':calls.append({'delay':delay,'label':label});return None
                return old_later(win,delay,label,callback,*args,**kwargs)
            controller.CallLaterQtSafe=later;controller.LastShutdownWasBad=lambda:False
            gui._InitialiseSession()
            shown=fresh.GetCurrentMediaPage()
            rows.append({'startup':startup,'tree':tree(fresh),'shown':None if shown is None else shown.GetName(),'calls':calls})
        return {'steps':rows,'named_tree':tree(named),'last_tree':tree(last)}
    try:return controller.CallBlockingToQt(gui,drive)
    finally:
        gui._notebook=old_root;HC.options['default_gui_session']=old_default;controller.LastShutdownWasBad=old_bad;controller.CallLaterQtSafe=old_later


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

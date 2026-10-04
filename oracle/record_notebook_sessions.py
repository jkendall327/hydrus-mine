#!/usr/bin/env python3
"""Record actual tab notebook session menus, naming questions and append routing.

Saves a clicked nested notebook while a different sibling remains selected;
records overwrite denial/acceptance, reserved names, name cancellation, duplicate
names and question closure. The real save method writes the database synchronously
inside the recorder, retaining the actual session container and its children.
"""
import json
import sys
import tempfile
import time
from pathlib import Path
HERE=Path(__file__).resolve().parent
sys.path.insert(0,str(HERE))


def record(session):
    from qtpy import QtWidgets as QW
    from hydrus.client import ClientConstants as CC,ClientLocation
    from hydrus.client.gui import ClientGUICore,ClientGUIDialogsQuick,ClientGUIDialogsMessage
    from hydrus.client.gui.pages import ClientGUIPages
    from hydrus.core import HydrusExceptions,HydrusSerialisable
    from record_main_menu import tree
    controller,gui=session.controller,session.controller.gui
    context=ClientLocation.LocationContext.STATICCreateSimple(CC.LOCAL_FILE_SERVICE_KEY)
    old_text,old_yesno=ClientGUIDialogsQuick.EnterText,ClientGUIDialogsQuick.GetYesNo
    old_warning,old_thread=ClientGUIDialogsMessage.ShowWarning,controller.CallToThread
    core=ClientGUICore.core();old_popup=core.PopupMenu
    answers,asked,saved,menus=[],[],[],[]
    def text(win,message,default='',**kwargs):
        answer=answers.pop(0);asked.append({'kind':'text','message':message,'default':default,'answer':answer})
        if answer is None:raise HydrusExceptions.CancelledException()
        return answer
    def yesno(win,message,title='Are you sure?',yes_label='yes',no_label='no',check_for_cancelled=False,**kwargs):
        answer=answers.pop(0);asked.append({'kind':'yes/no','message':message,'title':title,'yes':yes_label,'no':no_label,'answer':answer})
        result=QW.QDialog.DialogCode.Accepted if answer is True else QW.QDialog.DialogCode.Rejected
        return (result,answer=='closed') if check_for_cancelled else result
    def containers(container):
        row={'name':container.GetName()}
        if hasattr(container,'GetPageContainers'):row['children']=[containers(p) for p in container.GetPageContainers()]
        return row
    def call_thread(fn,*args,**kwargs):
        if fn==controller.SaveGUISession:
            value=args[0];saved.append({'name':value.GetName(),'children':[containers(p) for p in value.GetTopNotebook().GetPageContainers()]})
            return fn(*args,**kwargs)
        return old_thread(fn,*args,**kwargs)
    ClientGUIDialogsQuick.EnterText=text;ClientGUIDialogsQuick.GetYesNo=yesno
    ClientGUIDialogsMessage.ShowWarning=lambda win,message,**kwargs:asked.append({'kind':'warning','message':message})
    controller.CallToThread=call_thread
    core.PopupMenu=lambda win,menu:menus.append(tree(menu))
    def build():
        notebook=ClientGUIPages.PagesNotebook(gui,'outer')
        source=notebook.NewPagesNotebook(name='source notebook',give_it_a_blank_page=False)
        source.NewPageQuery(context,page_name='inside one')
        nested=source.NewPagesNotebook(name='nested',give_it_a_blank_page=False)
        nested.NewPageQuery(context,page_name='inside two')
        notebook.NewPageQuery(context,page_name='outside');notebook.setCurrentIndex(1)
        return notebook,source
    def pages(node):
        return [{'name':p.GetName(),**({'children':pages(p)} if isinstance(p,ClientGUIPages.PagesNotebook) else {})} for p in node.GetPages()]
    def drive():
        old=ClientGUIPages.PagesNotebook(gui,'seed')
        old.NewPageQuery(context,page_name='old entry')
        controller.SaveGUISession(old.GetCurrentGUISession('existing work',False,True))
        notebook,source=build();menus.clear()
        notebook._ShowMenuForTabIndex(0);notebook._ShowMenuForTabIndex(1)
        menu_rows=list(menus)
        cases=[('existing work',[False]),('existing work',[True]),
            (None,[None]),(None,['last session','new work']),
            (None,['existing work',False,'other work']),
            (None,['existing work','closed']), (None,['existing work',True])]
        rows=[]
        for name,script in cases:
            notebook,source=build();answers[:]=script;asked.clear();saved.clear()
            gui.ProposeSaveGUISession(name=name,suggested_name='source notebook' if name is None else '',notebook=source)
            rows.append({'name':name,'answers':script,'asked':list(asked),'saved':list(saved),
                'open_tree':pages(notebook),'shown':notebook.GetCurrentMediaPage().GetName()})
        destination,unused=build()
        destination_source=destination.widget(0)
        destination_source.AppendGUISessionFreshest('existing work')
        appended={'tree':pages(destination),'shown':destination.GetCurrentMediaPage().GetName()}
        return {'existing_names':controller.Read('serialisable_names',HydrusSerialisable.SERIALISABLE_TYPE_GUI_SESSION_CONTAINER),
            'menus':menu_rows,'steps':rows,'appended':appended}
    try:
        result=controller.CallBlockingToQt(gui,drive);time.sleep(1);return result
    finally:
        ClientGUIDialogsQuick.EnterText=old_text;ClientGUIDialogsQuick.GetYesNo=old_yesno
        ClientGUIDialogsMessage.ShowWarning=old_warning;controller.CallToThread=old_thread;core.PopupMenu=old_popup


def main():
    import hydrus_driver,record_api
    if len(sys.argv)>1 and sys.argv[1]=='--child':
        destination=Path(sys.argv[2])
        result=hydrus_driver.run_client(record_api.unpack_fixture('basic'),record)
        destination.write_text(json.dumps(result));return
    with tempfile.TemporaryDirectory() as work:
        out=Path(work)/'sessions.json'
        hydrus_driver.run_in_subprocess(str(Path(__file__).resolve()),'--child',str(out))
        result=json.loads(out.read_text())
    (HERE/'fixtures/notebook_sessions.json').write_text(json.dumps(result,indent=2)+'\n')
if __name__=='__main__':main()

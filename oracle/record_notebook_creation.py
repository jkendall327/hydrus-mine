#!/usr/bin/env python3
"""Record chooser-created notebook defaults and optional post-creation naming.

Drives the actual _ChooseNewPage method with its chooser returning 'pages';
answers EnterText by cancellation or acceptance. Each created notebook keeps its
initial blank search page and survives cancelling its optional name prompt.
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
    from hydrus.client.gui.pages import ClientGUIPages,ClientGUINewPageChooser
    controller,gui=session.controller,session.controller.gui
    old_chooser=ClientGUINewPageChooser.DialogPageChooser
    old_text=ClientGUIDialogsQuick.EnterText
    default=controller.new_options.GetBoolean('rename_page_of_pages_on_pick_new')
    class Chosen:
        def __init__(self,*args):pass
        def __enter__(self):return self
        def __exit__(self,*args):pass
        def exec(self):return QW.QDialog.DialogCode.Accepted
        def GetValue(self):return ('pages',None)
    ClientGUINewPageChooser.DialogPageChooser=Chosen
    def drive():
        rows=[]
        for prompt,name in [(False,None),(True,None),(True,'created pages')]:
            asked=[]
            def text(window,message,default='',**kwargs):
                asked.append({'message':message,'default':default})
                if name is None:raise HydrusExceptions.CancelledException()
                return name
            ClientGUIDialogsQuick.EnterText=text
            controller.new_options.SetBoolean('rename_page_of_pages_on_pick_new',prompt)
            notebook=ClientGUIPages.PagesNotebook(gui,'chooser source')
            notebook._ChooseNewPage()
            created=notebook.widget(0)
            shown=notebook.GetCurrentMediaPage()
            rows.append({'prompt':prompt,'answer':name,'asked':asked,'name':created.GetName(),
                'children':[p.GetName() for p in created.GetPages()],'shown':shown.GetName()})
        return {'default_prompt':default,'steps':rows}
    try:return controller.CallBlockingToQt(gui,drive)
    finally:
        ClientGUINewPageChooser.DialogPageChooser=old_chooser
        ClientGUIDialogsQuick.EnterText=old_text
        controller.new_options.SetBoolean('rename_page_of_pages_on_pick_new',default)


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
    (HERE/'fixtures/notebook_creation.json').write_text(json.dumps(result,indent=2)+'\n')
if __name__=='__main__':main()

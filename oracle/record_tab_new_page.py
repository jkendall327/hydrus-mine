#!/usr/bin/env python3
"""Record actual notebook popup chooser insertion before a clicked tab, all four default
insertion positions and chooser cancellation. The real _ChooseNewPage receives a pages choice
while another sibling is selected; inputs include the actual initial order created under
the insertion preference, and outputs include order, selection and children.
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
    old_insertion=controller.new_options.GetInteger('default_new_page_goes')
    class Chosen:
        def __init__(self,*args):pass
        def __enter__(self):return self
        def __exit__(self,*args):pass
        def exec(self):return QW.QDialog.DialogCode.Accepted
        def GetValue(self):return ('pages',None)
    ClientGUINewPageChooser.DialogPageChooser=Chosen
    def drive():
        rows=[]
        controller.new_options.SetBoolean('rename_page_of_pages_on_pick_new',False)
        for mode,here,cancel,selected in [(3,False,False,2),(3,True,False,2),(3,True,True,2)]+[(mode,here,False,1) for mode in range(4) for here in [False,True]]:
            controller.new_options.SetInteger('default_new_page_goes',mode)
            notebook=ClientGUIPages.PagesNotebook(gui,'source')
            for name in ['first','second','third']:notebook.NewPagesNotebook(name=name,give_it_a_blank_page=False)
            notebook.setCurrentIndex(selected)
            initial_names=[p.GetName() for p in notebook.GetPages()]
            def accepted(self):return QW.QDialog.DialogCode.Rejected if cancel else QW.QDialog.DialogCode.Accepted
            Chosen.exec=accepted
            notebook._ChooseNewPage(1 if here else None)
            rows.append({'mode':mode,'initial_names':initial_names,'initial_selected':selected,'here':here,'cancel':cancel,'insertion_index':1 if here else None,
                'names':[p.GetName() for p in notebook.GetPages()],
                'selected':notebook.currentIndex(),
                'children':[[c.GetName() for c in p.GetPages()] for p in notebook.GetPages()]})
        return {'steps':rows}
    try:return controller.CallBlockingToQt(gui,drive)
    finally:
        ClientGUINewPageChooser.DialogPageChooser=old_chooser
        ClientGUIDialogsQuick.EnterText=old_text
        controller.new_options.SetBoolean('rename_page_of_pages_on_pick_new',default)
        controller.new_options.SetInteger('default_new_page_goes',old_insertion)


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
    (HERE/'fixtures/tab_new_page.json').write_text(json.dumps(result,indent=2)+'\n')
if __name__=='__main__':main()

#!/usr/bin/env python3
"""Record real GUI Pages close confirmation, focus switching and history limits.

Drives actual page and notebook AskIfAbleToClose and sidebar PageShown; intercepts
only yes/no answers and the focus request target. Rebuilds the real Pages History
Qt menu over a controlled 25-entry history, including spinbox clamping.
"""
import json
import sys
import tempfile
from pathlib import Path
HERE=Path(__file__).resolve().parent
sys.path.insert(0,str(HERE))


def record(session):
    from qtpy import QtWidgets as QW
    from hydrus.client import ClientConstants as CC, ClientLocation
    from hydrus.client.gui import ClientGUIDialogsQuick
    from hydrus.client.gui.pages import ClientGUIPages
    from hydrus.client.gui.panels.options.GUIPagesPanel import GUIPagesPanel
    controller,gui=session.controller,session.controller.gui
    previous=(controller.new_options.GetBoolean('confirm_all_page_closes'),controller.new_options.GetBoolean('set_search_focus_on_page_change'),controller.new_options.GetInteger('page_nav_history_max_entries'))
    old_yesno=ClientGUIDialogsQuick.GetYesNo
    old_history=gui.page_nav_history.GetHistory
    def drive():
        options=controller.new_options.Duplicate()
        panel=GUIPagesPanel(gui,options)
        controls={'confirm':panel._confirm_all_page_closes.isChecked(),'focus':panel._set_search_focus_on_page_change.isChecked(),
                  'history':panel.page_nav_history_max_entries.value(),'min':panel.page_nav_history_max_entries.minimum(),'max':panel.page_nav_history_max_entries.maximum()}
        context=ClientLocation.LocationContext.STATICCreateSimple(CC.LOCAL_FILE_SERVICE_KEY)
        notebook=ClientGUIPages.PagesNotebook(gui,'recording')
        leaf=notebook.NewPageQuery(context,page_name='search',select_page=False)
        empty=notebook.NewPagesNotebook(name='empty',give_it_a_blank_page=False)
        held=notebook.NewPagesNotebook(name='held',give_it_a_blank_page=False)
        nested=held.NewPagesNotebook(name='nested',give_it_a_blank_page=False)
        nested.NewPageQuery(context,page_name='nested search',select_page=False)
        held.NewPageQuery(context,page_name='second search',select_page=False)
        close=[]
        for enabled in [False,True]:
            controller.new_options.SetBoolean('confirm_all_page_closes',enabled)
            for kind,page in [('search',leaf),('empty',empty),('held',held)]:
                for for_session in [False,True]:
                    for accepted in [False,True]:
                        questions=[]
                        def answer(parent,message,*args,**kwargs):
                            questions.append(message)
                            return QW.QDialog.DialogCode.Accepted if accepted else QW.QDialog.DialogCode.Rejected
                        ClientGUIDialogsQuick.GetYesNo=answer
                        error=None
                        try:page.AskIfAbleToClose(for_session_close=for_session)
                        except Exception as exc:error=type(exc).__name__
                        close.append({'enabled':enabled,'kind':kind,'for_session':for_session,'accepted':accepted,'questions':questions,'error':error})
        focus=[]
        sidebar=leaf._sidebar_management_panel
        original_focus=sidebar.SetSearchFocus
        try:
            for enabled in [False,True,False]:
                calls=[]
                controller.new_options.SetBoolean('set_search_focus_on_page_change',enabled)
                sidebar.SetSearchFocus=lambda:calls.append('tag autocomplete')
                sidebar.PageShown()
                focus.append({'enabled':enabled,'calls':calls})
        finally:sidebar.SetSearchFocus=original_focus
        history=[]
        rows=[(bytes([i])*32,f'page {i:02d}') for i in range(1,26)]
        gui.page_nav_history.GetHistory=lambda:rows
        for value in [0,1,2,20,100,1001]:
            panel.page_nav_history_max_entries.setValue(value)
            panel._confirm_all_page_closes.setChecked(True)
            panel._set_search_focus_on_page_change.setChecked(True)
            panel.UpdateOptions()
            actual=options.GetInteger('page_nav_history_max_entries')
            controller.new_options.SetInteger('page_nav_history_max_entries',actual)
            gui._pages_history_dirty=True
            gui._UpdateMenuPagesHistoryIfDirty()
            actions=gui.page_nav_history_menu.actions()
            history.append({'input':value,'value':actual,'labels':[a.text() for a in actions if not a.isSeparator()],
                            'first_bold':actions[0].font().bold(),'applied_confirm':options.GetBoolean('confirm_all_page_closes'),
                            'applied_focus':options.GetBoolean('set_search_focus_on_page_change')})
        notebook.deleteLater();panel.deleteLater()
        return {'controls':controls,'close':close,'focus':focus,'history':history}
    try:return controller.CallBlockingToQt(gui,drive)
    finally:
        ClientGUIDialogsQuick.GetYesNo=old_yesno
        gui.page_nav_history.GetHistory=old_history
        controller.new_options.SetBoolean('confirm_all_page_closes',previous[0])
        controller.new_options.SetBoolean('set_search_focus_on_page_change',previous[1])
        controller.new_options.SetInteger('page_nav_history_max_entries',previous[2])
        gui._pages_history_dirty=True


def main():
    import hydrus_driver
    import record_api
    if len(sys.argv)>1 and sys.argv[1]=='--child':
        destination=Path(sys.argv[2]);result=hydrus_driver.run_client(record_api.unpack_fixture('basic'),record)
        destination.write_text(json.dumps(result));return
    with tempfile.TemporaryDirectory() as work:
        destination=Path(work)/'navigation.json'
        hydrus_driver.run_in_subprocess(str(Path(__file__).resolve()),'--child',str(destination));result=json.loads(destination.read_text())
    (HERE/'fixtures/page_navigation_options.json').write_text(json.dumps(result,indent=2)+'\n')
if __name__=='__main__':main()

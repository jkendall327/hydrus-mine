#!/usr/bin/env python3
"""Record actual tab context menus, bulk close questions and navigation.

Builds search-only notebook scenarios in the real client, captures PopupMenu
rather than displaying it, answers bulk close both ways, and retains per-notebook
closed indices. Nested notebooks count themselves plus their descendant pages.
"""
import json
import sys
import tempfile
from pathlib import Path
HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))


def record(session):
    from qtpy import QtWidgets as QW
    from hydrus.client import ClientConstants as CC, ClientLocation
    from hydrus.client.gui import ClientGUICore, ClientGUIDialogsQuick
    from hydrus.client.gui.pages import ClientGUIPages
    from hydrus.core import HydrusTime
    from record_main_menu import tree
    controller = session.controller
    gui = controller.gui
    context = ClientLocation.LocationContext.STATICCreateSimple(CC.LOCAL_FILE_SERVICE_KEY)
    captured, asked = [], []
    answer = [False]
    core = ClientGUICore.core()
    old_popup, old_yesno = core.PopupMenu, ClientGUIDialogsQuick.GetYesNo
    core.PopupMenu = lambda window, menu: captured.append(tree(menu))
    def yesno(window, message, **kwargs):
        asked.append(message)
        return QW.QDialog.DialogCode.Accepted if answer[0] else QW.QDialog.DialogCode.Rejected
    ClientGUIDialogsQuick.GetYesNo = yesno
    def build(nested=False):
        notebook = ClientGUIPages.PagesNotebook(gui, 'oracle notebook')
        if nested:
            notebook.NewPageQuery(context, page_name='a', forced_insertion_index=0)
            child = notebook.NewPagesNotebook(name='nested', forced_insertion_index=1, give_it_a_blank_page=False)
            for i in range(2): child.NewPageQuery(context, page_name=f'child {i}', forced_insertion_index=i)
            notebook.NewPageQuery(context, page_name='c', forced_insertion_index=2)
            notebook.NewPageQuery(context, page_name='d', forced_insertion_index=3)
        else:
            for i in range(4): notebook.NewPageQuery(context, page_name=f'page {i}', forced_insertion_index=i)
        notebook.setCurrentIndex(2)
        return notebook
    def pages(notebook):return [p.GetName() for p in notebook.GetPages()]
    def drive():
        menu_notebook = build()
        menus = []
        for selected in range(4):
            menu_notebook.setCurrentIndex(selected)
            for clicked in range(4):
                captured.clear()
                menu_notebook._ShowMenuForTabIndex(clicked)
                menus.append({'selected': selected, 'clicked': clicked, 'entries': captured[0]})
        closes = []
        for side, method, index in [('other', '_CloseOtherPages', 2), ('left', '_CloseLeftPages', 2), ('right', '_CloseRightPages', 1)]:
            for accept in [False, True]:
                notebook = build(nested=True)
                answer[0] = accept
                asked.clear()
                getattr(notebook, method)(index)
                closes.append({'side': side, 'index': index, 'accepted': accept, 'asked': list(asked),
                    'pages': pages(notebook), 'closed_indices': [i for i, key in notebook._closed_pages]})
        navigation = []
        for movement, method, delta in [('first','MoveSelectionEnd',-1), ('left','MoveSelection',-1), ('right','MoveSelection',1), ('last','MoveSelectionEnd',1)]:
            notebook = build()
            getattr(notebook, method)(delta)
            navigation.append({'movement': movement, 'selected': notebook.currentIndex()})
        return {'menus': menus, 'close': closes, 'navigation': navigation,
            'default_close_focus': controller.new_options.GetInteger('close_page_focus_goes'),
            'default_rename_sent': controller.new_options.GetBoolean('rename_page_of_pages_on_send')}
    try:return controller.CallBlockingToQt(gui, drive)
    finally:
        core.PopupMenu, ClientGUIDialogsQuick.GetYesNo = old_popup, old_yesno


def main():
    import hydrus_driver, record_api
    if len(sys.argv)>1 and sys.argv[1]=='--child':
        destination = Path(sys.argv[2])
        result = hydrus_driver.run_client(record_api.unpack_fixture('basic'), record)
        destination.write_text(json.dumps(result))
        return
    with tempfile.TemporaryDirectory() as work:
        out=Path(work)/'actions.json'
        hydrus_driver.run_in_subprocess(str(Path(__file__).resolve()),'--child',str(out))
        result=json.loads(out.read_text())
    (HERE/'fixtures'/'tab_actions.json').write_text(json.dumps(result,indent=2)+'\n')
if __name__=='__main__':main()

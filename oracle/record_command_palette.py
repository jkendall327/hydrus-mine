#!/usr/bin/env python3
"""Drive real Qt palette providers and preferences on a synthetic nested session.

Record initial option controls and bounds, each provider's actual rich-text
results, filtered history, menu threshold/action invocation, provider queue
movement/removal/add cancellation and actual page/favourite activation.
"""
import json, os, sys, tempfile
HERE=os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0,HERE)

def record(session):
    def qt():
        from qtpy import QtWidgets as QW, QtCore as QC, QtGui as QG
        from hydrus.client import ClientConstants as CC, ClientLocation
        from hydrus.client.gui import ClientGUILocatorSearchProviders as providers, ClientGUIDialogsQuick
        from hydrus.client.gui.panels.options.CommandPalettePanel import CommandPalettePanel
        from hydrus.client.search import ClientSearchFileSearchContext
        from hydrus.core import HydrusExceptions
        gui=session.controller.gui;options=session.controller.new_options
        panel=CommandPalettePanel(gui,options)
        bools=['show_page_of_pages','initially_show_all_pages','initially_show_history','initially_show_favourite_searches','fav_searches_open_new_page','show_main_menu','show_media_menu']
        limits=['limit_page_results','limit_history_results','limit_favourite_searches_results']
        initial={k:options.GetBoolean('command_palette_'+k) for k in bools}
        initial.update({k:options.GetNoneableInteger('command_palette_'+k) for k in limits})
        initial['threshold']=panel._command_palette_num_chars_for_results_threshold.value()
        initial['order']=panel._command_palette_provider_order.GetData()
        initial['threshold_bounds']=[panel._command_palette_num_chars_for_results_threshold.minimum(),panel._command_palette_num_chars_for_results_threshold.maximum()]
        initial['limit_bounds']=[panel._command_palette_limit_page_results._number_value.minimum(),panel._command_palette_limit_page_results._number_value.maximum()]
        notebook=gui.GetTopLevelNotebook()
        location=ClientLocation.LocationContext.STATICCreateSimple(CC.LOCAL_FILE_SERVICE_KEY)
        alpha=notebook.NewPageQuery(location,page_name='Palette Alpha',select_page=False)
        nested=notebook.NewPagesNotebook(name='Palette Nested',give_it_a_blank_page=False,select_page=False)
        beta=nested.NewPageQuery(location,page_name='Palette Beta',select_page=False)
        gamma=notebook.NewPageQuery(location,page_name='Palette Gamma',select_page=False)
        for p in (alpha,beta,gamma):gui.ShowPage(p.GetPageKey())
        QW.QApplication.processEvents()
        favourites=[('Palette Folder','Favourite Alpha',ClientSearchFileSearchContext.FileSearchContext(location_context=location),False,None,None),(None,'Favourite Beta',ClientSearchFileSearchContext.FileSearchContext(location_context=location),True,None,None)]
        old_favourites=session.controller.favourite_search_manager.GetFavouriteSearchRows()
        session.controller.favourite_search_manager.SetFavouriteSearchRows(favourites)
        menu=QW.QMenu('Palette Synthetic',gui);action=menu.addAction('Palette &Launch');calls=[];action.triggered.connect(lambda:calls.append('launch'))
        checked=menu.addAction('Palette checked');checked.setCheckable(True);checked.setChecked(True)
        disabled=menu.addAction('Palette disabled');disabled.setEnabled(False)
        gui._menubar.addMenu(menu)
        events=[]
        def query(provider, text):
            p=providers.GetSearchProvider(provider);rows=[]
            p.resultsAvailable.connect(lambda job,data:rows.extend(data));p.processQuery(text,None,99)
            return p,rows
        def snap(provider,text):
            p,rows=query(provider,text)
            return {'provider':provider,'query':text,'rows':[{'text':r.text,'toggled':r.toggled,'close':r.closeOnActivated} for r in rows]}
        options.SetBoolean('command_palette_initially_show_favourite_searches',True)
        options.SetBoolean('command_palette_show_main_menu',True)
        for code,text in ((3,'Palette'),(4,'Palette'),(5,'Favourite'),(5,'Palette Folder'),(1,'Palette'),(1,'pa'),(0,'2+3*4'),(0,'sqrt(16)'),(0,'pow(2,3)'),(0,'1/0')):
            events.append(snap(code,text))
        options.SetBoolean('command_palette_show_page_of_pages',True);events.append(snap(3,'Palette'))
        for code,key in ((3,'limit_page_results'),(4,'limit_history_results'),(5,'limit_favourite_searches_results')):
            options.SetNoneableInteger('command_palette_'+key,1)
            events.append(snap(code,'Palette' if code!=5 else 'Favourite'))
            options.SetNoneableInteger('command_palette_'+key,initial[key])
        options.SetInteger('command_palette_num_chars_for_results_threshold',4)
        events.extend(snap(code,text) for code,text in ((3,'Pal'),(3,'Pale'),(3,'   '),(1,'Pal'),(1,'Pale')))
        options.SetInteger('command_palette_num_chars_for_results_threshold',1)
        p,rows=query(3,'Palette Alpha');p.resultSelected(rows[0].id)
        selected={'page':notebook.GetCurrentMediaPage().GetName()}
        p,rows=query(1,'Palette Launch');p.resultSelected(rows[0].id);selected['menu_calls']=calls
        p,rows=query(5,'Favourite Alpha');before=notebook.count();p.resultSelected(rows[0].id)
        selected['favourite']={'name':notebook.GetCurrentMediaPage().GetName(),'new_pages':notebook.count()-before}
        queue=panel._command_palette_provider_order;queue_events=[];questions=[]
        queue._listbox.item(0).setSelected(True);queue._Down();queue_events.append(queue.GetData())
        old_yesno=ClientGUIDialogsQuick.GetYesNo;old_select=ClientGUIDialogsQuick.SelectFromListButtons
        try:
            def yesno(parent,text,*args,**kwargs):questions.append(text);return QW.QDialog.DialogCode.Accepted
            ClientGUIDialogsQuick.GetYesNo=yesno;queue._Delete();queue_events.append(queue.GetData())
            def cancel(parent,text,choices,*args,**kwargs):questions.append({'text':text,'choices':choices});raise HydrusExceptions.CancelledException()
            ClientGUIDialogsQuick.SelectFromListButtons=cancel;queue._Add();queue_events.append(queue.GetData())
            ClientGUIDialogsQuick.SelectFromListButtons=lambda parent,text,choices,*args,**kwargs:choices[0][1]
            queue._Add();queue_events.append(queue.GetData())
            panel.UpdateOptions()
            persisted={'order':options.GetIntegerList('command_palette_provider_order')}
        finally:
            ClientGUIDialogsQuick.GetYesNo=old_yesno;ClientGUIDialogsQuick.SelectFromListButtons=old_select
            session.controller.favourite_search_manager.SetFavouriteSearchRows(old_favourites)
            gui._menubar.removeAction(menu.menuAction());menu.deleteLater();panel.deleteLater()
        return {'initial':initial,'events':events,'selected':selected,'queue_events':queue_events,'questions':questions,'persisted':persisted}
    return session.controller.CallBlockingToQt(session.controller.gui,qt)

def main():
    import hydrus_driver,record_api
    if len(sys.argv)>1:
        output=sys.argv[2];result=hydrus_driver.run_client(record_api.unpack_fixture('basic'),record)
        with open(output,'w') as f:json.dump(result,f)
        return
    with tempfile.TemporaryDirectory() as directory:
        path=os.path.join(directory,'result.json');hydrus_driver.run_in_subprocess(os.path.abspath(__file__),'--child',path)
        with open(path) as f:result=json.load(f)
    output=os.path.join(HERE,'fixtures/command_palette.json')
    with open(output,'w') as f:json.dump(result,f,indent=2);f.write('\n')
    print('wrote '+output)
if __name__=='__main__':main()

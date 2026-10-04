#!/usr/bin/env python3
"""Record owned Options default/fallback sort cogs and their actual media keys.

Drives real Qt cog menus, service/advanced-view actions and staged UpdateOptions,
then reopens applied controls. Sorts plain files and series collections through
MediaList with independent sort contexts (even when search tag toggles are off).
"""
import json
import sys
import tempfile
from pathlib import Path
HERE=Path(__file__).resolve().parent
sys.path.insert(0,str(HERE))


def record(session):
    from qtpy import QtWidgets as QW
    from hydrus.core import HydrusConstants as HC
    from hydrus.client import ClientConstants as CC,ClientLocation
    from hydrus.client.gui.panels.options.FileSortCollectPanel import FileSortCollectPanel
    from hydrus.client.gui.widgets import ClientGUIMenuButton
    from hydrus.client.media import ClientMediaSort,ClientMediaList,ClientMediaCollect
    from hydrus.client.search import ClientSearchTagContext
    from record_main_menu import tree
    from record_media_collect import media_facts,sort_facts
    controller,gui=session.controller,session.controller.gui
    manifest=json.loads((HERE/'fixtures/legacy_db/basic.manifest.json').read_text())
    results=controller.Read('media_results',[bytes.fromhex(f['hash']) for f in manifest['files']])
    results=[m for m in results if CC.LOCAL_FILE_SERVICE_KEY in m.GetLocationsManager().GetCurrent()]
    location=ClientLocation.LocationContext.STATICCreateSimple(CC.LOCAL_FILE_SERVICE_KEY)
    def drive():
        options=controller.new_options.Duplicate();panel=FileSortCollectPanel(gui,options)
        services=controller.services_manager.GetServices([HC.LOCAL_TAG,HC.TAG_REPOSITORY,HC.COMBINED_TAG])
        def facts(sort):
            f=sort_facts(sort);tc=sort.tag_context
            f['tag_context']={'service':tc.service_key.hex(),'display_service':tc.display_service_key.hex(),
                              'include_current':tc.include_current_tags,'include_pending':tc.include_pending_tags}
            return f
        def menu(control):
            m=QW.QMenu(control)
            ClientGUIMenuButton.PopulateMenuFromTemplateItems(m,control._tag_gubbins_cog_icon_button._menu_template_items)
            return m
        states=[];cases=[]
        for role,control in [('default',panel._default_media_sort),('fallback',panel._fallback_media_sort)]:
            for kind,sort_type in [('filesize',('system',CC.SORT_FILES_BY_FILESIZE)),
                                   ('namespaces',('namespaces',(('series','creator','title','volume','chapter','page'),1))),
                                   ('num_tags',('system',CC.SORT_FILES_BY_NUM_TAGS))]:
                control.SetSort(ClientMediaSort.MediaSort(sort_type,CC.SORT_ASC))
                m=menu(control)
                states.append({'role':role,'kind':kind,'visible':not control._tag_gubbins_cog_icon_button.isHidden(),'menu':tree(m)})
                m.deleteLater()
                if kind=='filesize':continue
                for service in services:
                    # Preserve full metadata, while GetCurrentAndPending ignores
                    # the search-current/pending toggles for actual sort keys.
                    tc=ClientSearchTagContext.TagContext(include_current_tags=False,include_pending_tags=False)
                    control.SetSort(ClientMediaSort.MediaSort(sort_type,CC.SORT_ASC,tag_context=tc))
                    m=menu(control)
                    action=next(a for a in m.actions()[0].menu().actions() if a.text()==service.GetName())
                    action.trigger();m.deleteLater()
                    for view in ([1,3,2] if kind=='namespaces' else [1]):
                        if kind=='namespaces':
                            m=menu(control);m.actions()[1].menu().actions()[[1,3,2].index(view)].trigger();m.deleteLater()
                        checked_menu=menu(control);checked=tree(checked_menu);checked_menu.deleteLater()
                        target=control.GetSort()
                        for order in [CC.SORT_ASC,CC.SORT_DESC]:
                            target.sort_order=order
                            for collected in [False,True]:
                                primary=target if role=='default' else ClientMediaSort.MediaSort(('system',CC.SORT_FILES_BY_NUM_TAGS),CC.SORT_ASC)
                                fallback=target if role=='fallback' else options.GetFallbackSort()
                                media=ClientMediaList.MediaList(location,results)
                                media._singleton_media=dict.fromkeys(media._sorted_media)
                                if collected:media.Collect(ClientMediaCollect.MediaCollect(namespaces=['series']))
                                media.Sort(primary,fallback)
                                cases.append({'role':role,'kind':kind,'service_name':service.GetName(),'sort':facts(target),
                                              'primary':facts(primary),'fallback':facts(fallback),'collected':collected,
                                              'menu':checked,'media':[media_facts(m) for m in media._sorted_media]})
        before={'default':facts(options.GetDefaultSort()),'fallback':facts(options.GetFallbackSort())}
        draft={'default':facts(panel._default_media_sort.GetSort()),'fallback':facts(panel._fallback_media_sort.GetSort())}
        panel.UpdateOptions()
        applied={'default':facts(options.GetDefaultSort()),'fallback':facts(options.GetFallbackSort())}
        reopened=FileSortCollectPanel(gui,options)
        after={'default':facts(reopened._default_media_sort.GetSort()),'fallback':facts(reopened._fallback_media_sort.GetSort())}
        panel.deleteLater();reopened.deleteLater()
        return {'services':[{'name':s.GetName(),'key':s.GetServiceKey().hex(),'type':s.GetServiceType()} for s in services],
                'states':states,'cases':cases,'before':before,'draft':draft,'applied':applied,'reopened':after,
                'files':[m.GetHash().hex() for m in results],'location':CC.LOCAL_FILE_SERVICE_KEY.hex()}
    return controller.CallBlockingToQt(gui,drive)


def main():
    import hydrus_driver,record_api
    if len(sys.argv)>1 and sys.argv[1]=='--child':
        dest=Path(sys.argv[2]);dest.write_text(json.dumps(hydrus_driver.run_client(record_api.unpack_fixture('basic'),record)));return
    with tempfile.TemporaryDirectory() as work:
        dest=Path(work)/'sorts.json';hydrus_driver.run_in_subprocess(str(Path(__file__).resolve()),'--child',str(dest));result=json.loads(dest.read_text())
    (HERE/'fixtures/sort_cogs.json').write_text(json.dumps(result,indent=2)+'\n')
if __name__=='__main__':main()

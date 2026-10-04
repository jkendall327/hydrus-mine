#!/usr/bin/env python3
"""Record actual recursive notebook refresh dispatch and advanced page weights.

Search refresh resumes paused queries; importer refresh broadcasts its sort.
An uninitialized descendant is skipped and an outside sibling stays untouched.
Weights count each leaf's hashes independently and both importer seed logs, with
an advanced-only copyable comma-formatted menu label.
"""
import json
import sys
import tempfile
import time
from pathlib import Path
HERE=Path(__file__).resolve().parent
sys.path.insert(0,str(HERE))


def record(session):
    from hydrus.client import ClientConstants as CC,ClientLocation
    from hydrus.client.gui import ClientGUICore
    from hydrus.client.gui.pages import ClientGUIPages
    from hydrus.client.search import ClientSearchPredicate
    from hydrus.client.importing import ClientImportGallerySeeds
    from record_main_menu import tree
    controller,gui=session.controller,session.controller.gui
    qt=lambda f:controller.CallBlockingToQt(gui,f)
    context=ClientLocation.LocationContext.STATICCreateSimple(CC.LOCAL_FILE_SERVICE_KEY)
    default_sync=controller.new_options.GetBoolean('default_search_synchronised')
    advanced=controller.new_options.GetBoolean('advanced_mode')
    controller.new_options.SetBoolean('default_search_synchronised',False)
    core=ClientGUICore.core();old_popup=core.PopupMenu
    menus=[];popup_menus=[]
    def popup(win,menu):menus.append(tree(menu));popup_menus.append(menu)
    core.PopupMenu=popup
    def build():
        root=ClientGUIPages.PagesNotebook(gui,'refresh root')
        group=root.NewPagesNotebook(name='group',give_it_a_blank_page=False)
        everything=[ClientSearchPredicate.Predicate(ClientSearchPredicate.PREDICATE_TYPE_SYSTEM_EVERYTHING)]
        first=group.NewPageQuery(context,page_name='first',initial_predicates=everything)
        nested=group.NewPagesNotebook(name='nested',give_it_a_blank_page=False)
        second=nested.NewPageQuery(context,page_name='second',initial_predicates=everything)
        importer=group.NewPageImportURLs(page_name='importer')
        urls=importer.GetPageManager().GetVariable('urls_import')
        if not urls.IsPaused():urls.PausePlay()
        urls.PendURLs([f'https://files.example/{i}.jpg' for i in range(54)])
        urls.GetGallerySeedLog().AddGallerySeeds([ClientImportGallerySeeds.GallerySeed('https://gallery.example/page/1',can_generate_more_pages=True)])
        skipped=group.NewPageQuery(context,page_name='uninitialized',initial_predicates=everything)
        outside=root.NewPageQuery(context,page_name='outside',initial_predicates=everything)
        root.setCurrentIndex(1)
        return root,group,first,second,importer,skipped,outside
    try:
        root,group,first,second,importer,skipped,outside=qt(build)
        for _ in range(400):
            if qt(lambda:all(p._initialised for p in [first,second,importer,skipped,outside])):break
            time.sleep(.025)
        else:raise RuntimeError('reference pages did not initialize')
        events=[]
        def drive():
            for p in [first,second,importer,skipped,outside]:
                old=p._sidebar_management_panel.RefreshQuery
                def wrapped(old=old,p=p):events.append(p.GetName());return old()
                p._sidebar_management_panel.RefreshQuery=wrapped
            for p in [first,second,skipped,outside]:p._sidebar_management_panel._tag_autocomplete.SetSynchronised(False)
            skipped._initialised=False
            before=[p._sidebar_management_panel._tag_autocomplete.IsSynchronised() for p in [first,second,skipped,outside]]
            group.RefreshAllPages()
            after=[p._sidebar_management_panel._tag_autocomplete.IsSynchronised() for p in [first,second,skipped,outside]]
            skipped._initialised=True
            return {'events':list(events),'before_sync':before,'after_sync':after,'shown':root.GetCurrentMediaPage().GetName()}
        refreshed=qt(drive)
        for _ in range(400):
            if qt(lambda:len(first.GetHashes())>0 and len(second.GetHashes())>0):break
            time.sleep(.025)
        else:raise RuntimeError('refreshed queries did not return files')
        refreshed['counts']=qt(lambda:[len(p.GetHashes()) for p in [first,second,skipped,outside]])
        manifest=json.loads((HERE/'fixtures/legacy_db/basic.manifest.json').read_text())
        hashes=[bytes.fromhex(f['hash']) for f in manifest['files'][:3]]
        def weights():
            # Locked pages retain their supplied media; the same hash appears
            # in both children, counting once per child for notebook weight.
            weighted=ClientGUIPages.PagesNotebook(gui,'weighted')
            pair=weighted.NewPagesNotebook(name='weighted group',give_it_a_blank_page=False)
            pair.NewPageQuery(context,page_name='files a',initial_hashes=hashes[:2])
            pair.NewPageQuery(context,page_name='files b',initial_hashes=hashes[1:])
            weighted_importer=pair.NewPageImportURLs(page_name='weighted importer')
            weighted_urls=weighted_importer.GetPageManager().GetVariable('urls_import')
            if not weighted_urls.IsPaused():weighted_urls.PausePlay()
            weighted_urls.PendURLs([f'https://files.example/{i}.jpg' for i in range(54)])
            weighted_urls.GetGallerySeedLog().AddGallerySeeds([ClientImportGallerySeeds.GallerySeed('https://gallery.example/page/1',can_generate_more_pages=True)])
            outputs=[];copied=[];old_pub=controller.pub
            def capture(topic,*args,**kwargs):
                if topic=='clipboard':copied.append(list(args))
                else:return old_pub(topic,*args,**kwargs)
            controller.pub=capture
            for enabled in [False,True]:
                controller.new_options.SetBoolean('advanced_mode',enabled);menus.clear();copied.clear()
                weighted._ShowMenuForTabIndex(0)
                output={'advanced':enabled,'entries':menus[0]}
                if enabled:
                    popup_menus[-1].actions()[0].trigger()
                    output['clipboard']=copied
                outputs.append(output)
            controller.pub=old_pub
            return {'hashes':[h.hex() for h in hashes],'files_per_leaf':[2,2],'file_seeds':54,'gallery_seeds':1,
                'leaf_weights':[p.GetTotalWeight() for p in pair.GetPages()],'weight':pair.GetTotalWeight(),'menus':outputs}
        weighted=qt(weights)
        time.sleep(1)
        return {'refresh':refreshed,'weights':weighted}
    finally:
        controller.new_options.SetBoolean('default_search_synchronised',default_sync)
        controller.new_options.SetBoolean('advanced_mode',advanced);core.PopupMenu=old_popup


def main():
    import hydrus_driver,record_api
    if len(sys.argv)>1 and sys.argv[1]=='--child':
        destination=Path(sys.argv[2]);result=hydrus_driver.run_client(record_api.unpack_fixture('basic'),record)
        destination.write_text(json.dumps(result));return
    with tempfile.TemporaryDirectory() as work:
        out=Path(work)/'refresh.json';hydrus_driver.run_in_subprocess(str(Path(__file__).resolve()),'--child',str(out))
        result=json.loads(out.read_text())
    (HERE/'fixtures/tab_refresh.json').write_text(json.dumps(result,indent=2)+'\n')
if __name__=='__main__':main()

#!/usr/bin/env python3
"""Record frozen URL importer seed logs and independent backup copies.

The real client saves paused importer state, changes the source logs, saves again,
appends the older backup twice, and changes one appended importer. Neither later
source work nor another loaded copy can alter a historical session snapshot.
"""
import json
import sys
import tempfile
import time
from pathlib import Path
HERE=Path(__file__).resolve().parent
sys.path.insert(0,str(HERE))


def record(session):
    from hydrus.core import HydrusTime
    from hydrus.client import ClientConstants as CC
    from hydrus.client.gui.pages import ClientGUIPages
    from hydrus.client.importing import ClientImportGallerySeeds
    controller, gui = session.controller, session.controller.gui
    now=[1700000000000]
    old_now=HydrusTime.GetNowMS
    HydrusTime.GetNowMS=lambda:now[0]
    def facts(importer):
        return {'paused':importer.IsPaused(),
            'files':[{'url':s.file_seed_data,'status':s.status,'note':s.note} for s in importer.GetFileSeedCache().GetFileSeeds()],
            'gallery':[{'url':s.url,'status':s.status,'note':s.note} for s in importer.GetGallerySeedLog().GetGallerySeeds()]}
    def drive():
        source=ClientGUIPages.PagesNotebook(gui,'source')
        page=source.NewPageImportURLs(page_name='url history')
        importer=page.GetPageManager().GetVariable('urls_import')
        if not importer.IsPaused():importer.PausePlay()
        importer.PendURLs(['https://files.example/first.jpg','https://files.example/second.jpg'])
        seeds=importer.GetFileSeedCache().GetFileSeeds()
        seeds[0].SetStatus(CC.STATUS_ERROR,note='recorded failure')
        importer.GetFileSeedCache().NotifyFileSeedsUpdated([seeds[0]])
        gallery=ClientImportGallerySeeds.GallerySeed('https://gallery.example/page/1',can_generate_more_pages=True)
        gallery.SetStatus(CC.STATUS_ERROR,note='recorded gallery failure')
        importer.GetGallerySeedLog().AddGallerySeeds([gallery])
        initial=facts(importer)
        controller.SaveGUISession(source.GetCurrentGUISession('importer history',False,True))
        importer.PendURLs(['https://files.example/later.jpg'])
        seeds[0].SetStatus(CC.STATUS_SUCCESSFUL_BUT_REDUNDANT,note='later state')
        importer.GetFileSeedCache().NotifyFileSeedsUpdated([seeds[0]])
        now[0]+=10000
        controller.SaveGUISession(source.GetCurrentGUISession('importer history',False,True))
        changed=facts(importer)
        destination=ClientGUIPages.PagesNotebook(gui,'destination')
        destination.AppendGUISessionBackup('importer history',1700000000000)
        first=destination.widget(0).widget(0).GetPageManager().GetVariable('urls_import')
        destination.AppendGUISessionBackup('importer history',1700000000000)
        second=destination.widget(1).widget(0).GetPageManager().GetVariable('urls_import')
        before=facts(first)
        first.PendURLs(['https://files.example/copy-only.jpg'])
        return {'initial':initial,'source_changed':changed,'loaded_backup':before,
            'first_copy_changed':facts(first),'second_copy':facts(second)}
    try:
        result=controller.CallBlockingToQt(gui,drive)
        time.sleep(1) # let paused importers' queued Start callbacks run before shutdown
        return result
    finally:HydrusTime.GetNowMS=old_now


def main():
    import hydrus_driver,record_api
    if len(sys.argv)>1 and sys.argv[1]=='--child':
        destination=Path(sys.argv[2])
        result=hydrus_driver.run_client(record_api.unpack_fixture('basic'),record)
        destination.write_text(json.dumps(result));return
    with tempfile.TemporaryDirectory() as work:
        out=Path(work)/'importers.json'
        hydrus_driver.run_in_subprocess(str(Path(__file__).resolve()),'--child',str(out))
        result=json.loads(out.read_text())
    (HERE/'fixtures'/'session_importers.json').write_text(json.dumps(result,indent=2)+'\n')
if __name__=='__main__':main()

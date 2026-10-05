#!/usr/bin/env python3
"""Actual Qt four update controls and paused gallery/watcher status deadlines.

Private real importer objects and actual list widgets exercise row-count periods,
strict deadline boundaries, forced updates, live settings and zero-denominator
fallback. The clock and options are restored; network traffic remains paused in
this private fixture process. No reference sources or external servers are used.
"""
import json,sys,tempfile
from pathlib import Path
HERE=Path(__file__).resolve().parent
sys.path.insert(0,str(HERE))
KEYS=['gallery_page_status_update_time_minimum_ms','gallery_page_status_update_time_ratio_denominator','watcher_page_status_update_time_minimum_ms','watcher_page_status_update_time_ratio_denominator']
ATTRS=['_gallery_page_status_update_time_minimum','_gallery_page_status_update_time_ratio_denominator','_watcher_page_status_update_time_minimum','_watcher_page_status_update_time_ratio_denominator']

def record(session):
    from qtpy import QtWidgets as QW
    from hydrus.client.gui.panels.options.SpeedAndMemoryPanel import SpeedAndMemoryPanel
    from hydrus.client.networking import ClientNetworkingGUG
    from hydrus.core import HydrusTime,HydrusSerialisable
    c=session.controller;gui=c.gui;original=c.new_options;get_now=HydrusTime.GetNowFloat
    def drive():
        draft=original.Duplicate();c.new_options=draft
        draft.SetBoolean('pause_all_new_network_traffic',True)
        def raw():return [draft.GetInteger(key) for key in KEYS]
        def shown(panel):
            result=[]
            for i,a in enumerate(ATTRS):
                w=getattr(panel,a)
                result.append(dict(value=w.GetValue() if i%2==0 else w.value(),enabled=w.isEnabled(),minimum=w._min if i%2==0 else w.minimum(),maximum=None if i%2==0 else w.maximum(),seconds=[w._seconds.minimum(),w._seconds.maximum()] if i%2==0 else None,milliseconds=[w._milliseconds.minimum(),w._milliseconds.maximum()] if i%2==0 else None))
            return result
        panel=SpeedAndMemoryPanel(gui,draft)
        labels=[w.text() for w in panel.findChildren(QW.QLabel) if 'importer update time:' in w.text() or 'importer magic update time denominator:' in w.text()]
        defaults=dict(raw=raw(),controls=shown(panel))
        box=panel._gallery_page_status_update_time_minimum.parentWidget()
        while not hasattr(box,'_expand_button'):box=box.parentWidget()
        box._expand_button.click()
        panel.resize(1100,750);panel.show();QW.QApplication.processEvents()
        box.grab().save(str(HERE/'fixtures/downloader_update_times_reference.png'));panel.close()
        for i,a in enumerate(ATTRS):
            w=getattr(panel,a)
            w.SetValue(.5) if i%2==0 else w.setValue(7)
        cancelled=raw();panel.deleteLater()
        controls=[]
        for values in [[1000,30,1000,30],[0,0,119,100],[250,1,1250,99],[60060,100,59999,2147483647],[333,7,999,3]]:
            for key,value in zip(KEYS,values):draft.SetInteger(key,value)
            panel=SpeedAndMemoryPanel(gui,draft);display=shown(panel);panel.UpdateOptions();saved=raw()
            reopened=SpeedAndMemoryPanel(gui,HydrusSerialisable.CreateFromString(draft.DumpToString()))
            controls.append(dict(imported=values,displayed=display,saved=saved,reopened=shown(reopened)))
            panel.deleteLater();reopened.deleteLater()
        gui_original_options=original.GetBoolean('pause_all_new_network_traffic')
        original.SetBoolean('pause_all_new_network_traffic',True)
        try:
            gug=ClientNetworkingGUG.GalleryURLGenerator('private deadline fixture',url_template='https://booru.example/search/%tags%/1',replacement_phrase='%tags%',search_terms_separator='+',initial_search_text='tag',example_search_text='test')
            c.network_engine.domain_manager.SetGUGs([gug])
            gallery=gui._notebook.NewPageImportGallery().GetSidebar()
            multiple=gallery._multiple_gallery_import
            multiple.SetStartFileQueuesPaused(True);multiple.SetStartGalleryQueuesPaused(True);multiple.SetGUGKeyAndName((gug.GetGUGKey(),gug.GetName()))
            gallery._PendQueries([f'private-{i}' for i in range(12)])
            watchers=gui._notebook.NewPageImportMultipleWatcher().GetSidebar()
            watchers._AddURLs([f'https://boards.example/thread/{i}' for i in range(12)])
            for w in watchers._multiple_watcher_import.GetWatchers():
                if not w.CheckingPaused():w.PausePlayChecking()
                if not w.FilesPaused():w.PausePlayFiles()
            clock=[100.0];HydrusTime.GetNowFloat=lambda:clock[0]
            schedules=[]
            for kind,sidebar in [('gallery',gallery),('watcher',watchers)]:
                manager=sidebar._multiple_gallery_import if kind=='gallery' else sidebar._multiple_watcher_import
                imports=manager.GetGalleryImports() if kind=='gallery' else manager.GetWatchers()
                listctrl=sidebar._gallery_importers_listctrl if kind=='gallery' else sidebar._watchers_listctrl
                prefix='gallery' if kind=='gallery' else 'watcher'
                for minimum,denominator,count in [(1000,30,0),(1000,30,12),(250,3,12),(1500,3,3),(333,7,12),(5000,0,12),(250,-2,12)]:
                    draft.SetInteger(prefix+'_page_status_update_time_minimum_ms',minimum);draft.SetInteger(prefix+'_page_status_update_time_ratio_denominator',denominator)
                    listctrl.SetData(imports[:count])
                    if kind=='gallery':sidebar._last_time_imports_changed=manager.GetLastTimeImportsChanged()
                    else:sidebar._last_time_watchers_changed=manager.GetLastTimeWatchersChanged()
                    sidebar._next_update_time=100.0;events=[]
                    def step(action,at):
                        clock[0]=at;before=sidebar._next_update_time
                        sidebar._UpdateImportStatusNow() if action=='force' else sidebar._UpdateImportStatus()
                        events.append(dict(action=action,at=at,before=before,after=sidebar._next_update_time,rows=len(listctrl.GetData())))
                    step('tick',100.0);step('tick',100.25)
                    deadline=sidebar._next_update_time;step('tick',deadline);step('tick',deadline+.25)
                    # Saved changes do not invalidate the pending deadline.
                    draft.SetInteger(prefix+'_page_status_update_time_minimum_ms',2000);draft.SetInteger(prefix+'_page_status_update_time_ratio_denominator',99)
                    step('tick',sidebar._next_update_time);step('force',clock[0])
                    schedules.append(dict(kind=kind,minimum_ms=minimum,denominator=denominator,items=count,events=events))
            return dict(keys=KEYS,labels=labels,defaults=defaults,cancelled=cancelled,controls=controls,schedules=schedules,legacy_options=json.loads(draft.DumpToString()),limits=['Private actual list subsets represent different UI row counts; actual importer managers remain populated and paused','Deadline methods run on Qt thread under a restored synthetic clock; no downloader work or real network requested'])
        finally:original.SetBoolean('pause_all_new_network_traffic',gui_original_options)
    try:
        result=c.CallBlockingToQt(gui,drive)
        HydrusTime.GetNowFloat=get_now
        c.CallBlockingToQt(gui,lambda:None)  # finish queued page Start while schedulers are alive
        return result
    finally:c.new_options=original;HydrusTime.GetNowFloat=get_now

def main():
    import hydrus_driver,record_api
    if len(sys.argv)>1 and sys.argv[1]=='--child':
        import signal;signal.alarm(70)
        from hydrus.client.gui.canvas import ClientGUIMPV
        ClientGUIMPV.MPV_IS_AVAILABLE=False
        Path(sys.argv[2]).write_text(json.dumps(hydrus_driver.run_client(record_api.unpack_fixture('basic'),record)));return
    with tempfile.TemporaryDirectory() as directory:
        target=Path(directory)/'result.json';hydrus_driver.run_in_subprocess(str(Path(__file__).resolve()),'--child',str(target));data=json.loads(target.read_text())
    out=HERE/'fixtures/downloader_update_times.json';out.write_text(json.dumps(data,indent=2)+'\n');print(f'wrote {out}')
if __name__=='__main__':main()

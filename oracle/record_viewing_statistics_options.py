#!/usr/bin/env python3
"""Record actual viewing Options, context-menu lines and manager timing policy.

Use real imported local media with synthetic in-memory view rows. Drive the
actual panel ticks/choice and UpdateOptions; generate actual media-menu labels.
Replay 18 NoneableTimeDeltaWidget bounds/None/millisecond-conversion states,
480 actual manager timing/filter policy outputs, and real displayed Canvas
same-file/clear-media intervals with controlled time.
"""
import itertools,json,os,sys,tempfile
HERE=os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0,HERE)

def record(session):
    def qt():
        from qtpy import QtCore as QC, QtWidgets as QW
        from hydrus.client import ClientConstants as CC
        from hydrus.client.gui.panels.options.FileViewingStatisticsPanel import FileViewingStatisticsPanel
        from hydrus.client.gui.media import ClientGUIMediaMenus
        from hydrus.client.media import ClientMediaSingle,ClientMediaManagers
        from hydrus.core import HydrusTime
        controller=session.controller;options=controller.new_options
        panel=FileViewingStatisticsPanel(controller.gui)
        initial={'active':panel._file_viewing_statistics_active.isChecked(),
            'archive_delete':panel._file_viewing_statistics_active_on_archive_delete_filter.isChecked(),
            'duplicates':panel._file_viewing_statistics_active_on_dupe_filter.isChecked(),
            'menu_display':panel._file_viewing_stats_menu_display.GetValue(),
            'menu_choices':[panel._file_viewing_stats_menu_display.itemText(i) for i in range(panel._file_viewing_stats_menu_display.count())],
            'canvases':panel._file_viewing_stats_interesting_canvas_types.GetValue(),
            'canvas_labels':[panel._file_viewing_stats_interesting_canvas_types.item(i).text() for i in range(3)]}
        durations={}
        for key in ('media_min_time','media_max_time','preview_min_time','preview_max_time'):
            control=getattr(panel,'_file_viewing_statistics_'+key);widget=control._time_delta_widget
            units={name:[getattr(widget,'_'+name).minimum(),getattr(widget,'_'+name).maximum()] for name in ('hours','minutes','seconds','milliseconds') if getattr(widget,'_show_'+name)}
            durations[key]={'initial':control.GetValue(),'minimum':widget._min,'none_phrase':control._checkbox.text(),'units':units}
        duration_events=[]
        for key in ('media_min_time','media_max_time'):
            control=getattr(panel,'_file_viewing_statistics_'+key)
            for value in (None,0,.049,.05,.999,1,1.001,1.999,600.125):
                control.SetValue(value);panel.UpdateOptions()
                widget=control._time_delta_widget
                duration_events.append({'control':key,'requested':value,'value':control.GetValue(),
                    'fields':[getattr(widget,'_'+name).value() for name in ('hours','minutes','seconds','milliseconds') if getattr(widget,'_show_'+name)],
                    'persisted_ms':options.GetNoneableInteger('file_viewing_statistics_'+key+'_ms')})
        with open(os.path.join(HERE,'fixtures/legacy_db/basic.manifest.json')) as f:manifest=json.load(f)
        file_hash=next(bytes.fromhex(f['hash']) for f in manifest['files'] if f['name']=='jpeg_00.jpg')
        result=controller.Read('media_results',[file_hash])[0].Duplicate()
        now=1800000000
        result._file_viewing_stats_manager=ClientMediaManagers.FileViewingStatsManager(result.GetTimesManager(),[(0,(now-10)*1000,2,12000),(1,(now-20)*1000,3,9000),(4,(now-30)*1000,4,28000)])
        media=ClientMediaSingle.MediaSingle(result)
        old_now=HydrusTime.GetNow;HydrusTime.GetNow=lambda:now
        events=[]
        def tree(menu):
            return [{'label':action.text(),'children':tree(action.menu())} if action.menu() else {'label':action.text()} for action in menu.actions() if not action.isSeparator()]
        try:
            for checks in itertools.product((False,True),repeat=3):
                for style in (2,3):
                    for i,value in enumerate(checks):panel._file_viewing_stats_interesting_canvas_types.Check(i,value)
                    panel._file_viewing_stats_menu_display.SetValue(style);panel.UpdateOptions()
                    menu=QW.QMenu(controller.gui);ClientGUIMediaMenus.AddFileViewingStatsMenu(menu,[media])
                    events.append({'canvases':options.GetIntegerList('file_viewing_stats_interesting_canvas_types'),'style':options.GetInteger('file_viewing_stats_menu_display'),'menu':tree(menu)})
                    menu.deleteLater()
            # Real manager policy, using the actual imported still and video durations.
            timed_results=[result,controller.Read('media_results',[next(bytes.fromhex(f['hash']) for f in manifest['files'] if f['name']=='video_with_audio.mp4')])[0]]
            timing_events=[]
            for archive,duplicates in itertools.product((False,True),repeat=2):
                panel._file_viewing_statistics_active_on_archive_delete_filter.setChecked(archive)
                panel._file_viewing_statistics_active_on_dupe_filter.setChecked(duplicates)
                for minimum,maximum in ((None,None),(.05,1),(2,1),(2,600)):
                    panel._file_viewing_statistics_media_min_time.SetValue(minimum)
                    panel._file_viewing_statistics_media_max_time.SetValue(maximum)
                    panel.UpdateOptions()
                    for mr in timed_results:
                        for canvas_type in (0,2,3):
                            for elapsed in (0,49,50,2000,600001):
                                row=controller.file_viewing_stats_manager._GenerateViewsRow(mr,canvas_type,123000,elapsed)
                                timing_events.append({'archive_delete':archive,'duplicates':duplicates,
                                    'minimum_ms':options.GetNoneableInteger('file_viewing_statistics_media_min_time_ms'),
                                    'maximum_ms':options.GetNoneableInteger('file_viewing_statistics_media_max_time_ms'),
                                    'duration_ms':mr.GetDurationMS(),'canvas':canvas_type,'elapsed_ms':elapsed,'row':row})
            # Actual displayed canvas starts one interval, ignores same-media SetMedia,
            # then saves on ClearMedia. Isolate publication, keeping the real manager.
            from hydrus.client import ClientLocation
            from hydrus.client.gui.canvas import ClientGUICanvas,ClientGUICanvasFrame
            manager=controller.file_viewing_stats_manager
            old_pending=manager._pending_updates;manager._pending_updates={}
            old_pub=manager._PubSubRow;manager._PubSubRow=lambda *args:None
            old_ms=HydrusTime.GetNowMS;clock=[123000]
            HydrusTime.GetNowMS=lambda:clock[0]
            frame=ClientGUICanvasFrame.CanvasFrame(controller.gui)
            canvas=ClientGUICanvas.CanvasMediaListBrowser(frame,os.urandom(32),ClientLocation.LocationContext.STATICCreateSimple(CC.LOCAL_FILE_SERVICE_KEY),[controller.Read('media_results',[file_hash])[0]],file_hash)
            frame.SetCanvas(canvas);frame.showNormal();QW.QApplication.processEvents()
            canvas_events=[]
            try:
                for active in (False,True):
                    panel._file_viewing_statistics_active.setChecked(active)
                    panel._file_viewing_statistics_media_min_time.SetValue(None)
                    panel._file_viewing_statistics_media_max_time.SetValue(1)
                    panel.UpdateOptions();manager._pending_updates={}
                    clock[0]=123000;canvas.ClearMedia();manager._pending_updates={}
                    current=canvas._media_list.GetMediaByHashes({file_hash})[0]
                    canvas.SetMedia(current)
                    clock[0]=123050;canvas.SetMedia(current)
                    clock[0]=125000;canvas.ClearMedia()
                    canvas_events.append({'active':active,'rows':[[key[1],list(value)] for key,value in manager._pending_updates.items()]})
            finally:
                canvas.ClearMedia();frame.close()
                QW.QApplication.sendPostedEvents(None,QC.QEvent.Type.DeferredDelete);QW.QApplication.processEvents()
                HydrusTime.GetNowMS=old_ms;manager._PubSubRow=old_pub;manager._pending_updates=old_pending
        finally:
            HydrusTime.GetNow=old_now;panel.deleteLater()
        return {'initial':initial,'durations':durations,'duration_events':duration_events,'menu_events':events,'timing_events':timing_events,'canvas_events':canvas_events,'now':now,'file':file_hash.hex()}
    return session.controller.CallBlockingToQt(session.controller.gui,qt)

def main():
    import hydrus_driver,record_api
    if len(sys.argv)>1:
        output=sys.argv[2];value=hydrus_driver.run_client(record_api.unpack_fixture('basic'),record)
        with open(output,'w') as f:json.dump(value,f)
        return
    with tempfile.TemporaryDirectory() as directory:
        path=os.path.join(directory,'result.json');hydrus_driver.run_in_subprocess(os.path.abspath(__file__),'--child',path)
        with open(path) as f:value=json.load(f)
    output=os.path.join(HERE,'fixtures/viewing_statistics_options.json')
    with open(output,'w') as f:json.dump(value,f,indent=2);f.write('\n')
    print('wrote '+output)
if __name__=='__main__':main()

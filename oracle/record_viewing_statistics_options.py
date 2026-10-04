#!/usr/bin/env python3
"""Record actual viewing Options, context-menu lines and manager timing policy.

Use real imported local media with synthetic in-memory view rows. Drive the
actual panel ticks/choice and UpdateOptions; generate actual media-menu labels.
Read NoneableTimeDeltaWidget units/min/field bounds for subsequent timing controls.
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
        finally:
            HydrusTime.GetNow=old_now;panel.deleteLater()
        return {'initial':initial,'durations':durations,'menu_events':events,'now':now,'file':file_hash.hex()}
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

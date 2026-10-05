#!/usr/bin/env python3
"""Actual GUI Misc staging and shared timestamp/byte-format consumers.

Uses real GUIPanel controls, controller ReinitGlobalSettings, HydrusTime and the
client-patched HydrusData.ToHumanBytes. File/gallery row formatting is recorded
from actual Qt log panels over synthetic seeds. Time is held; no real-site data,
network request or media deletion is authored. A minimal owned actual Qt dialog
also probes radio Return routing; identical native-platform outcomes are evidence
of a boundary, not a completion claim for an unused flag.
"""
import json,sys,tempfile,os,time,datetime
from pathlib import Path
HERE=Path(__file__).resolve().parent
sys.path.insert(0,str(HERE))
NOW=1700000000

def record(session):
    def drive():
        from qtpy import QtCore as QC,QtWidgets as QW,QtTest as QT
        from hydrus.core import HydrusData,HydrusTime,HydrusSerialisable
        from hydrus.client.gui.panels.options.GUIPanel import GUIPanel
        from hydrus.client.gui.widgets.ClientGUICommon import EnterCatchingRadioButton
        from hydrus.client.gui.parsing import ClientGUIParsingTest
        from hydrus.client.gui.networking import ClientGUINetworkJobControl
        from hydrus.client.importing.options import FileFilteringImportOptions as FFO,ImportOptionsConstants as IOC
        from hydrus.client.gui.importing import ClientGUIFileSeedCache as FC,ClientGUIGallerySeedLog as GL
        from hydrus.client.importing import ClientImportFileSeeds as FS,ClientImportGallerySeeds as GS
        c=session.controller;original=c.new_options.Duplicate();old_now=HydrusTime.GetNow;panels=[]
        HydrusTime.GetNow=lambda:NOW
        def prefs(options):return dict(iso=options.GetBoolean('always_show_iso_time'),figures=options.GetInteger('human_bytes_sig_figs'))
        sizes=[0,512,1023,1024,1536,243200,188213746,1023999,1024000,1024001,2**40+2**37]
        times=[None,NOW,NOW-3,NOW-4,NOW-86400,NOW+4,NOW+86400,0,-1,9223372036854775807]
        fs=FS.FileSeed(FS.FILE_SEED_TYPE_URL,'https://format.invalid/file');fs.created=NOW-86400;fs.modified=NOW-90;fs.source_time=NOW-172800
        cache=FS.FileSeedCache();cache.AddFileSeeds([fs]);file_panel=FC.EditFileSeedCachePanel(c.gui,cache);panels.append(file_panel)
        gs=GS.GallerySeed('https://format.invalid/search');gs.created=NOW-86400;gs.modified=NOW+90
        log=GS.GallerySeedLog();log.AddGallerySeeds([gs]);gallery_panel=GL.EditGallerySeedLogPanel(c.gui,False,True,'search',log);panels.append(gallery_panel)
        events=[];radios=[]
        parsing_panel=ClientGUIParsingTest.TestPanel(c.gui,lambda:None);panels.append(parsing_panel)
        parsing_inputs=['<html>'+('x'*1523)+'</html>',json.dumps('x'*1534)]
        filtering=FFO.FileFilteringImportOptions();filtering.SetMinSize(1536);filtering.SetMaxSize(243200);filtering.SetMaxGifSize(188213746)
        class RecordedJob:
            def __init__(self,data):self.data=data
            def NoEngineYet(self):return False
            def GetStatus(self):return ('receiving\nsecond line',self.data['speed'],self.data['read'],self.data['total'])
            def HasError(self):return self.data['error']
            def IsDone(self):return self.data['done']
        def consumers():
            jobs=[]
            for read,total,speed,error,done in [(1536,243200,1536,False,False),(1536,1536,1536,False,True),(1536,None,0,False,False),(0,243200,1536,False,False),(1536,243200,1536,True,True)]:
                network_control=ClientGUINetworkJobControl.NetworkJobControl(c.gui);panels.append(network_control)
                data=dict(read=read,total=total,speed=speed,error=error,done=done)
                network_control._network_job=RecordedJob(data);network_control._Update()
                jobs.append(dict(input=data,left=network_control._left_text.text(),right=network_control._right_text.text(),can_cancel=network_control._cancel_button.isEnabled()))
            network_control._network_job=None
            previews=[]
            for text in parsing_inputs:
                parsing_panel._SetExampleData(text)
                previews.append(dict(input=text,label=parsing_panel._example_data_raw_description.text(),raw=parsing_panel._example_data_raw))
            return dict(network_jobs=jobs,filtering_summary=filtering.GetSummary(IOC.IMPORT_OPTIONS_CALLER_TYPE_LOCAL_IMPORT),parser_previews=previews,expiry=[dict(timestamp=t,text=HydrusTime.TimestampToPrettyExpires(t)) for t in (NOW-4,NOW,NOW+4)],variants=[dict(timestamp=t,minutes=HydrusTime.TimestampToPrettyTimeDelta(t,show_seconds=False),exact=HydrusTime.TimestampToPrettyTimeDelta(t,just_now_threshold=0),no_prefix=HydrusTime.TimestampToPrettyTimeDelta(t,no_prefix=True),forced_relative=HydrusTime.TimestampToPrettyTimeDelta(t,force_no_iso=True)) for t in (NOW-86400,NOW-4,NOW,NOW+4,NOW+86400)])
        try:
            defaults=prefs(c.new_options)
            for iso,figures in [(False,3),(True,1),(True,2),(False,4),(True,5),(False,6),(True,0),(False,7)]:
                p=GUIPanel(c.gui);panels.append(p);before=prefs(c.new_options)
                p._always_show_iso_time.setChecked(iso);p._human_bytes_sig_figs.setValue(figures)
                draft=dict(iso=p._always_show_iso_time.isChecked(),figures=p._human_bytes_sig_figs.value());staged=prefs(c.new_options)
                p.UpdateOptions();c.ReinitGlobalSettings()
                encoded=HydrusSerialisable.CreateFromSerialisableTuple(c.new_options.GetSerialisableTuple());reopened=GUIPanel(c.gui);panels.append(reopened)
                events.append(dict(consumers=consumers(),input=[iso,figures],before=before,draft=draft,staged=staged,saved=prefs(c.new_options),round_trip=prefs(encoded),reopened=dict(iso=reopened._always_show_iso_time.isChecked(),figures=reopened._human_bytes_sig_figs.value()),bytes=[dict(size=size,text=HydrusData.ToHumanBytes(size)) for size in sizes],timestamps=[dict(timestamp=t,text=HydrusTime.TimestampToPrettyTimeDelta(t)) for t in times],file_row=file_panel._ConvertFileSeedToDisplayTuple(fs),gallery_row=gallery_panel._ConvertGallerySeedToDisplayTuple(gs)))
            p=GUIPanel(c.gui);before=prefs(c.new_options);p._always_show_iso_time.setChecked(not before['iso']);p._human_bytes_sig_figs.setValue(2);p.deleteLater();cancel_after=prefs(c.new_options)
            for force in (False,True):
                c.new_options.SetBoolean('force_enter_on_radio_buttons_to_do_dialog_ok',force)
                dialog=QW.QDialog(c.gui);layout=QW.QVBoxLayout(dialog);radio=EnterCatchingRadioButton('synthetic radio',dialog);radio.setChecked(True);layout.addWidget(radio)
                button=QW.QPushButton('ok',dialog);button.setDefault(True);button.clicked.connect(dialog.accept);layout.addWidget(button)
                accepted=[];dialog.accepted.connect(lambda:accepted.append(True));dialog.show();radio.setFocus();QT.QTest.qWait(20);QT.QTest.keyClick(radio,QC.Qt.Key.Key_Return);QT.QTest.qWait(20)
                radios.append(dict(force=force,accepted=len(accepted),checked=radio.isChecked(),visible=dialog.isVisible()));dialog.close();dialog.deleteLater()
            c.new_options.SetBoolean('always_show_iso_time',True);c.ReinitGlobalSettings()
            timezone_states=[];original_tz=os.environ.get('TZ')
            try:
                for zone in ('UTC','America/New_York','Europe/Berlin'):
                    os.environ['TZ']=zone;time.tzset()
                    offset=int(datetime.datetime.now().astimezone().utcoffset().total_seconds())
                    samples=[None,1672574400,1688212800,-1,9223372036854775807]
                    if zone=='UTC':samples.append(-62135596800)
                    timezone_states.append(dict(zone=zone,current_offset=offset,samples=[dict(timestamp=t,text=HydrusTime.TimestampToPrettyTimeDelta(t)) for t in samples]))
            finally:
                if original_tz is None:os.environ.pop('TZ',None)
                else:os.environ['TZ']=original_tz
                time.tzset()
            return dict(timezone_states=timezone_states,now=NOW,defaults=defaults,seeds=dict(file=dict(data=fs.file_seed_data,created=fs.created,modified=fs.modified,source_time=fs.source_time),gallery=dict(url=gs.url,created=gs.created,modified=gs.modified)),events=events,cancel_before=before,cancel_after=cancel_after,radio_return=radios)
        finally:
            c.new_options=original;c.ReinitGlobalSettings();HydrusTime.GetNow=old_now
            for p in panels:p.deleteLater()
    return session.controller.CallBlockingToQt(session.controller.gui,drive)
def main():
    import hydrus_driver,record_api
    if len(sys.argv)>1 and sys.argv[1]=='--child':Path(sys.argv[2]).write_text(json.dumps(hydrus_driver.run_client(record_api.unpack_fixture('basic'),record)));return
    with tempfile.TemporaryDirectory() as d:
        out=Path(d)/'result.json';hydrus_driver.run_in_subprocess(str(Path(__file__).resolve()),'--child',str(out));result=json.loads(out.read_text())
    (HERE/'fixtures/gui_format.json').write_text(json.dumps(result,indent=2)+'\n');print('wrote gui_format.json')
if __name__=='__main__':main()

#!/usr/bin/env python3
"""Actual Qt idle timeout controls, persisted seconds and controller gates.

Drives the real MaintenanceAndProcessingPanel NoneableSpinCtrls and UpdateOptions,
then CurrentlyIdle with controlled clock/boot/activity timestamps. No maintenance
jobs or force-idle behavior is claimed; reference data remains a private fixture.
"""
import json,sys,tempfile
from pathlib import Path
HERE=Path(__file__).resolve().parent
sys.path.insert(0,str(HERE))
def record(session):
    def qt():
        from hydrus.core import HydrusConstants as HC,HydrusGlobals as HG,HydrusTime
        from hydrus.client.gui.panels.options import MaintenanceAndProcessingPanel
        c=session.controller;options=c.new_options
        old={key:HC.options[key] for key in ('idle_period','idle_mouse_period','idle_normal')};old_api=options.GetNoneableInteger('idle_mode_client_api_timeout')
        old_now=HydrusTime.GetNowMS;old_boot=c.GetBootTimestampMS;old_get=c.GetTimestampMS;old_force=HG.force_idle_mode;old_previous=c._previously_idle;old_started=c._idle_started
        panel=MaintenanceAndProcessingPanel.MaintenanceAndProcessingPanel(c.gui)
        controls=[panel._idle_period,panel._idle_mouse_period,panel._idle_mode_client_api_timeout]
        def snapshot():
            return [dict(number=w._number_value.value(),minimum=w._number_value.minimum(),maximum=w._number_value.maximum(),none=w._checkbox.isChecked(),phrase=w._checkbox.text(),unit=w._unit,enabled=w.isEnabled(),number_enabled=w._number_value.isEnabled(),seconds=w.GetValue()) for w in controls]
        def saved():return [HC.options['idle_period'],HC.options['idle_mouse_period'],options.GetNoneableInteger('idle_mode_client_api_timeout')]
        initial=snapshot();edits=[];gates=[];now=[0];times={key:0 for key in ('last_user_action','last_mouse_action','last_client_api_action')}
        try:
            for value in [0,59,60,119,120,60000,60060,None]:
                for w in controls:w.SetValue(value)
                panel.UpdateOptions();edits.append(dict(action='set_seconds',input=value,controls=snapshot(),saved=saved()))
            for value in [-10,1001,2]:
                for w in controls:w._checkbox.setChecked(False);w._number_value.setValue(value)
                panel.UpdateOptions();edits.append(dict(action='edit_minutes',input=value,controls=snapshot(),saved=saved()))
            for none in (True,False):
                for w in controls:w._checkbox.setChecked(none)
                panel.UpdateOptions();edits.append(dict(action='none',input=none,controls=snapshot(),saved=saved()))
            panel._idle_normal.setChecked(False);panel._EnableDisableIdleNormal();disabled=snapshot()
            panel._idle_normal.setChecked(True);panel._EnableDisableIdleNormal()
            HydrusTime.GetNowMS=lambda:now[0];c.GetBootTimestampMS=lambda:0;c.GetTimestampMS=lambda key:times[key] if key in times else old_get(key);HG.force_idle_mode=False
            cases=[([None,None,None],[0,0,0],[120000,120001]),([60,None,None],[120001,999999999,999999999],[180001,180002]),([None,60,None],[999999999,120001,999999999],[180001,180002]),([None,None,60],[999999999,999999999,120001],[180001,180002]),([60000,None,None],[0,999999999,999999999],[60000000,60000001]),([None,None,60000],[999999999,999999999,0],[60000000,60000001])]
            for values,activity,checks in cases:
                for w,value in zip(controls,values):w.SetValue(value)
                panel.UpdateOptions();times.update(zip(times,activity))
                for at in checks:
                    now[0]=at;gates.append(dict(values=saved(),times=dict(times),now=at,enabled=HC.options['idle_normal'],eligible=c.CurrentlyIdle()))
            panel._idle_normal.setChecked(False);panel.UpdateOptions();now[0]=60000001
            gates.append(dict(values=saved(),times=dict(times),now=now[0],enabled=False,eligible=c.CurrentlyIdle()))
            # A fresh None preference retains the constructor's clamped 1-minute
            # number, rather than the source's nominal 30/10/5 arguments.
            HC.options['idle_period']=None;HC.options['idle_mouse_period']=None;options.SetNoneableInteger('idle_mode_client_api_timeout',None)
            reopened=MaintenanceAndProcessingPanel.MaintenanceAndProcessingPanel(c.gui)
            reopening=[dict(number=w._number_value.value(),none=w._checkbox.isChecked(),seconds=w.GetValue()) for w in [reopened._idle_period,reopened._idle_mouse_period,reopened._idle_mode_client_api_timeout]];reopened.deleteLater()
            return dict(initial=initial,edits=edits,disabled=disabled,gates=gates,reopened_none=reopening)
        finally:
            HC.options.update(old);options.SetNoneableInteger('idle_mode_client_api_timeout',old_api);HydrusTime.GetNowMS=old_now;c.GetBootTimestampMS=old_boot;c.GetTimestampMS=old_get;HG.force_idle_mode=old_force;c._previously_idle=old_previous;c._idle_started=old_started;panel.deleteLater()
    return session.controller.CallBlockingToQt(session.controller.gui,qt)
def main():
    import hydrus_driver
    if len(sys.argv)>1:
        import record_api
        Path(sys.argv[2]).write_text(json.dumps(hydrus_driver.run_client(record_api.unpack_fixture('basic'),record)));return
    with tempfile.TemporaryDirectory() as folder:
        out=Path(folder)/'result.json';hydrus_driver.run_in_subprocess(str(Path(__file__).resolve()),'--child',str(out));result=json.loads(out.read_text())
    (HERE/'fixtures/idle_timeout_options.json').write_text(json.dumps(result,indent=2)+'\n');print('wrote idle_timeout_options.json')
if __name__=='__main__':main()

#!/usr/bin/env python3
"""Record real session autosave retry, cadence and unchanged-save suppression.

Drives real AutoSaveLastSession with frozen time and actual controller/DB saves.
Records active idle-only retry, configured rescheduling, changed versus identical
last-session saves and stopping after a non-last startup selection.
"""
import json
import os
import sys
import tempfile
import time
from pathlib import Path

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))


def record(session):
    from hydrus.core import HydrusConstants as HC,HydrusTime,HydrusSerialisable
    from hydrus.client import ClientConstants as CC
    controller,gui=session.controller,session.controller.gui
    old_call=controller.CallLaterQtSafe;old_thread=controller.CallToThread;old_idle=controller.CurrentlyIdle
    old_default=HC.options['default_gui_session'];old_period=controller.new_options.GetInteger('last_session_save_period_minutes');old_only=controller.new_options.GetBoolean('only_save_last_session_during_idle')
    old_now=HydrusTime.GetNowMS;old_hash=controller._last_last_session_hash
    now=[1700100000000];idle=[True];calls=[];saves=[]
    HydrusTime.GetNowMS=lambda:now[0]
    controller.CurrentlyIdle=lambda:idle[0]
    controller.CallLaterQtSafe=lambda win,delay,label,callback,*args:calls.append({'delay':delay,'label':label})
    old_save=controller.SaveGUISession
    def save(session):
        prior=controller._last_last_session_hash;old_save(session)
        saves.append({'name':session.GetName(),'changed':controller._last_last_session_hash!=prior})
    controller.SaveGUISession=save
    def thread(callback,*args,**kwargs):
        if 'SaveGUISession' in callback.__code__.co_names:return callback(*args,**kwargs)
        return old_thread(callback,*args,**kwargs)
    controller.CallToThread=thread
    for _ in range(400):
        if controller.CallBlockingToQt(gui,lambda:gui._notebook.widget(0)._initialised):break
        time.sleep(.025)
    else:raise RuntimeError('initial query not initialized')
    def drive():
        rows=[]
        initial=gui._notebook.GetCurrentGUISession(CC.LAST_SESSION_SESSION_NAME,False,True)
        controller.WriteSynchronous('delete_serialisable_named',HydrusSerialisable.SERIALISABLE_TYPE_GUI_SESSION_CONTAINER,CC.LAST_SESSION_SESSION_NAME)
        controller.WriteSynchronous('serialisable',initial,force_timestamp_ms=now[0])
        controller._last_last_session_hash=None
        for startup,only,is_idle,minutes,rename in [(CC.LAST_SESSION_SESSION_NAME,True,False,5,None),(CC.LAST_SESSION_SESSION_NAME,True,True,5,None),(CC.LAST_SESSION_SESSION_NAME,False,True,1,None),(CC.LAST_SESSION_SESSION_NAME,False,True,2,'changed autosave'),('other startup',True,False,7,None),('other startup',True,True,7,None)]:
            HC.options['default_gui_session']=startup;controller.new_options.SetBoolean('only_save_last_session_during_idle',only);controller.new_options.SetInteger('last_session_save_period_minutes',minutes)
            idle[0]=is_idle;calls.clear();saves.clear();now[0]+=60000
            if rename:gui._notebook.NewPagesNotebook(name=rename,give_it_a_blank_page=False)
            gui.AutoSaveLastSession()
            backups=controller.Read('serialisable_names_to_backup_timestamps_ms',HydrusSerialisable.SERIALISABLE_TYPE_GUI_SESSION_CONTAINER)
            rows.append({'startup':startup,'only_idle':only,'idle':is_idle,'minutes':minutes,'add_notebook':rename,'now':now[0],'calls':list(calls),'saves':list(saves),'backups':backups.get(CC.LAST_SESSION_SESSION_NAME,[])})
        return {'steps':rows,'default_minutes':old_period,'default_only_idle':old_only}
    try:return controller.CallBlockingToQt(gui,drive)
    finally:
        controller.CallLaterQtSafe=old_call;controller.CallToThread=old_thread;controller.CurrentlyIdle=old_idle;controller.SaveGUISession=old_save;controller._last_last_session_hash=old_hash
        HC.options['default_gui_session']=old_default;controller.new_options.SetInteger('last_session_save_period_minutes',old_period);controller.new_options.SetBoolean('only_save_last_session_during_idle',old_only);HydrusTime.GetNowMS=old_now


def main():
    import hydrus_driver
    import record_api
    if len(sys.argv) > 1 and sys.argv[1] == '--child':
        destination = Path(sys.argv[2])
        result = hydrus_driver.run_client(record_api.unpack_fixture('basic'), record)
        destination.write_text(json.dumps(result))
        return
    with tempfile.TemporaryDirectory() as work:
        out = Path(work)/'backups.json'
        hydrus_driver.run_in_subprocess(str(Path(__file__).resolve()), '--child', str(out))
        result = json.loads(out.read_text())
    (HERE/'fixtures'/'session_autosave.json').write_text(json.dumps(result, indent=2)+'\n')

if __name__ == '__main__': main()

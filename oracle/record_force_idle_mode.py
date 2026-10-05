#!/usr/bin/env python3
"""Real debug QAction and controller idle/busy overrides with real worker entries.

Owned copied function globals hold clocks/force flags and database transport for
controller/worker probes; shared reference clocks and reference media are untouched.
The live QAction toggles the genuine fixture flag; only its wake publication is
captured to prevent background work escaping this bounded recorder.
"""
import json,sys,tempfile,types
from pathlib import Path
HERE=Path(__file__).resolve().parent
sys.path.insert(0,str(HERE))
def record(session):
    def qt():
        from qtpy import QtWidgets as QW
        from hydrus.client import ClientController,ClientDaemons
        from hydrus.client.files import ClientFilesManager
        from hydrus.core import HydrusGlobals as HG,HydrusConstants as HC,HydrusTime
        c=session.controller;gui=c.gui
        old_force=HG.force_idle_mode;old_pub=c.pub;menus=[];published=[]
        def find(menu,labels):
            for action in menu.actions():
                if action.text()==labels[0]:
                    return action if len(labels)==1 else find(action.menu(),labels[1:])
            raise ValueError(labels)
        def pub(topic,*args,**kwargs):
            if topic=='wake_idle_workers':published.append(topic);return
            old_pub(topic,*args,**kwargs)
        try:
            HG.force_idle_mode=False;c.pub=pub
            out={'initial':False,'toggles':[]}
            for _ in range(4):
                menu,label=gui._InitialiseMenuInfoHelp();menus.append(menu)
                action=find(menu,['debug','debug modes','force idle mode'])
                before=HG.force_idle_mode;checked=action.isChecked();published.clear()
                action.trigger()
                fresh,_=gui._InitialiseMenuInfoHelp();menus.append(fresh)
                fresh_action=find(fresh,['debug','debug modes','force idle mode'])
                out['toggles'].append({'before':before,'checked':checked,'after':HG.force_idle_mode,'reopened':fresh_action.isChecked(),'publications':list(published),'checkable':action.isCheckable()})
                if HG.force_idle_mode and len(out['toggles'])==1:
                    submenu=find(fresh,['debug','debug modes']).menu();submenu.show();QW.QApplication.processEvents();submenu.grab().save(str(HERE/'fixtures/force_idle_mode_qt.png'));submenu.hide()
            # Exact real controller methods on a private clock/state preserve
            # their shutdown/forced/boot/disabled ordering without live globals.
            clock=[1_000_000];flags=types.SimpleNamespace(force_idle_mode=False,idle_report_mode=False,started_shutdown=False)
            real_time=types.SimpleNamespace(**vars(HydrusTime));real_time.GetNow=lambda:clock[0]//1000
            for name,getter in [('TimeHasPassedMS','GetNowMS')]:
                original=getattr(HydrusTime,name);g=dict(original.__globals__);g[getter]=lambda:clock[0];setattr(real_time,name,types.FunctionType(original.__code__,g))
            class Controller:
                def __init__(self):
                    self.new_options=c.new_options.Duplicate();self.options={'idle_normal':True,'idle_period':60,'idle_mouse_period':60}
                    self._program_is_shutting_down=False;self._previously_idle=False;self._idle_started=None;self._system_busy=True
                    self.boot=clock[0]-200_000;self.activity=clock[0]-600_000;self.queue=[];self.events=[]
                    self.new_options.SetNoneableInteger('idle_mode_client_api_timeout',60)
                    self.new_options.SetNoneableInteger('system_busy_cpu_count',1)
                def __getattr__(self,name):return getattr(c,name)
                def GetBootTimestampMS(self):return self.boot
                def GetTimestampMS(self,name):return clock[0] if name=='last_cpu_check' else self.activity
                def pub(self,name,*args,**kwargs):self.events.append(['publish',name])
                def Read(self,name,*args,**kwargs):
                    if name=='deferred_physical_delete':self.events.append(['read',name]);return (None,None)
                    if name=='trash_hashes':self.events.append(['read',name]);return list(self.queue)
                    return c.Read(name,*args,**kwargs)
                def WriteSynchronous(self,name,*args,**kwargs):
                    assert name=='content_updates';hashes=set()
                    for _,updates in args[0].IterateContentUpdates():
                        for update in updates:hashes.update(update.GetHashes())
                    self.events.append(['write',name,len(hashes)]);self.queue=[h for h in self.queue if h not in hashes]
            proxy=Controller()
            for name in ['CurrentlyIdle','SystemBusy']:
                original=getattr(ClientController.Controller,name);g=dict(original.__globals__);g.update(HG=flags,HydrusTime=real_time)
                setattr(proxy,name,types.MethodType(types.FunctionType(original.__code__,g),proxy))
            out['idle_matrix']=[]
            for forced in [False,True]:
                for shutdown in [False,True]:
                    for enabled in [False,True]:
                        for boot_recent in [False,True]:
                            flags.force_idle_mode=forced;proxy._program_is_shutting_down=shutdown;proxy.options['idle_normal']=enabled;proxy.boot=clock[0] if boot_recent else clock[0]-200_000
                            proxy._previously_idle=False;proxy._idle_started=None;proxy.events.clear()
                            out['idle_matrix'].append({'forced':forced,'shutdown':shutdown,'enabled':enabled,'boot_recent':boot_recent,'idle':proxy.CurrentlyIdle(),'busy':proxy.SystemBusy(),'idle_started':proxy._idle_started})
            proxy.boot=clock[0]-200_000;proxy._program_is_shutting_down=False;proxy.options['idle_normal']=True
            proxy.activity=clock[0];before=proxy.activity;out['recent_activity']=[]
            for forced in [False,True,False]:
                flags.force_idle_mode=forced;out['recent_activity'].append({'forced':forced,'idle':proxy.CurrentlyIdle(),'activity_unchanged':proxy.activity==before})
            options=dict(HC.options);options['trash_max_size']=None;options['trash_max_age']=0
            g=dict(ClientDaemons.DAEMONMaintainTrash.__globals__);g.update(CG=types.SimpleNamespace(client_controller=proxy),HC=types.SimpleNamespace(**vars(HC)),time=types.SimpleNamespace(sleep=lambda seconds:proxy.events.append(['sleep',seconds])),HydrusThreading=types.SimpleNamespace(IsThreadShuttingDown=lambda:False));g['HC'].options=options
            trash=types.FunctionType(ClientDaemons.DAEMONMaintainTrash.__code__,g)
            manager=ClientFilesManager.ClientFilesManager(proxy)
            original=manager.DoDeferredPhysicalDeletes.__func__;g=dict(original.__globals__);g['HG']=flags
            deferred=types.MethodType(types.FunctionType(original.__code__,g),manager)
            out['worker_entries']=[]
            for worker in ['trash','deferred']:
                for forced in [False,True]:
                    for normal in [False,True]:
                        for normally_idle in [False,True]:
                            flags.force_idle_mode=forced;proxy.activity=clock[0]-600_000 if normally_idle else clock[0];proxy.events.clear();proxy.queue=[b'A'*32]
                            proxy.new_options.SetBoolean('maintain_trash_in_normal_time' if worker=='trash' else 'deferred_file_deletes_in_normal_time',normal)
                            (trash if worker=='trash' else deferred)()
                            out['worker_entries'].append({'worker':worker,'forced':forced,'normal':normal,'normally_idle':normally_idle,'events':list(proxy.events),'remaining':len(proxy.queue)})
            out['limits']='Actual QAction/checked rebuilding/wake publication and controller/worker bodies run. Private clock/force globals, synthetic worker queue/database transport and sleep prevent reference media changes. Native DB/repository wake subscribers and CPUbusy backend are not represented by this Partial slice; trash/deferred schedules must not be reset on toggle.'
            return out
        finally:
            HG.force_idle_mode=old_force;c.pub=old_pub
            for menu in menus:menu.hide();menu.deleteLater()
    return session.controller.CallBlockingToQt(session.controller.gui,qt)
def main():
    import hydrus_driver,record_api
    if len(sys.argv)>1:
        Path(sys.argv[2]).write_text(json.dumps(hydrus_driver.run_client(record_api.unpack_fixture('basic'),record)));return
    with tempfile.TemporaryDirectory() as directory:
        result=Path(directory)/'record.json';hydrus_driver.run_in_subprocess(str(Path(__file__).resolve()),'--child',str(result));out=json.loads(result.read_text())
    path=HERE/'fixtures/force_idle_mode.json';path.write_text(json.dumps(out,indent=2)+'\n');print('wrote '+str(path))
if __name__=='__main__':main()

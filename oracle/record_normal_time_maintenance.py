#!/usr/bin/env python3
"""Actual Qt normal-time controls and real trash/deferred pass admission.

The reference worker code executes with an owned recording controller/queue;
only its idle input, queue/database transport, sleep and shutdown boundaries are
scripted. No original media or reference database is mutated by these passes.
"""
import json
import sys
import tempfile
import types
from pathlib import Path
HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))

def record(session):
    def qt():
        from qtpy import QtWidgets as QW
        from hydrus.core import HydrusConstants as HC
        from hydrus.client import ClientDaemons
        from hydrus.client.files import ClientFilesManager
        from hydrus.client.gui.panels.options.FilesAndTrashPanel import FilesAndTrashPanel
        c = session.controller
        keys = ['maintain_trash_in_normal_time', 'deferred_file_deletes_in_normal_time']
        original = c.new_options.Duplicate()
        panels = []
        def panel():
            p = FilesAndTrashPanel(c.gui); panels.append(p); return p
        def fields(p):
            return [p._maintain_trash_in_normal_time, p._deferred_file_deletes_in_normal_time]
        out = {}
        try:
            p = panel()
            out['defaults'] = {'saved':[c.new_options.GetBoolean(k) for k in keys], 'checked':[w.isChecked() for w in fields(p)], 'enabled':[w.isEnabled() for w in fields(p)]}
            out['controls'] = []
            for values in [[False,False], [False,True], [True,False], [True,True]]:
                for apply in [False,True]:
                    for key in keys: c.new_options.SetBoolean(key, True)
                    p = panel()
                    for w,value in zip(fields(p),values): w.setChecked(value)
                    if apply: p.UpdateOptions()
                    reopened = panel()
                    out['controls'].append({'typed':values,'apply':apply,'saved':[c.new_options.GetBoolean(k) for k in keys], 'reopened':[w.isChecked() for w in fields(reopened)]})
            for key in keys: c.new_options.SetBoolean(key, False)
            out['legacy_options'] = c.new_options.GetSerialisableTuple()
            p = panel(); p.resize(850,1400); p.show(); QW.QApplication.processEvents()
            p.grab().save(str(HERE/'fixtures/normal_time_maintenance_qt.png')); p.hide()
            events = []
            class Controller:
                def __init__(self): self.idle=False; self.queue=[]; self.after_write=None
                def __getattr__(self,name): return getattr(c,name)
                def CurrentlyIdle(self): events.append(['idle',self.idle]); return self.idle
                def Read(self,name,*args,**kwargs):
                    if name == 'deferred_physical_delete': events.append(['read',name]); return (None,None)
                    if name == 'trash_hashes': events.append(['read',name]); return list(self.queue)
                    return c.Read(name,*args,**kwargs)
                def WriteSynchronous(self,name,*args,**kwargs):
                    if name == 'content_updates':
                        hashes=set()
                        for _,updates in args[0].IterateContentUpdates():
                            for update in updates: hashes.update(update.GetHashes())
                        events.append(['write',name,len(hashes)])
                        self.queue=[h for h in self.queue if h not in hashes]
                        if self.after_write is not None: self.after_write()
                        return
                    return c.WriteSynchronous(name,*args,**kwargs)
                def pub(self,name,*args,**kwargs): events.append(['publish',name])
            proxy = Controller()
            manager = ClientFilesManager.ClientFilesManager(proxy)
            # Copy the real function's globals, never replacing shared CG or time.
            options = dict(HC.options); options['trash_max_size']=None; options['trash_max_age']=0
            stopped = [False]
            globals_ = dict(ClientDaemons.DAEMONMaintainTrash.__globals__)
            globals_.update(CG=types.SimpleNamespace(client_controller=proxy), HC=types.SimpleNamespace(**vars(HC)), time=types.SimpleNamespace(sleep=lambda seconds: events.append(['sleep',seconds])), HydrusThreading=types.SimpleNamespace(IsThreadShuttingDown=lambda: stopped[0]))
            globals_['HC'].options = options
            trash = types.FunctionType(ClientDaemons.DAEMONMaintainTrash.__code__, globals_)
            out['passes'] = []
            for worker in ['trash','deferred']:
                for idle in [False,True]:
                    for normal in [False,True]:
                        events.clear(); proxy.idle=idle; proxy.queue=[b'A'*32]; stopped[0]=False
                        c.new_options.SetBoolean(keys[worker=='deferred'],normal)
                        (trash if worker=='trash' else manager.DoDeferredPhysicalDeletes)()
                        out['passes'].append({'worker':worker,'idle':idle,'normal':normal,'events':list(events),'remaining':len(proxy.queue)})
            events.clear(); proxy.idle=False; proxy.queue=[b'A'*32]; stopped[0]=True
            c.new_options.SetBoolean(keys[0],True); trash()
            out['trash_shutdown'] = {'events':list(events),'remaining':len(proxy.queue)}
            events.clear(); proxy.idle=False; proxy.queue=[bytes([i])*32 for i in range(16)]; stopped[0]=False
            c.new_options.SetBoolean(keys[0],True)
            proxy.after_write=lambda: c.new_options.SetBoolean(keys[0],False)
            trash()
            out['trash_admitted_setting_change']={'events':list(events),'remaining':len(proxy.queue),'saved_normal':c.new_options.GetBoolean(keys[0])}
            events.clear(); proxy.queue=[bytes([i])*32 for i in range(16)]; c.new_options.SetBoolean(keys[0],True)
            proxy.after_write=lambda: stopped.__setitem__(0,True)
            trash()
            out['trash_shutdown_between_groups']={'events':list(events),'remaining':len(proxy.queue)}
            proxy.after_write=None

            out['limits'] = 'Actual worker pass entry and real trash content-update construction execute. Recorded idle/queue/database/sleep/shutdown boundaries are owned and synthetic; these passes do not mutate media. Broader idle/system-busy/scheduler parity is unclaimed.'
            return out
        finally:
            c.new_options=original
            for p in panels: p.hide(); p.deleteLater()
    return session.controller.CallBlockingToQt(session.controller.gui,qt)

def main():
    import hydrus_driver, record_api
    if len(sys.argv)>1:
        output=Path(sys.argv[2])
        result=hydrus_driver.run_client(record_api.unpack_fixture('basic'),record)
        output.write_text(json.dumps(result)); return
    with tempfile.TemporaryDirectory() as directory:
        result=Path(directory)/'record.json'
        hydrus_driver.run_in_subprocess(str(Path(__file__).resolve()),'--child',str(result))
        out=json.loads(result.read_text())
    path=HERE/'fixtures/normal_time_maintenance.json'; path.write_text(json.dumps(out,indent=2)+'\n'); print('wrote '+str(path))
if __name__=='__main__': main()

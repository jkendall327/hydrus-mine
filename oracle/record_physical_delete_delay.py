#!/usr/bin/env python3
"""Actual Qt deletion-delay fields and real deferred-delete loop on owned temp files.

The loop's controller queue and waits are recorded boundaries; physical paths
are disposable authored files, and ClientPaths.DeletePath still performs deletion.
Wait callbacks expose captured period/last-pair ordering without real long sleeps.
"""
import collections
import json
import sys
import tempfile
from pathlib import Path
HERE = Path(__file__).resolve().parent
sys.path.insert(0,str(HERE))


def record(session):
    def qt():
        from qtpy import QtCore as QC, QtGui as QG, QtWidgets as QW
        from hydrus.core import HydrusConstants as HC, HydrusGlobals as HG
        from hydrus.client import ClientPaths
        from hydrus.client.files import ClientFilesManager
        from hydrus.client.gui.panels.options.FilesAndTrashPanel import FilesAndTrashPanel
        c=session.controller
        key='ms_to_wait_between_physical_file_deletes'
        original=c.new_options.Duplicate();old_recycle=HC.options['delete_to_recycle_bin'];old_shutdown=HG.started_shutdown
        windows=[]
        def panel():
            p=FilesAndTrashPanel(c.gui);windows.append(p);return p
        def fields(p):
            w=p._ms_to_wait_between_physical_file_deletes
            return [w._seconds.value(),w._milliseconds.value()]
        try:
            p=panel();w=p._ms_to_wait_between_physical_file_deletes
            defaults={'raw':c.new_options.GetInteger(key),'fields':fields(p),'minimum':w._min,'bounds':[[w._seconds.minimum(),w._seconds.maximum()],[w._milliseconds.minimum(),w._milliseconds.maximum()]],'enabled':[w._seconds.isEnabled(),w._milliseconds.isEnabled()]}
            cases=[]
            for raw,typed,apply in [(600,[1,234],False),(600,[1,234],True),*[(v,None,True) for v in [-1,0,19,20,21,29,600,1029,59999,60060]],(600,[0,0],True),(600,[1,29],True),(1028,[1,29],True),(600,[99,1001],True)]:
                c.new_options.SetInteger(key,raw);p=panel();before=fields(p);w=p._ms_to_wait_between_physical_file_deletes
                if typed is not None:
                    w._seconds.setValue(typed[0]);w._milliseconds.setValue(typed[1])
                    # The real Apply button moves focus out of this widget.
                    w.eventFilter(w._milliseconds,QG.QFocusEvent(QC.QEvent.Type.FocusOut))
                shown=fields(p)
                if apply:p.UpdateOptions()
                cases.append({'raw':raw,'typed':typed,'apply':apply,'before':before,'fields':shown,'saved':c.new_options.GetInteger(key),'reopened':fields(panel())})
            c.new_options.SetInteger(key,1234);legacy=c.new_options.GetSerialisableTuple()
            p=panel();p.resize(800,1200);p.show();QW.QApplication.processEvents();p.grab().save(str(HERE/'fixtures/physical_delete_delay_qt.png'));p.hide()
            manifest=json.loads((HERE/'fixtures/legacy_db/basic.manifest.json').read_text());hash_=bytes.fromhex(manifest['files'][0]['hash']);media=c.Read('media_results',[hash_])[0]
            results=[]
            HC.options['delete_to_recycle_bin']=False
            class Proxy:
                def __getattr__(self,name):return getattr(c,name)
                def CurrentlyIdle(self):return True
                def Read(self,name,*args,**kwargs):
                    if name=='deferred_physical_delete':
                        events.append(['read']);return queue[0] if queue else (None,None)
                    if name=='media_result':return media
                    return c.Read(name,*args,**kwargs)
                def WriteSynchronous(self,name,*args,**kwargs):
                    if name=='clear_deferred_physical_delete':
                        events.append(['clear']);queue.popleft();return
                    return c.WriteSynchronous(name,*args,**kwargs)
                def pub(self,name,*args,**kwargs):events.append(['publish',name])
            proxy=Proxy();manager=ClientFilesManager.ClientFilesManager(proxy)
            class Wait:
                def wait(self,period):
                    events.append(['wait',period]);assert all(not p.exists() for p in deleted)
                    if scenario=='change_during_wait':c.new_options.SetInteger(key,900)
                    if scenario=='shutdown':HG.started_shutdown=True;manager.shutdown()
                def clear(self):events.append(['wait_clear'])
                def set(self):events.append(['wake'])
            for scenario in ['pairs','change_during_wait','missing','shutdown','failure']:
                with tempfile.TemporaryDirectory() as directory:
                    paths={};deleted=[];events=[]
                    hashes=[i.to_bytes(32,'big') for i in [201,202]]
                    for h in hashes:
                        for kind in ['f','t']:
                            path=Path(directory)/(kind+h.hex());path.write_bytes(b'owned temporary delete evidence');paths[(kind,h)]=path
                    if scenario=='missing':paths[('f',hashes[0])].unlink()
                    queue=collections.deque((h,h) for h in hashes)
                    manager._GenerateExpectedFilePath=lambda h,m: str(paths[('f',h)])
                    manager._GenerateExpectedThumbnailPath=lambda h: str(paths[('t',h)])
                    def missing(h):
                        from hydrus.core import HydrusExceptions
                        raise HydrusExceptions.FileMissingException('authored missing file')
                    manager._LookForFilePath=missing
                    manager._physical_file_delete_wait=Wait()
                    saved_delete=ClientPaths.DeletePath
                    def delete(path,*args,**kwargs):
                        events.append(['delete',Path(path).name[0],bool(kwargs.get('always_delete_fully',False))])
                        if scenario=='failure':raise PermissionError('authored delete refusal')
                        saved_delete(path,*args,**kwargs);deleted.append(Path(path))
                    ClientPaths.DeletePath=delete
                    c.new_options.SetInteger(key,600);HG.started_shutdown=False
                    error=None
                    try:manager.DoDeferredPhysicalDeletes()
                    except Exception as exc:error=[type(exc).__name__,str(exc)]
                    finally:ClientPaths.DeletePath=saved_delete
                    results.append({'scenario':scenario,'events':events,'remaining':len(queue),'exists':[[paths[('f',h)].exists(),paths[('t',h)].exists()] for h in hashes],'error':error,'saved_after':c.new_options.GetInteger(key)})
            # Actual threading.Event is used by the manager's real shutdown hook.
            import threading
            event=threading.Event();manager._physical_file_delete_wait=event;manager.shutdown();wake=event.wait(600)
            return {'defaults':defaults,'controls':cases,'legacy_options':legacy,'passes':results,'shutdown_wakes_event':wake,'limits':'Only disposable authored paths are physically mutated. Controller queue/publication and waits are recorded boundaries, actual deferred loop and ClientPaths physical delete run unchanged. Idle/normal scheduling policy and prefix-location repair are outside this finite delay recording.'}
        finally:
            c.new_options=original;HC.options['delete_to_recycle_bin']=old_recycle;HG.started_shutdown=old_shutdown
            for p in windows:p.hide();p.deleteLater()
    return session.controller.CallBlockingToQt(session.controller.gui,qt)


def main():
    import hydrus_driver,record_api
    if len(sys.argv)>1:
        output=Path(sys.argv[2]);result=hydrus_driver.run_client(record_api.unpack_fixture('basic'),record);output.write_text(json.dumps(result));return
    with tempfile.TemporaryDirectory() as directory:
        output=Path(directory)/'record.json';hydrus_driver.run_in_subprocess(str(Path(__file__).resolve()),'--child',str(output));result=json.loads(output.read_text())
    output=HERE/'fixtures/physical_delete_delay.json';output.write_text(json.dumps(result,indent=2)+'\n');print('wrote '+str(output))
if __name__=='__main__':main()

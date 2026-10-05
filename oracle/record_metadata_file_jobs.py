#!/usr/bin/env python3
"""Execute actual Qt metadata acceptance and real temporary-file workers.

The reference worker bodies, ChangeFileExt and UpdateFileModifiedTimestampMS
run unchanged. Only thread dispatch, clock, job cancellation, database command
capture, and an injected rename failure are scripted. Files belong to a copied
basic client; authored synthetic hashes never alter the installed reference.
"""
import hashlib, json, os, sys, tempfile
from pathlib import Path
HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))

def record(session):
    def qt():
        from qtpy import QtWidgets as QW
        from hydrus.core import HydrusConstants as HC, HydrusPaths, HydrusTime
        from hydrus.core.processes import HydrusThreading
        from hydrus.client import ClientThreading
        from hydrus.client.gui.media import ClientGUIMediaModalActions as Actions
        from hydrus.client.gui import ClientGUITopLevelWindowsPanels as Windows
        c = session.controller
        seed = c.Read('media_results', [bytes.fromhex(json.loads((HERE/'fixtures/legacy_db/basic.manifest.json').read_text())['files'][0]['hash'])])[0]
        manager = c.client_files_manager
        saved = [(c, 'Write', c.Write), (c, 'WriteSynchronous', c.WriteSynchronous),
                 (c, 'CallToThread', c.CallToThread), (c, 'pub', c.pub),
                 (HydrusTime, 'GetNow', HydrusTime.GetNow),
                 (ClientThreading, 'JobStatus', ClientThreading.JobStatus),
                 (Windows, 'DialogEdit', Windows.DialogEdit),
                 (HydrusPaths, 'MergeFile', HydrusPaths.MergeFile),
                 (HydrusThreading, 'BigJobPauser', HydrusThreading.BigJobPauser)]
        real_status = ClientThreading.JobStatus
        real_merge = HydrusPaths.MergeFile
        now = [1000]; output = []; writes = []; progress = []; published = []; statuses = []
        config = {}; ordinal = {}; threads = []
        def encode(value):
            if isinstance(value, bytes): return ordinal.get(value, value.hex())
            if isinstance(value, (set, list, tuple)): return sorted(encode(v) for v in value) if isinstance(value, set) else [encode(v) for v in value]
            return value
        def write(name, *args, **kwargs):
            if name == 'content_updates': writes.append(dict(name=name, has_content=args[0].HasContent()))
            else: writes.append(dict(name=name, args=encode(args)))
        class Status:
            def __init__(self, *args, **kwargs): self.actual=real_status(*args, **kwargs); statuses.append(self)
            def __getattr__(self, key): return getattr(self.actual, key)
            def SetGauge(self, done, total, *args, **kwargs):
                self.actual.SetGauge(done,total,*args,**kwargs)
                now[0]=1000+done*config.get('seconds_per_file',0)
                if config.get('cancel_at') is not None and done >= config['cancel_at']: self.actual.Cancel()
                progress.append(dict(title=self.actual.GetStatusTitle(), text=self.actual.GetStatusText(), gauge=list(self.actual.GetGauge()), cancelled=self.actual.IsCancelled()))
            def FinishAndDismiss(self, *args, **kwargs): self.actual.FinishAndDismiss(*args,**kwargs)
        class Pauser:
            def __init__(self,*args,**kwargs): pass
            def Pause(self): pass
        class Dialog(QW.QDialog):
            def __init__(self, parent, *args, **kwargs): super().__init__(parent)
            def __enter__(self): return self
            def __exit__(self,*args): self.deleteLater()
            def SetPanel(self,panel):
                self.panel=panel
                if config['kind']=='modified':
                    value=panel._file_modified_time.GetValue().DuplicateWithNewTimestampMS(1700000000123)
                    value.SetStepMS(250); panel._file_modified_time.SetValue(value,from_user=True)
                else: panel._forced_mime.SetValue(HC.IMAGE_PNG)
            def exec(self): return QW.QDialog.DialogCode.Rejected if config.get('reject') else QW.QDialog.DialogCode.Accepted
        def publish(topic, *args, **kwargs):
            if topic=='message': published.append(dict(title=args[0].GetStatusTitle(), at=now[0], gauge=list(args[0].GetGauge()) if args[0].GetGauge() is not None else None))
        c.Write=write; c.WriteSynchronous=write; c.CallToThread=lambda f,*args,**kwargs:threads.append((f,args,kwargs)); c.pub=publish
        HydrusTime.GetNow=lambda:now[0]; ClientThreading.JobStatus=Status
        Windows.DialogEdit=Dialog; HydrusThreading.BigJobPauser=Pauser
        try:
            for kind,count,cancel,speed,fallback in [('modified',4,None,0,False),('modified',4,2,2,False),('modified',4,0,2,False),('modified',4,None,1,False),('force',65,None,0,False),('force',65,64,0,False),('force',65,0,0,False),('force',1,None,0,True),('force',0,None,0,False)]:
                config.update(kind=kind,cancel_at=cancel,seconds_per_file=speed); now[0]=1000
                writes.clear(); progress.clear(); published.clear(); statuses.clear(); threads.clear(); ordinal.clear()
                medias=[]; paths=[]
                for i in range(count):
                    result=seed.Duplicate(); h=hashlib.sha256(f'metadata-{len(output)}-{i}'.encode()).digest()
                    info=result.GetFileInfoManager(); info.hash=h; info.mime=HC.IMAGE_JPEG; info.original_mime=None
                    ordinal[h]=i; medias.append(__import__('hydrus.client.media.ClientMediaSingle',fromlist=['MediaSingle']).MediaSingle(result))
                    source=Path(manager._GenerateExpectedFilePath(h,HC.IMAGE_JPEG)); source.parent.mkdir(parents=True,exist_ok=True)
                    source.write_bytes(b'owned metadata fixture'); os.utime(source,(1234567890,1234567890))
                    paths.append((source,Path(manager._GenerateExpectedFilePath(h,HC.IMAGE_PNG))))
                def fail_merge(*args,**kwargs): raise PermissionError('scripted rename unavailable')
                HydrusPaths.MergeFile=fail_merge if fallback else real_merge
                if kind=='modified':
                    Actions.EditFileTimestamps(c.gui,medias)
                    for f,args,kwargs in threads: f(*args,**kwargs)
                else:
                    Actions.SetFilesForcedFiletypes(c.gui,medias)
                    for f,args,kwargs in threads: f(*args,**kwargs)
                files=[]
                for source,dest in paths:
                    files.append(dict(source=source.exists(),destination=dest.exists(),modified_ms=int(source.stat().st_mtime*1000) if source.exists() else None,access_seconds=int(source.stat().st_atime) if source.exists() else None,destination_modified_seconds=int(dest.stat().st_mtime) if dest.exists() else None))
                output.append(dict(kind=kind,count=count,cancel_at=cancel,seconds_per_file=speed,fallback=fallback,writes=list(writes),progress=list(progress),published=list(published),files=files,finished=all(s.IsDone() for s in statuses),dismissed=all(s.IsDismissed() for s in statuses)))
                for pair in paths:
                    for path in pair: path.unlink(missing_ok=True)
            rejections=[]
            for kind in ['modified','force']:
                config.update(kind=kind,reject=True); writes.clear(); threads.clear()
                if kind=='modified': Actions.EditFileTimestamps(c.gui,[__import__('hydrus.client.media.ClientMediaSingle',fromlist=['MediaSingle']).MediaSingle(seed.Duplicate())])
                else: Actions.SetFilesForcedFiletypes(c.gui,[__import__('hydrus.client.media.ClientMediaSingle',fromlist=['MediaSingle']).MediaSingle(seed.Duplicate())])
                rejections.append(dict(kind=kind,writes=list(writes),workers=len(threads)))
            return dict(cases=output,dialog_rejections=rejections)
        finally:
            for obj,key,value in saved: setattr(obj,key,value)
    return session.controller.CallBlockingToQt(session.controller.gui,qt)

def main():
    import hydrus_driver
    if len(sys.argv)>1:
        import record_api
        Path(sys.argv[2]).write_text(json.dumps(hydrus_driver.run_client(record_api.unpack_fixture('basic'),record))); return
    with tempfile.TemporaryDirectory() as folder:
        output=Path(folder)/'result.json'; hydrus_driver.run_in_subprocess(str(Path(__file__).resolve()),'--child',str(output)); result=json.loads(output.read_text())
    (HERE/'fixtures/metadata_file_jobs.json').write_text(json.dumps(result,indent=2)+'\n')
    print('wrote metadata_file_jobs.json')
if __name__=='__main__': main()

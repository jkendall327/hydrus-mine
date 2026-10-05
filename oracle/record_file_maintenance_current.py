#!/usr/bin/env python3
"""Real scheduled-work list/actions and force-maintenance progress on a copied DB.

Only AsyncQtJob scheduling is made synchronous for deterministic list publication;
real work/publish callables and actual queue reads/writes are retained. Force work
is captured at its thread dispatch and invoked outside Qt. Questions and popup
publication are observed; the manager and its physical runners are unchanged.
The named redownload producer is separately driven with a synthetic file URL;
network imports remain paused so this records its actual page/queue destination.
"""
import json
import sys
import tempfile
from pathlib import Path
HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))

def record(session):
    from hydrus.client import ClientThreading
    from hydrus.client.gui import ClientGUIAsync as A, ClientGUIDialogsQuick as Q
    from hydrus.client.gui.panels import ClientGUIScrolledPanelsReview as R
    from hydrus.client.files import ClientFilesMaintenance as F
    from hydrus.core import HydrusTime, HydrusData
    from qtpy import QtWidgets as W
    c = session.controller
    jobs = []
    messages = []
    events = []
    dispatched = []
    gauges = []
    cancel_next = [False]
    old_gauge = ClientThreading.JobStatus.SetGauge
    def gauge(job, done, total, level=1):
        old_gauge(job, done, total, level)
        if job.GetStatusTitle() == "file maintenance":
            gauges.append(dict(done=done, total=total, text=job.GetStatusText()))
            if cancel_next[0]:
                cancel_next[0] = False
                job.Cancel()
    ClientThreading.JobStatus.SetGauge = gauge
    held = [2_000_000_000]
    original = (A.AsyncQtJob.start, Q.GetYesNo, c.CallToThread, c.pub, HydrusTime.GetNow)
    panel = [None]
    def publish(topic, *args, **kwargs):
        if topic == 'message':
            jobs.append(args[0])
        return original[3](topic, *args, **kwargs)
    def setup():
        c.new_options.SetBoolean('file_maintenance_during_idle', False)
        c.new_options.SetBoolean('file_maintenance_during_active', False)
        c.new_options.SetBoolean('pause_all_new_network_traffic', True)
        HydrusTime.GetNow = lambda: held[0]
        def immediate(job):
            result = job._work_callable()
            job._publish_callable(result)
            if job._ui_restoration_callable:
                job._ui_restoration_callable()
        A.AsyncQtJob.start = immediate
        c.CallToThread = lambda fn, *a, **kw: dispatched.append((fn, a, kw))
        c.pub = publish
        manifest = json.loads((HERE/'fixtures/legacy_db/basic.manifest.json').read_text())
        hashes = [bytes.fromhex(row['hash']) for row in manifest['files'][:3]]
        ids = [m.GetHashId() for m in c.Read('media_results', hashes)]
        for code in F.ALL_REGEN_JOBS_IN_RUN_ORDER:
            c.WriteSynchronous('file_maintenance_cancel_jobs', code)
        c.WriteSynchronous('file_maintenance_add_jobs', ids[:2], F.REGENERATE_FILE_DATA_JOB_FILE_HAS_EXIF, 0)
        c.WriteSynchronous('file_maintenance_add_jobs', ids[2:], F.REGENERATE_FILE_DATA_JOB_FILE_HAS_ICC_PROFILE, held[0])
        panel[0] = R.ReviewFileMaintenance(c.gui, {})
        panel[0].resize(950, 600)
        panel[0].show()
        W.QApplication.processEvents()
        return ids
    def state(name):
        p = panel[0]
        p._RefreshWorkDue()
        codes = p._jobs_listctrl.GetData()
        events.append(dict(name=name, now=held[0], rows=[dict(code=code, display=p._ConvertJobTypeToDisplayTuple(code), sort=p._ConvertJobTypeToSortTuple(code)) for code in codes], selected=sorted(p._jobs_listctrl.GetData(only_selected=True)), do_work=p._WorkToDoOnSelection(), do_all=p._WorkToDo(), questions=list(messages)))
    def choices():
        p = panel[0]
        state('opened_due_and_equal_deadline')
        p._jobs_listctrl.SelectDatas([F.REGENERATE_FILE_DATA_JOB_FILE_HAS_ICC_PROFILE])
        state('future_only_selection')
        p._jobs_listctrl.SelectDatas([F.REGENERATE_FILE_DATA_JOB_FILE_HAS_EXIF])
        state('due_selection')
        p.grab().save(str(HERE/'fixtures/file-maintenance-current-qt.png'))
        def answer(parent, message, **kwargs):
            messages.append(dict(message=message, **kwargs))
            return W.QDialog.DialogCode.Rejected
        Q.GetYesNo = answer
        p._DeleteWork()
        state('clear_cancel')
        p._DoWork()
    try:
        ids = c.CallBlockingToQt(c.gui, setup)
        c.CallBlockingToQt(c.gui, choices)
        while dispatched:
            fn, args, kwargs = dispatched.pop(0)
            fn(*args, **kwargs)
        c.CallBlockingToQt(c.gui, lambda: state('selected_work_finished_future_retained'))
        def future():
            held[0] += 1
            state('past_deadline_due')
            panel[0]._DoAllWork()
        c.CallBlockingToQt(c.gui, future)
        while dispatched:
            fn, args, kwargs = dispatched.pop(0)
            fn(*args, **kwargs)
        c.CallBlockingToQt(c.gui, lambda: state('all_work_finished'))
        def clear():
            c.WriteSynchronous('file_maintenance_add_jobs', ids, F.REGENERATE_FILE_DATA_JOB_FILE_HAS_EXIF, held[0]+100)
            state('future_added')
            panel[0]._jobs_listctrl.SelectDatas([F.REGENERATE_FILE_DATA_JOB_FILE_HAS_EXIF])
            def answer(parent, message, **kwargs):
                messages.append(dict(message=message, **kwargs))
                return W.QDialog.DialogCode.Accepted
            Q.GetYesNo = answer
            panel[0]._DeleteWork()
            state('clear_accepted_removes_future_too')
            current = c.gui.GetCurrentPage().GetPageKey().hex()
            url = 'https://maintenance.synthetic.example/repair.png'
            errors = []
            old_exception = HydrusData.ShowException
            try:
                HydrusData.ShowException = lambda error, **kwargs: errors.append(str(error))
                c.gui.ImportURL('0missing-scheme', 'missing files redownloader')
                c.gui.ImportURL(url, 'missing files redownloader')
                c.gui.ImportURL(url, 'missing files redownloader')
            finally:
                HydrusData.ShowException = old_exception
            pages = c.gui._notebook._GetMediaPages(False)
            destinations = [dict(name=p.GetName(), key=p.GetPageKey().hex(), count=len(p.GetPageManager().GetVariable('urls_import').GetFileSeedCache().GetFileSeeds())) for p in pages if p.IsURLImportPage() and p.GetName() == 'missing files redownloader']
            return dict(url=url, errors=errors, before_current=current, after_current=c.gui.GetCurrentPage().GetPageKey().hex(), destinations=destinations)
        destination = c.CallBlockingToQt(c.gui, clear)
        def cancellable():
            c.WriteSynchronous('file_maintenance_add_jobs', ids[:2], F.REGENERATE_FILE_DATA_JOB_FILE_HAS_EXIF, 0)
            state('cancel_work_queued')
            cancel_next[0] = True
            panel[0]._DoAllWork()
        c.CallBlockingToQt(c.gui, cancellable)
        while dispatched:
            fn, args, kwargs = dispatched.pop(0)
            fn(*args, **kwargs)
        c.CallBlockingToQt(c.gui, lambda: state('cancel_at_file_boundary'))
        c.files_maintenance_manager.ForceMaintenance(mandated_job_types=[F.REGENERATE_FILE_DATA_JOB_FILE_HAS_ICC_PROFILE])
        return dict(events=events, gauges=gauges, progress=[dict(title=j.GetStatusTitle(), text=j.GetStatusText(), done=j.IsDone(), cancellable=j.IsCancellable(), dismissed=j.IsDismissed()) for j in jobs], redownload=destination)
    finally:
        def cleanup():
            ClientThreading.JobStatus.SetGauge = old_gauge
            A.AsyncQtJob.start, Q.GetYesNo, c.CallToThread, c.pub, HydrusTime.GetNow = original
            if panel[0] is not None:
                panel[0].hide()
                panel[0].deleteLater()
        c.CallBlockingToQt(c.gui, cleanup)

def main():
    import hydrus_driver
    import record_api
    if len(sys.argv)>1 and sys.argv[1]=='--child':
        output=Path(sys.argv[2])
        result=hydrus_driver.run_client(record_api.unpack_fixture('basic'), record)
        output.write_text(json.dumps(result))
        return
    with tempfile.TemporaryDirectory() as folder:
        output=Path(folder)/'result.json'
        hydrus_driver.run_in_subprocess(str(Path(__file__).resolve()), '--child', str(output))
        result=json.loads(output.read_text())
    (HERE/'fixtures/file_maintenance_current.json').write_text(json.dumps(result,indent=2)+'\n')
    print('wrote file_maintenance_current.json')
if __name__=='__main__': main()

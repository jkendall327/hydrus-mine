#!/usr/bin/env python3
"""Execute actual subscription Sync/file loops with scripted typed file exceptions.

Record cumulative errors across real saved query logs, 404/Veto/DataMissing
exclusions, None, reset on the next sync, exact abandonment/delay and 5-second
throttling. File exception inputs are scripted; no real site is contacted.
Also drive the actual Qt DownloadingPanel noneable field and UpdateOptions.
"""
import json
import os
import sys
import tempfile
import types
HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
NOW = 1700000000


def record(session):
    c = session.controller
    from hydrus.core import HydrusExceptions as E, HydrusTime as T, HydrusData as D
    from hydrus.client import ClientConstants as CC, ClientThreading
    from hydrus.client.importing import ClientImportSubscriptions as S, ClientImportSubscriptionQuery as Q, ClientImportFileSeeds as F
    from hydrus.client.gui.panels.options.DownloadingPanel import DownloadingPanel
    def qt():
        panel = DownloadingPanel(c.gui, c.new_options)
        control = panel._subscription_file_error_cancel_threshold
        states = []
        for value in [5, None, 2, 1000000]:
            control.SetValue(value)
            states.append({'given': value, 'value': control.GetValue(), 'saved_before': c.new_options.GetNoneableInteger('subscription_file_error_cancel_threshold')})
            panel.UpdateOptions()
            states[-1]['saved_after'] = c.new_options.GetNoneableInteger('subscription_file_error_cancel_threshold')
        panel.deleteLater()
        return {'states': states, 'none_phrase': control._checkbox.text(), 'minimum': control._number_value.minimum(), 'maximum': control._number_value.maximum(), 'unit': control._unit}
    options = c.CallBlockingToQt(c.gui, qt)
    original_work = F.FileSeed.WorkOnURL; original_time = S.time; original_file_time = F.time; old_add_job = c.network_engine.AddJob
    original_now = T.GetNow; original_view = c.WaitUntilViewFree
    old_show = D.ShowText; old_exception = D.ShowException
    sleeps = []; errors = []; calls = []
    S.time = types.SimpleNamespace(sleep=lambda seconds: sleeps.append(seconds))
    F.time = types.SimpleNamespace(sleep=lambda seconds: sleeps.append(seconds))
    T.GetNow = lambda: NOW; c.WaitUntilViewFree = lambda: None
    D.ShowText = lambda text: errors.append(text); D.ShowException = lambda error: errors.append(str(error))
    def work(seed, *args, **kwargs):
        kind = seed.file_seed_data.rsplit('/', 1)[1]; calls.append(kind)
        # Model an escaped post-work exception after the seed/cache have updated.
        seed.SetStatus(CC.STATUS_ERROR if kind != 'ok5' else CC.STATUS_SUCCESSFUL_AND_NEW)
        args[0].NotifyFileSeedsUpdated((seed,))
        if kind.startswith('missing'): raise E.DataMissing('synthetic missing data')
        if kind.startswith('404'): raise E.NotFoundException('synthetic 404')
        if kind.startswith('veto'): raise E.VetoException('synthetic veto')
        if kind.startswith('error'): raise E.NetworkException('synthetic file network failure')
        if kind.startswith('other'): raise ValueError('synthetic import failure')
        seed.SetStatus(CC.STATUS_SUCCESSFUL_AND_NEW)
        return True
    F.FileSeed.WorkOnURL = work
    c.new_options.SetBoolean('process_subs_in_random_order', False)
    c.new_options.SetBoolean('pause_subs_sync', False)
    c.new_options.SetBoolean('pause_all_new_network_traffic', False)
    c.new_options.SetInteger('subscription_other_error_delay', 37)
    cases = []
    try:
        for label, threshold, queries in [('cross-query threshold', 2, [['error1'], ['missing2', '4043', 'other4', 'ok5']]), ('one', 1, [['missing1', '4042', 'veto3', 'error4', 'ok5']]), ('unlimited', None, [['error1', 'missing2', '4043', 'other4', 'ok5']])]:
            c.new_options.SetNoneableInteger('subscription_file_error_cancel_threshold', threshold)
            sub = S.Subscription('synthetic failures', gug_key_and_name=(bytes([11])*32, 'synthetic gallery'))
            headers = []; logs = []
            for i, kinds in enumerate(queries):
                header = Q.SubscriptionQueryHeader(); header.SetQueryText(str(i))
                log = Q.SubscriptionQueryLogContainer(header.GetQueryLogContainerName())
                cache = log.GetFileSeedCache()
                cache.AddFileSeeds([F.FileSeed(F.FILE_SEED_TYPE_URL, f'https://failures.example/{i}/{kind}') for kind in kinds])
                header.UpdateFileStatus(log)
                header.SetQueryLogContainerStatus(Q.LOG_CONTAINER_SYNCED)
                header.FileBandwidthOK = lambda *args: True
                header.FileDomainOK = lambda *args: True
                header.FileLoginOK = lambda *args: (True, '')
                c.WriteSynchronous('serialisable', log)
                headers.append(header); logs.append(log)
            sub.SetQueryHeaders(headers)
            sub._SyncQueryLogContainersCanDoWork = lambda: False
            sub._SyncQueriesCanDoWork = lambda: False
            for run in range(2 if label == 'cross-query threshold' else 1):
                sleeps.clear(); errors.clear(); calls.clear()
                sub.ScrubDelay()
                sub.Sync()
                saved_logs = [c.Read('serialisable_named', log.SERIALISABLE_TYPE, log.GetName()) for log in logs]
                cases.append({'case': label, 'run': run, 'threshold': threshold, 'calls': list(calls), 'file_errors': sub._file_error_count,
                    'statuses': [[seed.status for seed in log.GetFileSeedCache().GetFileSeeds()] for log in saved_logs],
                    'sleeps': list(sleeps), 'delay': sub._no_work_until - NOW if sub._no_work_until else 0,
                    'reason': sub._no_work_until_reason, 'messages': list(errors)})
        # Unlike escaped post-work errors, actual WorkOnURL catches these.
        F.FileSeed.WorkOnURL = original_work
        c.network_engine.AddJob = lambda job: None
        class FakeNetworkJob:
            def __init__(self, kind): self.kind = kind
            def WaitUntilDone(self):
                if self.kind == '404': raise E.NotFoundException('synthetic 404')
                raise E.NetworkException('synthetic HTTP 500')
            def __getattr__(self, name): return lambda *args, **kwargs: None
        handled = []
        for kind in ['500', '404']:
            c.new_options.SetNoneableInteger('subscription_file_error_cancel_threshold', 1)
            sub = S.Subscription('handled failures', gug_key_and_name=(bytes([11])*32, 'synthetic gallery'))
            header = Q.SubscriptionQueryHeader(); header.SetQueryText('handled')
            log = Q.SubscriptionQueryLogContainer(header.GetQueryLogContainerName())
            seed = F.FileSeed(F.FILE_SEED_TYPE_URL, 'https://failures.example/raw/' + kind)
            log.GetFileSeedCache().AddFileSeeds([seed]); header.UpdateFileStatus(log); header.SetQueryLogContainerStatus(Q.LOG_CONTAINER_SYNCED)
            header.FileBandwidthOK = lambda *args: True; header.FileDomainOK = lambda *args: True; header.FileLoginOK = lambda *args: (True, '')
            header.GenerateNetworkJobFactory = lambda *args: lambda *args, **kwargs: FakeNetworkJob(kind)
            c.WriteSynchronous('serialisable', log); sub.SetQueryHeaders([header])
            sub._SyncQueryLogContainersCanDoWork = lambda: False; sub._SyncQueriesCanDoWork = lambda: False
            sleeps.clear(); errors.clear(); sub.Sync()
            saved = c.Read('serialisable_named', log.SERIALISABLE_TYPE, log.GetName()).GetFileSeedCache().GetFileSeeds()[0]
            handled.append({'kind': kind, 'status': saved.status, 'file_errors': sub._file_error_count, 'sleeps': list(sleeps), 'delay': sub._no_work_until, 'reason': sub._no_work_until_reason})
        return {'options': options, 'cases': cases, 'handled_work_on_url': handled}
    finally:
        F.FileSeed.WorkOnURL = original_work; S.time = original_time; F.time = original_file_time; c.network_engine.AddJob = old_add_job; T.GetNow = original_now; c.WaitUntilViewFree = original_view
        D.ShowText = old_show; D.ShowException = old_exception


def main():
    import hydrus_driver
    if len(sys.argv) > 1:
        import record_api
        output = sys.argv[2]
        result = hydrus_driver.run_client(record_api.unpack_fixture('basic'), record)
        with open(output, 'w') as stream: json.dump(result, stream)
        return
    with tempfile.TemporaryDirectory() as work:
        path = os.path.join(work, 'failures.json')
        hydrus_driver.run_in_subprocess(os.path.abspath(__file__), '--child', path)
        with open(path) as stream: result = json.load(stream)
    with open(os.path.join(HERE, 'fixtures', 'subscription_failure_limit.json'), 'w') as stream:
        json.dump(result, stream, indent=2); stream.write('\n')
    print('recorded subscription failure-limit actual Sync and Qt field')

if __name__ == '__main__': main()

#!/usr/bin/env python3
"""The background workers' pace, as the options page sets it.

Values are typed into the reference's real "maintenance and processing" options
panel and applied; then the real `_DoSingleLoop` of three workers runs on a
held clock:

- the potential duplicates search (`PotentialDuplicatesMaintenanceManager`),
- duplicates auto-resolution (`DuplicatesAutoResolutionManager`),
- file maintenance (`FilesMaintenanceManager`, its bandwidth-rule throttle).

Only the boundaries are scripted: whether the client is idle, the database
work (it takes a scripted share of the packet it is given and says whether more
is left), the file maintenance jobs (a scripted queue; each job reports its
weight to the throttle as `_RunJob` does) and the sleeps, which move the held
clock on instead of waiting. Writes `fixtures/maintenance_pace.json`.

Run: QT_QPA_PLATFORM=offscreen ~/pyenv/bin/python oracle/record_maintenance_pace.py
"""
import json
import sys
import tempfile
import types
from pathlib import Path

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))

T0 = 1_700_000_000

# values typed into the panel, as the native test types them
TYPED = {
    'similar': {'idle': (7.5, 123), 'normal': (2.04, 456)},
    'auto': {'idle': (3.25, 77), 'normal': (0.8, 88)},
    'files': {'idle': (5, 150), 'normal': (9, 45)},
}

# one packet of work: how much of the packet it took, whether work is left,
# and how many files it searched
PACKETS = [(0.5, False, 3), (1.25, True, 5), (10, True, 50)]

# the file maintenance queue: one job per file, by weight 100, 25 and 5
JOBS = ['metadata', 'exif', 'exif', 'exif', 'exif', 'presence', 'metadata', 'metadata', 'exif']


class Stop(Exception):
    pass


def record(session):
    def qt():
        from hydrus.core import HydrusTime
        from hydrus.core.networking import HydrusNetworking
        from hydrus.client import ClientDaemons
        from hydrus.client.duplicates import ClientDuplicatesAutoResolution
        from hydrus.client.duplicates import ClientPotentialDuplicatesManager
        from hydrus.client.files import ClientFilesMaintenance
        from hydrus.client.files import ClientFilesMaintenanceManager
        from hydrus.client.gui.panels.options.MaintenanceAndProcessingPanel import MaintenanceAndProcessingPanel

        c = session.controller
        original_options = c.new_options.Duplicate()
        panels = []
        clock = [float(T0)]
        idle = [False]
        events = []

        job_types = {
            'metadata': ClientFilesMaintenance.REGENERATE_FILE_DATA_JOB_FILE_METADATA,
            'exif': ClientFilesMaintenance.REGENERATE_FILE_DATA_JOB_FILE_HAS_EXIF,
            'presence': ClientFilesMaintenance.REGENERATE_FILE_DATA_JOB_FILE_INTEGRITY_PRESENCE_LOG_ONLY,
        }
        job_names = {v: k for (k, v) in job_types.items()}

        def panel():
            p = MaintenanceAndProcessingPanel(c.gui)
            panels.append(p)
            return p

        def type_values(values, switches):
            """Type into the panel and apply, as a user does."""
            p = panel()
            (s_idle, s_normal) = (values['similar']['idle'], values['similar']['normal'])
            p._potential_duplicates_search_work_time_idle.SetValue(s_idle[0])
            p._potential_duplicates_search_rest_percentage_idle.setValue(s_idle[1])
            p._potential_duplicates_search_work_time_active.SetValue(s_normal[0])
            p._potential_duplicates_search_rest_percentage_active.setValue(s_normal[1])
            (a_idle, a_normal) = (values['auto']['idle'], values['auto']['normal'])
            p._duplicates_auto_resolution_work_time_idle.SetValue(a_idle[0])
            p._duplicates_auto_resolution_rest_percentage_idle.setValue(a_idle[1])
            p._duplicates_auto_resolution_work_time_active.SetValue(a_normal[0])
            p._duplicates_auto_resolution_rest_percentage_active.setValue(a_normal[1])
            p._file_maintenance_idle_throttle_velocity.SetValue(values['files']['idle'])
            p._file_maintenance_active_throttle_velocity.SetValue(values['files']['normal'])
            (idle_on, normal_on) = switches
            p._maintain_similar_files_duplicate_pairs_during_idle.setChecked(idle_on)
            p._maintain_similar_files_duplicate_pairs_during_active.setChecked(normal_on)
            p._duplicates_auto_resolution_during_idle.setChecked(idle_on)
            p._duplicates_auto_resolution_during_active.setChecked(normal_on)
            p._file_maintenance_during_idle.setChecked(idle_on)
            p._file_maintenance_during_active.setChecked(normal_on)
            p.UpdateOptions()

        saved_keys = [
            'maintain_similar_files_duplicate_pairs_during_idle', 'maintain_similar_files_duplicate_pairs_during_active',
            'potential_duplicates_search_work_time_ms_idle', 'potential_duplicates_search_rest_percentage_idle',
            'potential_duplicates_search_work_time_ms_active', 'potential_duplicates_search_rest_percentage_active',
            'duplicates_auto_resolution_during_idle', 'duplicates_auto_resolution_during_active',
            'duplicates_auto_resolution_work_time_ms_idle', 'duplicates_auto_resolution_rest_percentage_idle',
            'duplicates_auto_resolution_work_time_ms_active', 'duplicates_auto_resolution_rest_percentage_active',
            'file_maintenance_during_idle', 'file_maintenance_during_active',
            'file_maintenance_idle_throttle_files', 'file_maintenance_idle_throttle_time_delta',
            'file_maintenance_active_throttle_files', 'file_maintenance_active_throttle_time_delta',
        ]

        def saved():
            out = {}
            for key in saved_keys:
                if key in c.new_options._dictionary['booleans']:
                    out[key] = c.new_options.GetBoolean(key)
                else:
                    out[key] = c.new_options.GetInteger(key)
            return out

        # the held clock
        fake_time = types.SimpleNamespace(**vars(HydrusTime))
        fake_time.GetNow = lambda: int(clock[0])
        fake_time.GetNowFloat = lambda: clock[0]
        fake_time.GetNowPrecise = lambda: clock[0]
        fake_time.TimeHasPassed = lambda t: t is not None and int(clock[0]) > t
        fake_time.TimeHasPassedFloat = lambda t: t is not None and clock[0] > t

        def fake_sleep(seconds):
            events.append(['sleep', round(seconds, 6)])
            clock[0] += seconds
            # (a throttle that never opens: a switch is off)
            run = 0
            for e in reversed(events):
                if e[0] != 'sleep':
                    break
                run += 1
            if run > 400:
                raise Stop()

        class Event:
            def __init__(self, name):
                self.name = name

            def wait(self, seconds):
                events.append(['wait', self.name, round(seconds, 6)])
                clock[0] += seconds

            def clear(self):
                pass

            def set(self):
                pass

            def is_set(self):
                return False

        def single_loop(manager, overrides):
            """The manager's real `_DoSingleLoop`, with the held clock."""
            function = type(manager)._DoSingleLoop
            globals_ = dict(function.__globals__)
            globals_['HydrusTime'] = fake_time
            globals_['time'] = types.SimpleNamespace(sleep=fake_sleep)
            globals_.update(overrides)
            manager._wake_from_work_sleep_event = Event('work')
            manager._wake_from_idle_sleep_event = Event('idle')
            return types.FunctionType(function.__code__, globals_)(manager)

        controller_overrides = {
            'CurrentlyIdle': lambda: idle[0],
            'GoodTimeToStartBackgroundWork': lambda: True,
            'WaitUntilViewFree': lambda: None,
        }
        real_write = c.WriteSynchronous
        real_read = c.Read
        script = {}

        def write(name, *args, **kwargs):
            if name == 'maintain_similar_files_search_for_potential_duplicates':
                period = kwargs['work_period']
                (share, still, done) = script['packet']
                events.append(['work', round(period, 6)])
                clock[0] += period * share
                return (still, done)
            return real_write(name, *args, **kwargs)

        class Media:
            def __init__(self, i):
                self.i = i

            def GetHash(self):
                return bytes([self.i]) * 32

        queue = {}

        def read(name, *args, **kwargs):
            if name == 'file_maintenance_get_jobs':
                # as `GetJobs`: the files with a job of the first type in run
                # order that has any
                for job_type in ClientFilesMaintenance.ALL_REGEN_JOBS_IN_RUN_ORDER:
                    group = {bytes([i]) * 32: [job_type] for (i, job) in queue.items() if job == job_type}
                    if len(group) > 0:
                        return group
                return {}
            if name == 'media_results':
                return [Media(hash[0]) for hash in args[0]]
            return real_read(name, *args, **kwargs)

        numbers = types.SimpleNamespace(instance=lambda: types.SimpleNamespace(RefreshMaintenanceNumbers=lambda: None))

        def similar_pass(packet):
            manager = ClientPotentialDuplicatesManager.PotentialDuplicatesMaintenanceManager(c)
            manager._need_to_rebalance_tree.clear()
            manager._WorkToDo = lambda: True
            script['packet'] = packet
            single_loop(manager, {'PotentialDuplicatesMaintenanceNumbersStore': numbers})

        def auto_pass(packet):
            manager = ClientDuplicatesAutoResolution.DuplicatesAutoResolutionManager(c)
            (share, still, _) = packet

            def work_rules(period):
                events.append(['work', round(period, 6)])
                clock[0] += period * share
                return still

            manager._WorkRules = work_rules
            single_loop(manager, {})

        def files_pass():
            manager = ClientFilesMaintenanceManager.FilesMaintenanceManager(c)

            def run_job(media_results_to_job_types, job_status, job_done_hook=None):
                for (media, jobs) in media_results_to_job_types.items():
                    for job in jobs:
                        events.append(['job', int(clock[0]) - T0, job_names[job]])
                        del queue[media.i]
                        # as `_RunJob`'s `finally` does
                        manager._work_tracker.ReportRequestUsed(num_requests=ClientFilesMaintenance.regen_file_enum_to_job_weight_lookup[job])

            manager._RunJob = run_job
            queue.clear()
            queue.update({i: job_types[job] for (i, job) in enumerate(JOBS)})
            try:
                # loops until one finds no work
                for _ in range(10):
                    single_loop(manager, {})
                    if events[-1][:2] == ['wait', 'idle']:
                        break
            except Stop:
                events.append(['stopped'])

        def squeeze(events):
            """Runs of the same sleep as one ['sleeps', seconds, count]."""
            out = []
            for e in events:
                if e[0] == 'sleep' and len(out) > 0 and out[-1][0] == 'sleeps' and out[-1][1] == e[1]:
                    out[-1][2] += 1
                elif e[0] == 'sleep':
                    out.append(['sleeps', e[1], 1])
                else:
                    out.append(list(e))
            return out

        workers = {'similar': similar_pass, 'auto': auto_pass}
        out = {'t0': T0, 'typed': TYPED, 'packets': PACKETS, 'jobs': JOBS, 'cases': []}
        defaults = {
            'similar': {'idle': (5.0, 50), 'normal': (0.1, 1900)},
            'auto': {'idle': (1.0, 100), 'normal': (0.1, 900)},
            'files': {'idle': (1, 2), 'normal': (1, 20)},
        }
        real_tracker_time = HydrusNetworking.HydrusTime
        for (name, value) in controller_overrides.items():
            setattr(c, name, value)
        c.WriteSynchronous = write
        c.Read = read
        HydrusNetworking.HydrusTime = fake_time
        try:
            out['defaults_saved'] = saved()
            for (label, values) in [('defaults', defaults), ('typed', TYPED)]:
                for switches in [(True, True), (True, False), (False, True)]:
                    type_values(values, switches)
                    case = {'values': label, 'switches': list(switches), 'saved': saved(), 'passes': []}
                    for now_idle in [False, True]:
                        idle[0] = now_idle
                        for worker in ['similar', 'auto']:
                            for packet in PACKETS:
                                events.clear()
                                clock[0] = float(T0)
                                workers[worker](packet)
                                case['passes'].append({'worker': worker, 'idle': now_idle, 'packet': list(packet), 'events': list(events)})
                        events.clear()
                        clock[0] = float(T0)
                        files_pass()
                        case['passes'].append({'worker': 'files', 'idle': now_idle, 'events': squeeze(events)})
                    out['cases'].append(case)
            out['limits'] = 'Each manager\'s real _DoSingleLoop (and the file manager\'s real bandwidth-rule throttle and tracker) runs on a held clock. Idle state, database work, the job queue and _RunJob (reduced to reporting each job\'s weight, as its finally does) are scripted; sleeps and event waits are recorded and move the clock. Work-hard modes are not recorded.'
            return out
        finally:
            HydrusNetworking.HydrusTime = real_tracker_time
            for name in list(controller_overrides) + ['WriteSynchronous', 'Read']:
                try:
                    delattr(c, name)
                except AttributeError:
                    pass
            c.new_options = original_options
            for p in panels:
                p.hide()
                p.deleteLater()

    return session.controller.CallBlockingToQt(session.controller.gui, qt)


def main():
    import hydrus_driver
    import record_api
    if len(sys.argv) > 1:
        output = Path(sys.argv[2])
        result = hydrus_driver.run_client(record_api.unpack_fixture('basic'), record)
        output.write_text(json.dumps(result))
        return
    with tempfile.TemporaryDirectory() as directory:
        result = Path(directory) / 'record.json'
        hydrus_driver.run_in_subprocess(str(Path(__file__).resolve()), '--child', str(result))
        out = json.loads(result.read_text())
    path = HERE / 'fixtures/maintenance_pace.json'
    path.write_text(json.dumps(out, indent=1) + '\n')
    print('wrote ' + str(path))


if __name__ == '__main__':
    main()

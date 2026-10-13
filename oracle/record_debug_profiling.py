#!/usr/bin/env python3
"""Help > debug > profiling, and report modes > db report mode, on the real client.

Real Help-menu QActions are triggered; the popups (`pub('message')`), the
menu items' check states, the profile and query planner logs the reference
writes to its database directory, and the db report mode's popups are read
back. Time is the real clock: the "slow" job is a real 60 ms sleep and the
fast ones return at once. The cProfile text in a slow job's log is Python's;
only its framing is recorded.
"""
import glob
import json
import os
import re
import sys
import tempfile
import time
from pathlib import Path

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))

PREFIX = re.compile(r'^\d{4}-\d\d-\d\d \d\d:\d\d:\d\d: ')
STAMP = r'\d{4}-\d\d-\d\d \d\d-\d\d-\d\d'


def record(session):
    def drive():
        from qtpy import QtTest as T, QtWidgets as W
        from hydrus.core import HydrusProfiling
        from hydrus.core import HydrusGlobals as HG
        controller = session.controller
        gui = controller.gui
        db_dir = controller.GetDBDir()
        shown = []
        original_pub = controller.pub

        def pub(topic, *args, **kwargs):
            if topic == 'message':
                shown.append(args[0].GetStatusText())
            return original_pub(topic, *args, **kwargs)

        def find(menu, labels):
            for action in menu.actions():
                if action.text() == labels[0]:
                    return action if len(labels) == 1 else find(action.menu(), labels[1:])
            raise ValueError(labels)

        def logs(pattern):
            return sorted(os.path.basename(p) for p in glob.glob(os.path.join(db_dir, pattern)))

        def read_log(name):
            return Path(db_dir, name).read_text(encoding='utf-8')

        out = {}
        menu = None
        try:
            controller.pub = pub
            menu, label = gui._InitialiseMenuInfoHelp()
            paths = dict(
                client_api=['debug', 'profiling', 'profile mode (client api)'],
                db=['debug', 'profiling', 'profile mode (db)'],
                threads=['debug', 'profiling', 'profile mode (threads)'],
                ui=['debug', 'profiling', 'profile mode (ui)'],
                planner=['debug', 'profiling', 'query planner mode'],
                db_report=['debug', 'report modes', 'db report mode'])
            actions = {k: find(menu, v) for k, v in paths.items()}
            out['menu_labels'] = [a.text() for a in find(menu, ['debug', 'profiling']).menu().actions()]
            out['menu_checkable'] = {k: actions[k].isCheckable() for k in actions}
            out['menu_tooltips'] = {k: actions[k].toolTip() for k in actions}

            def checks():
                return {k: actions[k].isChecked() for k in ('client_api', 'db', 'threads', 'ui', 'planner')}

            def step(name, key):
                shown.clear()
                actions[key].trigger()
                T.QTest.qWait(50)
                return dict(step=name, messages=list(shown), checked=checks())

            # profile modes: exclusive, flipping the one on turns it off
            sequence = [step('db on', 'db'), step('ui on (ends db)', 'ui'), step('ui off', 'ui'),
                        step('client_api on', 'client_api'), step('threads on (ends client api)', 'threads'),
                        step('threads off', 'threads')]
            out['sequence'] = sequence

            # a db profile with two fast jobs and one slow one
            shown.clear()
            actions['db'].trigger()
            before = set(logs('client profile*'))
            controller.Read('nums_pending')
            controller.WriteSynchronous('null')
            HydrusProfiling.Profile('Profiling test job', lambda: time.sleep(0.06), min_duration_ms=16)
            HydrusProfiling.Profile('Profiling outer job', lambda: HydrusProfiling.Profile(
                'Profiling inner job', lambda: time.sleep(0.03), min_duration_ms=16), min_duration_ms=16)
            created = sorted(set(logs('client profile*')) - before)
            out['profile_log_names'] = created
            out['profile_log_name_pattern'] = r'^client profile \(db\) - %s\.log$' % STAMP
            text = read_log(created[0])
            out['profile_log_has_stats_framing'] = 'Stats' in text and 'Callers' in text
            out['profile_log_lines'] = [PREFIX.sub('<time>: ', l) for l in text.split('\n')
                                         if PREFIX.match(l) or l.startswith('EXCLUSIVE')]
            out['profile_started_with'] = list(shown)
            shown.clear()
            actions['db'].trigger()
            T.QTest.qWait(50)
            out['profile_stopped_with'] = list(shown)
            out['jobs_seen_by_db_profile_summary'] = [l for l in out['profile_log_lines']]

            # query planner
            shown.clear()
            actions['planner'].trigger()
            T.QTest.qWait(50)
            out['planner_on_messages'] = list(shown)
            out['planner_checked_on'] = actions['planner'].isChecked()
            before = set(logs('client query planner*'))
            controller.Read('nums_pending')
            controller.Read('nums_pending')
            controller.Read('services')
            controller.Read('services')
            controller.Read('serialisable_simple', 'pending_logins')
            created = sorted(set(logs('client query planner*')) - before)
            out['planner_log_names'] = created
            out['planner_log_name_pattern'] = r'^client query planner - %s\.log$' % STAMP
            text = read_log(created[0])
            blocks = [b for b in text.split('\n\n') if b]
            out['planner_block_count'] = len(blocks)
            out['planner_blocks_are_unique'] = len(set(b.split('\n', 1)[1] for b in blocks)) == len(blocks)
            out['planner_first_blocks'] = [PREFIX.sub('<time>: ', b) for b in blocks[:6]]
            out['planner_queries_planned'] = len(HydrusProfiling.queries_planned)
            out['planner_query_count'] = HydrusProfiling.query_planner_query_count
            shown.clear()
            actions['planner'].trigger()
            T.QTest.qWait(50)
            out['planner_off_messages'] = list(shown)
            out['planner_checked_off'] = actions['planner'].isChecked()
            out['planner_off_clears_planned'] = len(HydrusProfiling.queries_planned) == 0

            # db report mode
            shown.clear()
            actions['db_report'].trigger()
            controller.Read('nums_pending')
            controller.WriteSynchronous('null')
            T.QTest.qWait(300)
            out['db_report_messages'] = list(shown)
            actions['db_report'].trigger()
            out['db_report_flag_after'] = HG.db_report_mode
            out['min_job_ms'] = dict(db=HG.db_profile_min_job_time_ms, server=HG.server_profile_min_job_time_ms,
                                     menu=HG.menu_profile_min_job_time_ms)
            out['controller_name'] = controller.GetName()
            return out
        finally:
            controller.pub = original_pub
            if HydrusProfiling.profile_mode:
                HydrusProfiling.StopProfileMode()
            if HydrusProfiling.query_planner_mode:
                HydrusProfiling.FlipQueryPlannerMode()
            if menu is not None:
                menu.deleteLater()
            W.QApplication.processEvents()

    return session.controller.CallBlockingToQt(session.controller.gui, drive)


def main():
    import hydrus_driver
    import record_api
    if len(sys.argv) > 1 and sys.argv[1] == '--child':
        database = record_api.unpack_fixture('basic')
        Path(sys.argv[2]).write_text(json.dumps(hydrus_driver.run_client(database, record)))
        return
    with tempfile.TemporaryDirectory() as directory:
        output = Path(directory) / 'result.json'
        hydrus_driver.run_in_subprocess(str(Path(__file__).resolve()), '--child', str(output))
        result = json.loads(output.read_text())
    (HERE / 'fixtures/debug_profiling.json').write_text(json.dumps(result, indent=2) + '\n')
    print('wrote debug_profiling.json')


if __name__ == '__main__':
    main()

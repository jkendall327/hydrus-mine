#!/usr/bin/env python3
"""Record real MigrationJob/Qt popup phases, speed, ownership and dismissal.

Only the eleven-row source/destination and elapsed timing are scripted. The
reference job, status, popup and settings panel implementations are unmodified.
"""
import json
import os
import shutil
import sys
import threading
from unittest.mock import patch
HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
from hydrus_driver import run_client
import record_api


def record(session):
    from hydrus.client import ClientMigration
    from hydrus.client.gui.ClientGUIPopupMessages import PopupMessage
    from hydrus.client.gui.metadata.ClientGUIMigrateTags import MigrateTagsPanel
    from hydrus.core import HydrusConstants as HC, HydrusTime
    controller = session.controller
    records = []
    for action in ('resume', 'cancel'):
        entered, release_cleanup = threading.Event(), threading.Event()
        state = {'batches': 0, 'accepted': [], 'cleanup': [], 'timeline': [], 'speed_inputs': [], 'dismiss_signals': 0, 'finish_dismiss_seconds': []}
        popup = []
        local = controller.services_manager.GetServices((HC.LOCAL_TAG,))[0].GetServiceKey()
        panel = controller.CallBlockingToQt(controller.gui, lambda: MigrateTagsPanel(controller.gui, local))
        controller.CallBlockingToQt(controller.gui, panel.show)

        class Source:
            def Prepare(self): pass
            def StillWorkToDo(self): return len(state['accepted']) < 11
            def GetSomeData(self): return list(range(len(state['accepted']), min(len(state['accepted']) + 3, 11)))
            def CleanUp(self):
                if action == 'cancel': assert release_cleanup.wait(10)
                state['cleanup'].append('source')

        class Destination:
            def Prepare(self): pass
            def DoSomeWork(self, source):
                rows = source.GetSomeData()
                state['accepted'].extend(rows)
                state['batches'] += 1
                if state['batches'] == 1:
                    controller.CallBlockingToQt(controller.gui, popup[0].PausePlay)
                with patch.object(HydrusTime, 'GetNowPrecise', return_value=10.5):
                    speed = ClientMigration.GetBasicSpeedStatement(len(rows), 10.0)
                state['speed_inputs'].append({'rows': len(rows), 'elapsed_ms': 500, 'text': speed})
                return speed
            def CleanUp(self): state['cleanup'].append('destination')

        class Controller:
            def pub(self, topic, status):
                state['status'] = status
                def make():
                    p = PopupMessage(controller.gui, status)
                    p.dismiss.connect(lambda ignored: state.__setitem__('dismiss_signals', state['dismiss_signals'] + 1))
                    return p
                popup.append(controller.CallBlockingToQt(controller.gui, make))
                original_text, original_finish = status.SetStatusText, status.FinishAndDismiss
                def text(value):
                    original_text(value)
                    def update():
                        popup[0].UpdateMessage()
                        state['timeline'].append({'status': value, 'visible': popup[0]._text_1.text()})
                    controller.CallBlockingToQt(controller.gui, update)
                    if state['batches'] == 1 and value.endswith('rows/s'): entered.set()
                def finish(seconds=None):
                    state['finish_dismiss_seconds'].append(seconds)
                    original_finish(seconds)
                status.SetStatusText, status.FinishAndDismiss = text, finish

        thread = threading.Thread(target=ClientMigration.MigrationJob(Controller(), 'migration popup oracle', Source(), Destination()).Run)
        thread.start()
        assert entered.wait(10)
        status = state['status']
        controller.CallBlockingToQt(controller.gui, popup[0].TryToDismiss)
        assert not status.IsDismissed()
        def close_settings():
            panel.close()
            return panel.isHidden()
        settings_closed = controller.CallBlockingToQt(controller.gui, close_settings)
        assert settings_closed and thread.is_alive() and status.IsPaused()
        paused = {'text': controller.CallBlockingToQt(controller.gui, lambda: popup[0]._text_1.text()), 'batches': state['batches'], 'accepted': list(state['accepted']), 'dismissed': status.IsDismissed(), 'settings_closed': settings_closed, 'job_alive': thread.is_alive(), 'title': status.GetStatusTitle()}
        controller.CallBlockingToQt(controller.gui, popup[0].PausePlay if action == 'resume' else popup[0].Cancel)
        cancel_state = None
        if action == 'cancel':
            cancel_state = {'done': status.IsDone(), 'pausable': status.IsPausable(), 'cancellable': status.IsCancellable(), 'paused': status.IsPaused(), 'worker_alive': thread.is_alive()}
            controller.CallBlockingToQt(controller.gui, popup[0].TryToDismiss)
            cancel_state['dismissed'] = status.IsDismissed()
        release_cleanup.set()
        thread.join(10)
        assert not thread.is_alive()
        dismissal = []
        if action == 'resume':
            deadline = status._finish_and_dismiss_time
            for delta in (2, 3, 4):
                # The real regular checker gates calls; force its next due time,
                # while keeping its implementation and strict deadline intact.
                status._cancel_tests_regular_checker._next_check = 0
                with patch.object(HydrusTime, 'GetNow', return_value=deadline - 3 + delta):
                    dismissal.append({'seconds': delta, 'dismissed': status.IsDismissed()})
        records.append({'action': action, 'paused': paused, 'cancel_state': cancel_state, 'batches': state['batches'], 'accepted': state['accepted'], 'cleanup': state['cleanup'], 'timeline': state['timeline'], 'speed_inputs': state['speed_inputs'], 'finish_dismiss_seconds': state['finish_dismiss_seconds'], 'dismissal': dismissal, 'dismiss_signals': state['dismiss_signals'], 'done': status.IsDone(), 'cancelled': status.IsCancelled(), 'text': status.GetStatusText()})
        controller.CallBlockingToQt(controller.gui, popup[0].deleteLater)
        controller.CallBlockingToQt(controller.gui, panel.deleteLater)
    with open(os.path.join(HERE, 'fixtures/tag_migration_progress.json'), 'w') as stream:
        json.dump(records, stream, indent=2)
        stream.write('\n')


if __name__ == '__main__':
    db = record_api.unpack_fixture('basic')
    try:
        run_client(db, record)
    finally:
        shutil.rmtree(db)

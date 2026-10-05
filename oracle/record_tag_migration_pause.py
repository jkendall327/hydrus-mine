#!/usr/bin/env python3
"""Drive actual MigrationJob and Qt PopupMessage pause/resume/cancel controls.

An eleven-entry scripted source with three-entry batches isolates the post-commit pause boundary; all job
status and cleanup behavior is performed by the unmodified reference classes.
"""
import json
import os
import shutil
import sys
import threading
import time
HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
from hydrus_driver import run_client
import record_api


def record(session):
    from hydrus.client import ClientMigration
    from hydrus.client.gui.ClientGUIPopupMessages import PopupMessage
    controller = session.controller
    records = []
    for action in ('resume', 'cancel'):
        entered = threading.Event()
        state = {'batches': 0, 'accepted': [], 'cleanup': []}

        class Source:
            def Prepare(self): pass
            def StillWorkToDo(self): return len(state['accepted']) < 11
            def GetSomeData(self): return list(range(len(state['accepted']), min(len(state['accepted']) + 3, 11)))
            def CleanUp(self): state['cleanup'].append('source')

        class Destination:
            def Prepare(self): pass
            def DoSomeWork(self, source):
                state['accepted'].extend(source.GetSomeData())
                state['batches'] += 1
                if state['batches'] == 1:
                    controller.CallBlockingToQt(controller.gui, popup[0].PausePlay)
                    entered.set()
                return 'committed batch'
            def CleanUp(self): state['cleanup'].append('destination')

        popup = []
        class Controller:
            def pub(self, topic, status):
                state['status'] = status
                popup.append(controller.CallBlockingToQt(controller.gui, lambda: PopupMessage(controller.gui, status)))

        thread = threading.Thread(target=ClientMigration.MigrationJob(Controller(), 'pause oracle', Source(), Destination()).Run)
        thread.start()
        assert entered.wait(10)
        time.sleep(0.25)
        status = state['status']
        paused = {'pausable': status.IsPausable(), 'paused': status.IsPaused(), 'batches': state['batches'], 'accepted': list(state['accepted'])}
        controller.CallBlockingToQt(controller.gui, popup[0].PausePlay if action == 'resume' else popup[0].Cancel)
        thread.join(10)
        assert not thread.is_alive()
        records.append({'action': action, 'paused': paused, 'batches': state['batches'], 'accepted': state['accepted'], 'cancelled': status.IsCancelled(), 'done': status.IsDone(), 'cleanup': state['cleanup'], 'text': status.GetStatusText()})
        controller.CallBlockingToQt(controller.gui, popup[0].deleteLater)
    with open(os.path.join(HERE, 'fixtures/tag_migration_pause.json'), 'w') as stream:
        json.dump(records, stream, indent=2)
        stream.write('\n')


if __name__ == '__main__':
    db = record_api.unpack_fixture('basic')
    try:
        run_client(db, record)
    finally:
        shutil.rmtree(db)

#!/usr/bin/env python3
"""Record real import/export manager pause, wait and finally lifecycles.

Invoke FrameGUI's actual manager methods and run their captured worker on the
recorder thread, retaining the real blocking Qt handoff and real manager panels.
Script the modal Apply/Cancel/error result and simulate an already-running
folder daemon with its real global running flag. Empty private-client folder
lists avoid filesystem work. This records a remaining native workflow boundary,
not a claim that the Rust manager already pauses or waits for workers.
"""
import json
import sys
import tempfile
import threading
from pathlib import Path

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))


def record(session):
    from qtpy import QtWidgets as QW
    from hydrus.client.gui import ClientGUITopLevelWindowsPanels
    from hydrus.core import HydrusGlobals as HG

    controller, gui = session.controller, session.controller.gui
    old_exec = ClientGUITopLevelWindowsPanels.DialogEdit.exec
    old_thread, old_pub = controller.CallToThread, controller.pub
    captured, active, cases = [], {}, []
    keys = ['pause_import_folders_sync', 'pause_export_folders_sync']
    original_pauses = {key: controller.new_options.GetBoolean(key) for key in keys}
    original_running = {kind: getattr(HG, f'{kind}_folders_running') for kind in ['import', 'export']}

    def call_to_thread(function, *args, **kwargs):
        if function.__name__ == 'THREAD_do_it' and any(f'._Manage{kind}Folders.' in function.__qualname__ for kind in ['Import', 'Export']):
            captured.append((function, args, kwargs))
        else:
            return old_thread(function, *args, **kwargs)

    def publish(topic, *args, **kwargs):
        if active and topic == f"notify_new_{active['kind']}_folders":
            active['events'].append({'kind': 'notification', 'topic': topic,
                                     'paused': controller.new_options.GetBoolean(active['key'])})
            return
        if active and topic == 'message' and args and args[0].GetStatusText() == f"Waiting for {active['kind']} folders to finish.":
            active['jobs'].append(args[0])
            active['events'].append({'kind': 'wait', 'text': args[0].GetStatusText(),
                                     'paused': controller.new_options.GetBoolean(active['key'])})
            return
        return old_pub(topic, *args, **kwargs)

    def dialog_exec(dialog):
        if not active or dialog.windowTitle() != f"edit {active['kind']} folders":
            return old_exec(dialog)
        active['events'].append({'kind': 'dialog', 'title': dialog.windowTitle(),
                                 'paused': controller.new_options.GetBoolean(active['key']),
                                 'worker_running': getattr(HG, f"{active['kind']}_folders_running"),
                                 'folder_count': len(dialog._panel.GetValue()),
                                 'answer': active['answer']})
        if active['answer'] == 'exception':
            raise RuntimeError('scripted folder-manager exception')
        return QW.QDialog.DialogCode.Accepted if active['answer'] == 'apply' else QW.QDialog.DialogCode.Rejected

    try:
        controller.CallToThread, controller.pub = call_to_thread, publish
        ClientGUITopLevelWindowsPanels.DialogEdit.exec = dialog_exec
        for kind in ['import', 'export']:
            key, running = f'pause_{kind}_folders_sync', f'{kind}_folders_running'
            for busy in [False, True]:
                for initially_paused in [False, True]:
                    for answer in ['cancel', 'apply', 'exception']:
                        active.update(kind=kind, key=key, answer=answer, events=[], jobs=[])
                        controller.new_options.SetBoolean(key, initially_paused)
                        setattr(HG, running, busy)
                        controller.CallBlockingToQt(gui, getattr(gui, f'_Manage{kind.title()}Folders'))
                        assert len(captured) == 1, captured
                        function, args, kwargs = captured.pop()
                        timer = None
                        if busy:
                            timer = threading.Timer(0.25, setattr, args=(HG, running, False))
                            timer.start()
                        error = None
                        try:
                            function(*args, **kwargs)
                        except RuntimeError as e:
                            error = str(e)
                        finally:
                            if timer is not None:
                                timer.join()
                        case = {'folder_kind': kind, 'initially_paused': initially_paused,
                                      'worker_initially_running': busy, 'answer': answer,
                                      'events': active['events'], 'error': error,
                                      'finally_paused': controller.new_options.GetBoolean(key),
                                      'wait_jobs': [{'done': job.IsDone(), 'dismissed': job.IsDismissed()}
                                                    for job in active['jobs']]}
                        assert case['finally_paused'] == initially_paused, case
                        dialogs = [event for event in case['events'] if event['kind'] == 'dialog']
                        assert len(dialogs) == 1 and dialogs[0]['paused'] and not dialogs[0]['worker_running'], case
                        assert dialogs[0]['folder_count'] == 0, case
                        notifications = [event for event in case['events'] if event['kind'] == 'notification']
                        expected_notifications = (2 if answer == 'apply' else 1) if kind == 'export' else int(answer == 'apply')
                        assert len(notifications) == expected_notifications, case
                        assert case['wait_jobs'] == ([{'done': True, 'dismissed': True}] if busy else []), case
                        assert error == ('scripted folder-manager exception' if answer == 'exception' else None), case
                        cases.append(case)
                        active.clear()
        return {'cases': cases}
    finally:
        controller.CallToThread, controller.pub = old_thread, old_pub
        ClientGUITopLevelWindowsPanels.DialogEdit.exec = old_exec
        for key, paused in original_pauses.items():
            controller.new_options.SetBoolean(key, paused)
        for kind, running in original_running.items():
            setattr(HG, f'{kind}_folders_running', running)


def main():
    import hydrus_driver, record_api
    if len(sys.argv) > 1 and sys.argv[1] == '--child':
        destination = Path(sys.argv[2])
        result = hydrus_driver.run_client(record_api.unpack_fixture('basic'), record)
        destination.write_text(json.dumps(result))
        return
    with tempfile.TemporaryDirectory() as work:
        destination = Path(work) / 'folder-manager-lifecycle.json'
        hydrus_driver.run_in_subprocess(str(Path(__file__).resolve()), '--child', str(destination))
        result = json.loads(destination.read_text())
    (HERE / 'fixtures/folder_manager_lifecycle.json').write_text(json.dumps(result, indent=2) + '\n')


if __name__ == '__main__':
    main()

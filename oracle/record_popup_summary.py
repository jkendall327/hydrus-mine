#!/usr/bin/env python3
"""The popup message manager's summary bar: twelve messages added (the ten
oldest get cards, the bar counts all twelve), the bar's collapse arrow
clicked (the cards' panel hides, the bar and the jobs stay) and clicked
again (the same ten cards show); then one message alone ("1 message").

Usage: QT_QPA_PLATFORM=offscreen ~/pyenv/bin/python oracle/record_popup_summary.py
       (writes fixtures/popup_summary.json)
"""
import json
import sys
import tempfile
import time
from pathlib import Path

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))


def record(session):
    # (the manager's own "initialising" message is gone after a second)
    time.sleep(2)

    def drive():
        from hydrus.client import ClientThreading
        from qtpy import QtWidgets as QW

        manager = session.controller.gui._message_manager

        def cards():
            vbox = manager._message_vbox
            windows = [vbox.itemAt(i).widget() for i in range(vbox.count())]
            return [w for w in windows if w is not None]

        def state(label, jobs):
            return dict(
                label=label,
                cards=[w._job_status.GetStatusText() for w in cards()],
                panel_hidden=manager._message_panel.isHidden(),
                summary=manager._summary_bar._text.text(),
                arrow=manager._summary_bar._expand_collapse.text(),
                jobs=[dict(text=j.GetStatusText(), done=j.IsDone(), dismissed=j.IsDismissed(), cancelled=j.IsCancelled()) for j in jobs],
            )

        jobs = []
        for i in range(12):
            job = ClientThreading.JobStatus()
            job.SetStatusText('m%d' % i)
            job.Finish()
            jobs.append(job)
            manager.AddMessage(job)
        manager._CheckPending()
        QW.QApplication.processEvents()
        states = [state('twelve', jobs)]
        arrow = manager._summary_bar._expand_collapse
        arrow.click()
        QW.QApplication.processEvents()
        states.append(state('collapsed', jobs))
        arrow.click()
        QW.QApplication.processEvents()
        states.append(state('expanded again', jobs))
        for job in jobs:
            job.FinishAndDismiss()
        manager._CheckPending()
        QW.QApplication.processEvents()
        one = ClientThreading.JobStatus()
        one.SetStatusText('alone')
        one.Finish()
        manager.AddMessage(one)
        manager._CheckPending()
        QW.QApplication.processEvents()
        states.append(state('one', [one]))
        one.FinishAndDismiss()
        return dict(states=states)

    return session.controller.CallBlockingToQt(session.controller.gui, drive)


def main():
    import hydrus_driver
    import record_api

    if len(sys.argv) > 1 and sys.argv[1] == '--child':
        Path(sys.argv[2]).write_text(json.dumps(hydrus_driver.run_client(record_api.unpack_fixture('basic'), record)))
        return
    with tempfile.TemporaryDirectory() as directory:
        path = Path(directory) / 'result.json'
        hydrus_driver.run_in_subprocess(str(Path(__file__).resolve()), '--child', str(path))
        result = json.loads(path.read_text())
    (HERE / 'fixtures/popup_summary.json').write_text(json.dumps(result, indent=2, ensure_ascii=False) + '\n')


if __name__ == '__main__':
    main()

#!/usr/bin/env python3
"""Record the network job control inside a reference popup message
(`PopupMessage.UpdateMessage` and `NetworkJobControl._Update`).

A popup whose job status carries a network job shows the job's control: the
first line of its status on the left, what it has read (of the total) and
its speed on the right, a gauge, and a stop button that is enabled while the
job is not done. The control is shown as soon as the popup has a network job,
stays (cleared) for ten seconds after the job goes, then hides.

The real `PopupMessage` and `NetworkJobControl` run; the network job is a
stand-in answering the few questions the control asks of it.

Usage: QT_QPA_PLATFORM=offscreen python oracle/record_popup_network_job.py
       (writes fixtures/popup_network_job.json)
"""
import json
import sys
import tempfile
from pathlib import Path

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))


def record(session):
    def drive():
        from hydrus.client import ClientThreading
        from hydrus.core import HydrusTime
        from hydrus.client.gui import ClientGUIPopupMessages

        gui = session.controller.gui

        class NetworkJob:
            def __init__(self, status, speed, read, to_read, done=False, error=False, engine=True):
                self.status, self.speed, self.read, self.to_read = status, speed, read, to_read
                self.done, self.error, self.engine = done, error, engine

            def GetURL(self):
                return 'https://example.com/file'

            def NoEngineYet(self):
                return not self.engine

            def IsDone(self):
                return self.done

            def HasError(self):
                return self.error

            def GetStatus(self):
                return (self.status, self.speed, self.read, self.to_read)

        job_status = ClientThreading.JobStatus(cancellable=True)
        job_status.SetStatusTitle('a download')
        message = ClientGUIPopupMessages.PopupMessage(gui, job_status)
        control = message._network_job_ctrl
        steps = []

        def look(label, job, now_offset=0):
            if job is not None:
                job_status.SetNetworkJob(job)
            message.UpdateMessage()
            control.TIMERUIUpdate()
            value, range_ = control._gauge.GetValueRange()
            steps.append(dict(
                label=label,
                has_job=job is not None,
                shown=not control.isHidden(),
                left=control._left_text.text(),
                right=control._right_text.text(),
                right_shown=control._right_text.isVisibleTo(control),
                gauge=[value, range_],
                gauge_maximum=control._gauge.maximum(),
                gauge_qt_value=control._gauge.value(),
                stop_enabled=control._cancel_button.isEnabled(),
            ))

        look('popup with no network job', None)
        look('job with no engine yet', NetworkJob('waiting', 0, 0, None, engine=False))
        look('connecting, nothing read', NetworkJob('connecting...\nsecond line', 0, 0, None))
        look('downloading with a known size', NetworkJob('downloading', 12345, 500000, 2000000))
        look('downloading a small file', NetworkJob('downloading', 100, 400, 800))
        look('downloading with no known size', NetworkJob('downloading', 2048, 3000, None))
        look('a quick download', NetworkJob('done!', 4096, 4096, 4096, done=True))
        look('a failed download', NetworkJob('ERROR: 404', 0, 1500, 9000, done=True, error=True))
        # the job goes
        job_status.DeleteVariable('network_job')
        message.UpdateMessage()
        control.TIMERUIUpdate()
        steps.append(dict(
            label='the job goes: cleared at once, still shown',
            has_job=False,
            shown=not control.isHidden(),
            left=control._left_text.text(),
            right=control._right_text.text(),
            gauge=list(control._gauge.GetValueRange()),
            stop_enabled=control._cancel_button.isEnabled(),
        ))
        message._time_network_job_disappeared = HydrusTime.GetNow() - 9
        message.UpdateMessage()
        steps.append(dict(label='nine seconds on: still shown', shown=not control.isHidden()))
        message._time_network_job_disappeared = HydrusTime.GetNow() - 11
        message.UpdateMessage()
        steps.append(dict(label='eleven seconds on: hidden', shown=not control.isHidden()))
        message.deleteLater()
        return steps

    return session.controller.CallBlockingToQt(session.controller.gui, drive)


def main():
    import hydrus_driver
    import record_api
    if len(sys.argv) > 1 and sys.argv[1] == '--child':
        Path(sys.argv[2]).write_text(json.dumps(hydrus_driver.run_client(record_api.unpack_fixture('basic'), record)))
        return
    with tempfile.TemporaryDirectory() as d:
        output = Path(d) / 'result.json'
        hydrus_driver.run_in_subprocess(str(Path(__file__).resolve()), '--child', str(output))
        result = json.loads(output.read_text())
    (HERE / 'fixtures/popup_network_job.json').write_text(json.dumps(result, indent=1) + '\n')
    print('wrote popup_network_job.json')


if __name__ == '__main__':
    main()

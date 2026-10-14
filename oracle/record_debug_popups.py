#!/usr/bin/env python3
"""Help > debug > gui actions > make some popups, and the two modal popups.

The real Help menu's QActions are triggered. Every `pub('message')` and
`pub('modal_message')` the reference makes is read back as it is published
(title, texts, gauges, what the job can do, files, error, who made it and
how many milliseconds after the click), then followed for as long as it runs,
and the Python sleeps and `CallLater`s are the real ones. The modal ones are
followed each 100 ms: their texts and gauges as the countdown goes, then the
same cancelled in the middle.
"""
import json
import sys
import tempfile
import time
from pathlib import Path

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))


def describe(job):
    files = job.GetFiles()
    callable_ = job.GetUserCallable()
    return dict(
        title=job.GetStatusTitle(), text_1=job.GetStatusText(1), text_2=job.GetStatusText(2),
        gauge_1=job.GetGauge(1), gauge_2=job.GetGauge(2),
        pausable=job.IsPausable(), cancellable=job.IsCancellable(), done=job.IsDone(),
        had_error=job.HadError(), has_traceback=job.GetTraceback() is not None,
        traceback_first_lines=(job.GetTraceback() or '').split('\n')[:1],
        files=None if files is None else dict(count=len(files[0]), label=files[1]),
        mergable=job.GetIfHasVariable('attached_files_mergable'),
        user_callable_label=None if callable_ is None else callable_.GetLabel(),
        has_network_job=job.GetNetworkJob() is not None)


def record(session):
    def drive():
        from qtpy import QtTest as T, QtWidgets as W
        controller = session.controller
        gui = controller.gui
        events = []
        original_pub = controller.pub
        started = [time.monotonic()]

        def pub(topic, *args, **kwargs):
            if topic in ('message', 'modal_message'):
                job = args[0]
                events.append(dict(topic=topic, at_ms=round((time.monotonic() - started[0]) * 1000),
                                   job=describe(job), _job=job))
            return original_pub(topic, *args, **kwargs)

        def find(menu, labels):
            for action in menu.actions():
                if action.text() == labels[0]:
                    return action if len(labels) == 1 else find(action.menu(), labels[1:])
            raise ValueError(labels)

        def wait_ms(ms):
            end = time.monotonic() + ms / 1000
            while time.monotonic() < end:
                T.QTest.qWait(20)

        menu = None
        out = {}
        try:
            controller.pub = pub
            menu, label = gui._InitialiseMenuInfoHelp()

            # make some popups
            started[0] = time.monotonic()
            find(menu, ['debug', 'gui actions', 'make some popups']).trigger()
            wait_ms(2600)
            out['some_popups'] = [{k: v for k, v in e.items() if k != '_job'} for e in events]
            out['some_popups_two_seconds_on'] = [
                dict(index=i, text_2=e['_job'].GetStatusText(2), gauge_2=e['_job'].GetGauge(2))
                for i, e in enumerate(events) if e['_job'].GetStatusTitle() == 'test job']
            for e in events:
                e['_job'].FinishAndDismiss()
            events.clear()

            # the modal popups
            for name, cancellable, cancel_at in [('cancellable', True, None), ('non_cancellable', False, None),
                                                 ('cancellable_cancelled', True, 7500)]:
                events.clear()
                started[0] = time.monotonic()
                entry = 'make a modal popup in five seconds' if cancellable else 'make a non-cancellable modal popup in five seconds'
                find(menu, ['debug', 'gui actions', entry]).trigger()
                samples = []
                job = None
                finish = None
                cancelled_sent = False
                while (time.monotonic() - started[0]) < 16.5:
                    T.QTest.qWait(100)
                    t = round((time.monotonic() - started[0]) * 1000)
                    if job is None and events:
                        job = events[0]['_job']
                    if job is not None:
                        if cancel_at is not None and not cancelled_sent and t >= cancel_at:
                            job.Cancel()
                            cancelled_sent = True
                        samples.append(dict(at_ms=t, text_1=job.GetStatusText(), gauge_1=job.GetGauge(),
                                            done=job.IsDone(), dismissed=job.IsDismissed(),
                                            cancelled=job.IsCancelled()))
                        if job.IsDone() and finish is None:
                            finish = t
                            if cancellable and cancel_at is None:
                                pass
                        if job.IsDismissed() and not cancellable or (job.IsDismissed() and cancel_at is None):
                            break
                out[name] = dict(events=[{k: v for k, v in e.items() if k != '_job'} for e in events],
                                 cancel_at_ms=cancel_at, finish_ms=finish,
                                 samples=samples[::5] + samples[-1:])
                for e in events:
                    e['_job'].FinishAndDismiss()
                # (the countdown thread runs to the end before the next one starts)
                wait_ms(max(0, 16500 - (time.monotonic() - started[0]) * 1000))
            out['menu_path'] = [label.replace('&', ''), 'debug', 'gui actions']
            return out
        finally:
            controller.pub = original_pub
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
    (HERE / 'fixtures/debug_popups.json').write_text(json.dumps(result, indent=2) + '\n')
    print('wrote debug_popups.json')


if __name__ == '__main__':
    main()

#!/usr/bin/env python3
"""The popup card's content and traceback controls on private jobs: what
each of the card's labels, gauges and buttons shows for a job with a title,
two texts and two gauges (determinate and pulsing), for the same job paused
and resumed, for text past the card's cutoff, and for a job with a
traceback (the show/hide button, the text it reveals, the copied text).

Usage: QT_QPA_PLATFORM=offscreen ~/pyenv/bin/python oracle/record_popup_content.py
       (writes fixtures/popup_content.json)
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
        from hydrus.client.gui import ClientGUIPopupMessages as P
        from qtpy import QtWidgets as W

        c = session.controller
        published = []
        panels = []
        original = c.pub

        def publish(topic, *args, **kwargs):
            if topic == 'clipboard':
                published.append(list(args))
            else:
                original(topic, *args, **kwargs)

        c.pub = publish

        def gauge(g):
            if g.isHidden():
                return None
            value, range_ = g.GetValueRange()
            return dict(value=value, range=range_, pulsing=g._is_pulsing)

        def snapshot(label, p, j):
            return dict(
                label=label,
                title_hidden=p._title.isHidden(),
                title=p._title.text(),
                text_1_hidden=p._text_1.isHidden(),
                text_1=p._text_1.text(),
                gauge_1=gauge(p._gauge_1),
                text_2_hidden=p._text_2.isHidden(),
                text_2=p._text_2.text(),
                gauge_2=gauge(p._gauge_2),
                show_tb_hidden=p._show_tb_button.isHidden(),
                show_tb_label=p._show_tb_button.text(),
                tb_text_hidden=p._tb_text.isHidden(),
                tb_text=p._tb_text.text(),
                copy_tb_hidden=p._copy_tb_button.isHidden(),
                copy_tb_label=p._copy_tb_button.text(),
                pause_hidden=p._pause_button.isHidden(),
                cancel_hidden=p._cancel_button.isHidden(),
                published=list(published),
            )

        states = []

        def new(j):
            p = P.PopupMessage(c.gui, j)
            panels.append(p)
            p.UpdateMessage()
            return p

        try:
            # nothing set
            j = ClientThreading.JobStatus(pausable=True, cancellable=True)
            p = new(j)
            states.append(snapshot('empty', p, j))
            # the pieces on their own
            j = ClientThreading.JobStatus()
            j.SetStatusTitle('only a title')
            p = new(j)
            states.append(snapshot('title only', p, j))
            j = ClientThreading.JobStatus()
            j.SetStatusText('only text')
            p = new(j)
            states.append(snapshot('text 1 only', p, j))
            j = ClientThreading.JobStatus()
            j.SetStatusText('only second text', 2)
            p = new(j)
            states.append(snapshot('text 2 only', p, j))
            j = ClientThreading.JobStatus()
            j.SetGauge(1, 4)
            p = new(j)
            states.append(snapshot('gauge 1 only', p, j))
            # all of it, then paused, then played
            j = ClientThreading.JobStatus(pausable=True, cancellable=True)
            j.SetStatusTitle('a title')
            j.SetStatusText('first')
            j.SetStatusText('second', 2)
            j.SetGauge(1, 4)
            j.SetGauge(0, 0, 2)
            p = new(j)
            states.append(snapshot('all', p, j))
            p.PausePlay()
            p.UpdateMessage()
            states.append(snapshot('paused', p, j))
            p.PausePlay()
            p.UpdateMessage()
            states.append(snapshot('played', p, j))
            # the second gauge with a range, the first without
            j = ClientThreading.JobStatus()
            j.SetGauge(None, None)
            j.SetGauge(3, 8, 2)
            p = new(j)
            states.append(snapshot('pulsing first, ranged second', p, j))
            # text past the cutoff
            j = ClientThreading.JobStatus()
            j.SetStatusText('x' * 1500)
            j.SetStatusText('y' * 1500, 2)
            p = new(j)
            states.append(snapshot('long texts', p, j))
            # a traceback: shown by its button, hidden again, copied
            j = ClientThreading.JobStatus()
            j.SetStatusTitle('oh no')
            j.SetStatusText('it broke')
            trace = 'trace line 1\ntrace line 2\n' + 'z' * 2000
            j.SetVariable('traceback', trace)
            p = new(j)
            states.append(snapshot('traceback', p, j))
            p._show_tb_button.click()
            states.append(snapshot('traceback shown', p, j))
            p._show_tb_button.click()
            states.append(snapshot('traceback hidden again', p, j))
            p._copy_tb_button.click()
            states.append(snapshot('traceback copied', p, j))
            j2 = ClientThreading.JobStatus()
            j2.SetStatusTitle('oh no')
            j2.SetStatusText('it broke')
            j2.SetVariable('traceback', 'short trace\nline two')
            p2 = new(j2)
            states.append(snapshot('short traceback', p2, j2))
            p2._copy_tb_button.click()
            states.append(snapshot('short traceback copied', p2, j2))
            return dict(
                traceback=trace,
                short_traceback='short trace\nline two',
                short_traceback_string=j2.ToString(),
                states=states,
            )
        finally:
            c.pub = original
            for p in panels:
                p.deleteLater()
            W.QApplication.processEvents()

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
    (HERE / 'fixtures/popup_content.json').write_text(json.dumps(result, indent=2, ensure_ascii=False) + '\n')


if __name__ == '__main__':
    main()

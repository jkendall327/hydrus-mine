#!/usr/bin/env python3
"""Real PopupPanel staging/bounds and PopupMessage construction-time width policy.

Synthetic local job text only. Measures actual Qt short/long/gauge cards, preserves
an existing card across Apply, and constructs a successor from saved preferences.
"""
import json
import sys
import tempfile
from pathlib import Path

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))


def record(session):
    def drive():
        from qtpy import QtCore as QC, QtWidgets as QW, QtTest as QT
        from hydrus.core import HydrusSerialisable
        from hydrus.client import ClientThreading
        from hydrus.client.gui import ClientGUIFunctions, ClientGUIPopupMessages
        from hydrus.client.gui.panels.options.PopupPanel import PopupPanel

        c = session.controller
        original = c.new_options.Duplicate()
        widgets = []

        def prefs(o):
            return dict(characters=o.GetInteger('popup_message_character_width'), fixed=o.GetBoolean('popup_message_force_min_width'))

        def panel():
            p = PopupPanel(c.gui, c.new_options)
            widgets.append(p)
            return p

        def controls(p):
            return dict(characters=p._popup_message_character_width.value(), fixed=p._popup_message_force_min_width.isChecked())

        def card(text, gauge=False):
            job = ClientThreading.JobStatus()
            job.SetStatusText(text)
            if gauge:
                job.SetVariable('popup_gauge_1', (1, 4))
            host = QW.QDialog(c.gui)
            layout = QW.QVBoxLayout(host)
            layout.setSizeConstraint(QW.QLayout.SizeConstraint.SetFixedSize)
            popup = ClientGUIPopupMessages.PopupMessage(host, job)
            layout.addWidget(popup)
            widgets.append(host)
            popup.UpdateMessage()
            host.show()
            QT.QTest.qWait(10)
            return host, popup, job

        def measurement(p):
            return dict(minimum=p.minimumWidth(), maximum=p.maximumWidth(), width=p.width(), hint=p.sizeHint().width(), title_maximum=p._title.maximumWidth(), text_maximum=p._text_1.maximumWidth(), gauge_minimum=p._gauge_1.minimumWidth())

        try:
            defaults = prefs(c.new_options)
            events = []
            for chars, fixed in [(16, False), (56, False), (256, False), (16, True), (80, True), (256, True), (0, False), (999, True)]:
                p = panel()
                before = prefs(c.new_options)
                p._popup_message_character_width.setValue(chars)
                p._popup_message_force_min_width.setChecked(fixed)
                draft = controls(p)
                staged = prefs(c.new_options)
                p.UpdateOptions()
                saved = prefs(c.new_options)
                encoded = HydrusSerialisable.CreateFromSerialisableTuple(c.new_options.GetSerialisableTuple())
                samples = []
                for text, gauge in [('short', False), ('synthetic words ' * 40, False), ('short', True)]:
                    host, popup, job = card(text, gauge)
                    samples.append(dict(text=text, gauge=gauge, expected_cap=ClientGUIFunctions.ConvertTextToPixelWidth(popup._title, saved['characters']), **measurement(popup)))
                    host.hide()
                events.append(dict(input=[chars, fixed], before=before, staged=staged, draft=draft, saved=saved, round_trip=prefs(encoded), reopened=controls(panel()), samples=samples))
            loaded = []
            for chars in [-1, 0, 15, 257, 999]:
                c.new_options.SetInteger('popup_message_character_width', chars)
                p = panel()
                before = prefs(c.new_options)
                shown = controls(p)
                p.UpdateOptions()
                loaded.append(dict(before=before, shown=shown, saved=prefs(c.new_options)))
            c.new_options.SetInteger('popup_message_character_width', 56)
            c.new_options.SetBoolean('popup_message_force_min_width', False)
            host, old, job = card('short')
            old_before = measurement(old)
            p = panel()
            p._popup_message_character_width.setValue(100)
            p._popup_message_force_min_width.setChecked(True)
            cancel_before = prefs(c.new_options)
            cancel_draft = controls(p)
            p.hide()
            cancel_after = prefs(c.new_options)
            p.UpdateOptions()
            job.SetStatusText('updated synthetic words ' * 40)
            old.UpdateMessage()
            host.adjustSize()
            QT.QTest.qWait(10)
            _, successor, _ = card('short')
            live = dict(before=old_before, after=measurement(old), successor=measurement(successor), saved=prefs(c.new_options))
            host.grab().save(str(HERE / 'fixtures/popup_width_reference.png'))
            # Real queue admission constructs the pending eleventh card only
            # after one visible card is dismissed. Existing cards keep caps.
            c.new_options.SetInteger('popup_message_character_width', 56)
            c.new_options.SetBoolean('popup_message_force_min_width', False)
            queue = ClientGUIPopupMessages.JobStatusPopupQueue()
            manager_host = QW.QDialog(c.gui)
            widgets.append(manager_host)
            manager_host.resize(1600, 1000)
            manager_host.show()
            manager = ClientGUIPopupMessages.PopupMessageManager(manager_host, queue)
            manager._update_job.Cancel()
            try:
                for index in range(11):
                    queued = ClientThreading.JobStatus()
                    queued.SetStatusText(f'synthetic popup {index}')
                    queued.Finish()
                    queue.AddJobStatus(queued)
                manager._CheckPending()
                def rows():
                    return [dict(text=manager._message_vbox.itemAt(i).widget().GetJobStatus().GetStatusText(), **measurement(manager._message_vbox.itemAt(i).widget())) for i in range(manager._message_vbox.count())]
                pending_before = rows()
                c.new_options.SetInteger('popup_message_character_width', 80)
                c.new_options.SetBoolean('popup_message_force_min_width', True)
                manager.Dismiss(manager._message_vbox.itemAt(0).widget())
                pending_after = rows()
                pending = dict(before=pending_before, after=pending_after, count=queue.GetCount(), in_view=queue.GetInViewCount())
            finally:
                manager.CleanBeforeDestroy()
                manager_host.hide()
            return dict(defaults=defaults, range=[16, 256], events=events, loaded=loaded, cancel=dict(before=cancel_before, draft=cancel_draft, after=cancel_after), live=live, pending=pending)
        finally:
            c.new_options = original
            for widget in widgets:
                widget.hide()
                widget.deleteLater()

    return session.controller.CallBlockingToQt(session.controller.gui, drive)


def main():
    import hydrus_driver
    import record_api
    if len(sys.argv) > 1 and sys.argv[1] == '--child':
        Path(sys.argv[2]).write_text(json.dumps(hydrus_driver.run_client(record_api.unpack_fixture('basic'), record)))
        return
    with tempfile.TemporaryDirectory() as directory:
        output = Path(directory) / 'result.json'
        hydrus_driver.run_in_subprocess(str(Path(__file__).resolve()), '--child', str(output))
        result = json.loads(output.read_text())
    (HERE / 'fixtures/popup_width.json').write_text(json.dumps(result, indent=2) + '\n')
    print('wrote popup_width.json')


if __name__ == '__main__':
    main()

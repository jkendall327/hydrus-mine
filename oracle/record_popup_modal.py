#!/usr/bin/env python3
"""Record the reference's modal popup lifecycle (`FrameGUI.AddModalMessage` and
`PopupMessageDialogPanel`).

(A job that is neither pausable nor cancellable is done from the start.)

A job published as `modal_message` is thrown up in a dialog titled with the
job's title ("important job" without one), with a close button only if the job
can be cancelled. It waits (in `_pending_modal_job_statuses`, retried one at a
time by the page update tick) while the main window is minimised or hidden to
the tray, another dialog is open, or the main window is not active; a job that
is already done goes straight to the ordinary popups, and a dismissed one
nowhere. Closing the dialog of a job that is done releases it to the popups;
of one that can be cancelled asks "Cancel/stop job?" (yes cancels it and
releases it); of one that cannot, warns that it can't be cancelled and stays.
The dialog closes itself when the job finishes. A panel made to hide the main
GUI hides the other windows (the main one too) until the job is released.

The real `AddModalMessage`, panel and job status run; only the modal event loop
(`DialogNullipotent.exec`), the two answer dialogs, the window-activity probes
and `pub` are scripted, and each case's dialog events are recorded.

Usage: QT_QPA_PLATFORM=offscreen python oracle/record_popup_modal.py
       (writes fixtures/popup_modal.json)
"""
import json
import sys
import tempfile
from pathlib import Path

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))

CANCEL_QUESTION = 'Cancel/stop job?'


def record(session):
    def drive():
        from qtpy import QtWidgets as QW
        from hydrus.client import ClientThreading
        from hydrus.client.gui import ClientGUIDialogsMessage, ClientGUIDialogsQuick, ClientGUIFunctions
        from hydrus.client.gui import ClientGUITopLevelWindows, ClientGUITopLevelWindowsPanels as P

        c = session.controller
        gui = c.gui
        cases = []

        state = dict(minimised=False, hidden_to_tray=False, dialog_open=False, active=True)
        pubs = []
        events = []
        script = {'steps': None, 'answers': []}
        dialogs = []

        real_pub = c.pub

        def pub(topic, *args, **kwargs):
            if topic in ('message', 'pause_all_media', 'modal_message'):
                pubs.append(topic)
                if topic == 'modal_message':
                    return real_pub(topic, *args, **kwargs)
                return None
            return real_pub(topic, *args, **kwargs)

        real_exec = P.DialogNullipotent.exec
        real_init = P.DialogNullipotent.__init__
        real_yes_no = ClientGUIDialogsQuick.GetYesNo
        real_warning = ClientGUIDialogsMessage.ShowWarning
        real_dialog_open = ClientGUIFunctions.DialogIsOpen
        real_active = ClientGUIFunctions.TLWOrChildIsActive
        real_minimised = gui.isMinimized

        def init(self, parent, title, hide_buttons=False, **kwargs):
            real_init(self, parent, title, hide_buttons=hide_buttons, **kwargs)
            dialogs.append(dict(title=title, hide_buttons=hide_buttons))

        def exec_(self):
            dialogs[-1]['panel_type'] = type(self._panel).__name__
            self.setModal(True)
            if script['steps'] is not None:
                script['steps'](self, self._panel)
            return 0

        def yes_no(win, message, *args, **kwargs):
            events.append(dict(event='asked', text=message))
            return QW.QDialog.DialogCode.Accepted if script['answers'].pop(0) else QW.QDialog.DialogCode.Rejected

        def warning(win, message, *args, **kwargs):
            events.append(dict(event='warned', text=message))

        P.DialogNullipotent.__init__ = init
        P.DialogNullipotent.exec = exec_
        ClientGUIDialogsQuick.GetYesNo = yes_no
        ClientGUIDialogsMessage.ShowWarning = warning
        ClientGUIFunctions.DialogIsOpen = lambda: state['dialog_open']
        ClientGUIFunctions.TLWOrChildIsActive = lambda win: state['active']
        gui.isMinimized = lambda: state['minimised']
        c.pub = pub

        def reset(**kwargs):
            state.update(dict(minimised=False, hidden_to_tray=False, dialog_open=False, active=True))
            state.update(kwargs)
            gui._currently_hidden_to_system_tray = state['hidden_to_tray']
            gui._pending_modal_job_statuses = set()
            pubs.clear()
            events.clear()
            dialogs.clear()
            script['steps'] = None
            script['answers'] = []

        def job(title='a long job', cancellable=True, pausable=False, done=False, dismissed=False):
            j = ClientThreading.JobStatus(pausable=pausable, cancellable=cancellable)
            if title is not None:
                j.SetStatusTitle(title)
            if done:
                j.FinishAndDismiss() if dismissed else j.Finish()
            elif dismissed:
                j.FinishAndDismiss()
            return j

        def finish(label, j, extra=None):
            cases.append(dict(
                label=label,
                dialogs=list(dialogs),
                pubs=list(pubs),
                events=list(events),
                pending=len(gui._pending_modal_job_statuses),
                job=dict(cancelled=j.IsCancelled(), done=j.IsDone(), dismissed=j.IsDismissed()),
                **(extra or {}),
            ))

        try:
            # whether the dialog is thrown up at all
            for label, kwargs, j_kwargs in [
                ('active, cancellable job with a title', {}, {}),
                ('active, job with no title', {}, dict(title=None)),
                ('active, pausable job that cannot be cancelled', {}, dict(cancellable=False, pausable=True)),
                ('active, job that is neither (done from the start)', {}, dict(cancellable=False)),
                ('job already done goes to the popups', {}, dict(done=True)),
                ('dismissed job is dropped', {}, dict(dismissed=True)),
                ('main window minimised: pending', dict(minimised=True), {}),
                ('hidden to the tray: pending', dict(hidden_to_tray=True), {}),
                ('another dialog open: pending', dict(dialog_open=True), {}),
                ('main window not active: pending', dict(active=False), {}),
            ]:
                reset(**kwargs)
                j = job(**j_kwargs)
                gui.AddModalMessage(j)
                finish(label, j)

            # a pending job is retried, one per page update, once things are quiet
            reset(minimised=True)
            first, second = job(title='first'), job(title='second')
            gui.AddModalMessage(first)
            gui.AddModalMessage(second)
            at_start = len(gui._pending_modal_job_statuses)
            state['minimised'] = False
            gui.REPEATINGPageUpdate()
            after_one = len(gui._pending_modal_job_statuses)
            gui.REPEATINGPageUpdate()
            cases.append(dict(
                label='pending jobs are retried one per page update',
                pending_at_start=at_start, pending_after_one=after_one,
                pending_after_two=len(gui._pending_modal_job_statuses),
                dialogs=list(dialogs), pubs=list(pubs),
            ))

            # the dialog's own lifecycle
            def closing(answers, expect_job_state=None):
                def steps(dlg, panel):
                    script['answers'] = list(answers)
                    result = panel.UserIsOKToCancel()
                    events.append(dict(event='close_attempt', allowed=bool(result)))
                    if expect_job_state:
                        expect_job_state(dlg, panel)
                return steps

            for label, j_kwargs, answers in [
                ('close a running cancellable job, answer yes', dict(), [True]),
                ('close a running cancellable job, answer no', dict(), [False]),
                ('close a running job that cannot be cancelled', dict(cancellable=False, pausable=True), []),
            ]:
                reset()
                j = ClientThreading.JobStatus(pausable=j_kwargs.get('pausable', False), cancellable=j_kwargs.get('cancellable', True))
                j.SetStatusTitle('a long job')
                script['steps'] = closing(answers)
                gui.AddModalMessage(j)
                finish(label, j)

            # the job finishes while the dialog is up
            reset()
            j = ClientThreading.JobStatus(cancellable=True)
            j.SetStatusTitle('a long job')

            def finishes(dlg, panel):
                j.SetStatusText('half way')
                panel.REPEATINGUpdate()
                events.append(dict(event='updated', text=j.GetStatusText(), emitted_ok=False))
                got = []
                panel.okSignal.connect(lambda: got.append(1))
                j.Finish()
                panel.REPEATINGUpdate()
                events.append(dict(event='job_finished', ok_signal_emitted=bool(got)))
                events.append(dict(event='close_attempt', allowed=bool(panel.UserIsOKToOK())))

            script['steps'] = finishes
            gui.AddModalMessage(j)
            finish('the dialog closes itself when the job finishes', j)

            # no ok signal while a yes/no is open
            reset()
            j = ClientThreading.JobStatus(cancellable=True)

            def finishes_while_asking(dlg, panel):
                got = []
                panel.okSignal.connect(lambda: got.append(1))
                panel._yesno_open = True
                j.Finish()
                panel.REPEATINGUpdate()
                events.append(dict(event='finished_while_asking', ok_signal_emitted=bool(got)))
                panel._yesno_open = False

            script['steps'] = finishes_while_asking
            gui.AddModalMessage(j)
            finish('no ok signal while a question is open', j)

            # hiding the other windows
            extra = ClientGUITopLevelWindows.Frame(gui, 'a review window')
            extra.show()
            for label, hide_main in [('other windows hidden, main kept', False), ('main window hidden too', True)]:
                reset()
                j = ClientThreading.JobStatus(cancellable=True)
                seen = {}

                def make_panel(parent, j=j, hide_main=hide_main, seen=seen):
                    from hydrus.client.gui import ClientGUIPopupMessages
                    dlg = P.DialogNullipotent(None, 'migrating files')
                    panel = ClientGUIPopupMessages.PopupMessageDialogPanel(dlg, j, hide_main_gui=hide_main)
                    seen['hidden_during'] = dict(main=not gui.isVisible(), extra=not extra.isVisible())
                    script['answers'] = [True]
                    panel.UserIsOKToCancel()
                    seen['restored_after'] = dict(main=gui.isVisible(), extra=extra.isVisible())
                    panel._update_job.Cancel()
                    return seen

                make_panel(None)
                cases.append(dict(label=label, hide_main_gui=hide_main, **seen, pubs=list(pubs)))
                gui.show()
                extra.show()
            extra.close()
            return cases
        finally:
            P.DialogNullipotent.__init__ = real_init
            P.DialogNullipotent.exec = real_exec
            ClientGUIDialogsQuick.GetYesNo = real_yes_no
            ClientGUIDialogsMessage.ShowWarning = real_warning
            ClientGUIFunctions.DialogIsOpen = real_dialog_open
            ClientGUIFunctions.TLWOrChildIsActive = real_active
            gui.isMinimized = real_minimised
            c.pub = real_pub
            gui._pending_modal_job_statuses = set()

    return dict(cancel_question=CANCEL_QUESTION, cases=session.controller.CallBlockingToQt(session.controller.gui, drive))


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
    (HERE / 'fixtures/popup_modal.json').write_text(json.dumps(result, indent=1) + '\n')
    print('wrote popup_modal.json')


if __name__ == '__main__':
    main()

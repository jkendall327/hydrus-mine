#!/usr/bin/env python3
"""Real PopupPanel and PopupMessageManager hidden/minimized freeze semantics.

Private real Qt window/queue, unchanged AddMessage/REPEATINGUpdate handlers.
The constructor's three background timers are captured instead of scheduled so
manual ticks/held time are deterministic; no manager method or state signal is
replaced. The private expiring job regular checker is made due at each held-time
observation; the global floating scheduling clock is unmodified. Actual
show/hide/setWindowState drives Qt visibility/minimization.
Other-monitor policy is disabled and not evaluated as platform evidence.
"""
import json
import sys
import tempfile
from pathlib import Path
HERE=Path(__file__).resolve().parent
sys.path.insert(0,str(HERE))
KEY='freeze_message_manager_when_main_gui_minimised'

def record(session):
    def drive():
        from qtpy import QtCore as C, QtWidgets as W
        from hydrus.client import ClientOptions, ClientThreading
        from hydrus.client.gui import ClientGUIPopupMessages as P
        from hydrus.client.gui.panels.options.PopupPanel import PopupPanel
        from hydrus.core import HydrusTime
        c=session.controller
        original_options=c.new_options
        original_now=HydrusTime.GetNow
        original_repeat=c.CallRepeatingQtSafe
        original_later=c.CallLaterQtSafe
        held=[100]
        timers=[]
        class Captured:
            def Cancel(self):pass
        def repeat(owner,*args):
            timers.append(dict(kind='repeating',description=args[2],delay=args[0],period=args[1]))
            return Captured()
        def later(owner,*args):
            timers.append(dict(kind='single',description=args[1],delay=args[0]))
            return Captured()
        windows=[]; manager=None
        out=dict(default=ClientOptions.ClientOptions().GetBoolean(KEY),label='Freeze the popup toaster when the main gui is minimised: ',options=[],states=[])
        try:
            c.new_options=original_options.Duplicate()
            c.new_options.SetBoolean('freeze_message_manager_when_mouse_on_other_monitor',False)
            HydrusTime.GetNow=lambda:held[0]
            panel=PopupPanel(c.gui,c.new_options.Duplicate());windows.append(panel)
            out['options'].append(dict(case='opened',checked=panel._freeze_message_manager_when_main_gui_minimised.isChecked(),saved=c.new_options.GetBoolean(KEY)))
            panel._freeze_message_manager_when_main_gui_minimised.setChecked(True)
            out['options'].append(dict(case='edited',checked=panel._freeze_message_manager_when_main_gui_minimised.isChecked(),saved=c.new_options.GetBoolean(KEY)))
            cancelled=PopupPanel(c.gui,c.new_options.Duplicate());windows.append(cancelled)
            out['options'].append(dict(case='cancelled_reopen',checked=cancelled._freeze_message_manager_when_main_gui_minimised.isChecked(),saved=c.new_options.GetBoolean(KEY)))
            panel.UpdateOptions()
            out['options'].append(dict(case='draft_apply',checked=panel._new_options.GetBoolean(KEY),saved=c.new_options.GetBoolean(KEY)))
            c.new_options=panel._new_options
            reopened=PopupPanel(c.gui,c.new_options.Duplicate());windows.append(reopened)
            out['options'].append(dict(case='applied_reopen',checked=reopened._freeze_message_manager_when_main_gui_minimised.isChecked(),saved=c.new_options.GetBoolean(KEY)))
            reopened.resize(950,400);reopened.show();W.QApplication.processEvents()
            reopened.grab().save(str(HERE/'fixtures/popup-freeze-options-qt.png'))
            out['legacy']=c.new_options.GetSerialisableTuple()
            owner=W.QWidget();windows.append(owner);owner.resize(900,700);owner.show()
            c.CallRepeatingQtSafe,c.CallLaterQtSafe=repeat,later
            queue=P.JobStatusPopupQueue();manager=P.PopupMessageManager(owner,queue)
            c.CallRepeatingQtSafe,c.CallLaterQtSafe=original_repeat,original_later
            running=ClientThreading.JobStatus(pausable=True,cancellable=True);running.SetStatusText('working before freeze')
            expiring=ClientThreading.JobStatus();expiring.SetStatusText('expires while frozen');expiring.FinishAndDismiss(2)
            pending=ClientThreading.JobStatus();pending.SetStatusText('queued while frozen');pending.Finish()
            def state(name,tick=True):
                # Keep the per-job cancel checker due without changing the
                # process-wide float clock used by Qt scheduling.
                expiring._cancel_tests_regular_checker._next_check=0
                if tick:manager.REPEATINGUpdate()
                cards=[]
                for i in range(manager._message_vbox.count()):
                    card=manager._message_vbox.itemAt(i).widget()
                    cards.append(dict(text=card._text_1.text(),visible=card.isVisible(),done=card.GetJobStatus().IsDone()))
                out['states'].append(dict(name=name,now=held[0],hidden=owner.isHidden(),minimized=owner.isMinimized(),active=owner.isActiveWindow(),policy=c.new_options.GetBoolean(KEY),ok_to_alter=manager._OKToAlterUI(),cards=cards,summary=manager._summary_bar._text.text(),queue_count=queue.GetCount(),in_view_count=queue.GetInViewCount(),expiring_dismissed=expiring.IsDismissed()))
            manager.AddMessage(running);manager.AddMessage(expiring);state('shown_before_freeze')
            owner.setWindowState(C.Qt.WindowState.WindowMinimized);W.QApplication.processEvents()
            running.SetStatusText('working changed while frozen');manager.AddMessage(pending)
            state('minimized_freezes_admission_and_text')
            held[0]=102;state('exact_deadline_retains')
            held[0]=103;state('expired_job_cached_while_frozen')
            owner.setWindowState(C.Qt.WindowState.WindowNoState);owner.show();W.QApplication.processEvents()
            state('restore_reconciles_expired_and_pending')
            manager.grab().save(str(HERE/'fixtures/popup-freeze-toaster-qt.png'))
            owner.hide();running.SetStatusText('working changed while hidden')
            extra=ClientThreading.JobStatus();extra.SetStatusText('queued while hidden');extra.Finish();manager.AddMessage(extra)
            state('hidden_freezes_even_with_false_policy')
            c.new_options.SetBoolean(KEY,False);state('hidden_false_still_frozen')
            owner.show();W.QApplication.processEvents();state('show_reconciles_hidden_changes')
            owner.setWindowState(C.Qt.WindowState.WindowMinimized);W.QApplication.processEvents()
            running.SetStatusText('minimized policy false updates');state('minimized_false_updates')
            c.new_options.SetBoolean(KEY,True);running.SetStatusText('live true freezes new text');state('live_true_freezes')
            c.new_options.SetBoolean(KEY,False);state('live_false_resumes_while_minimized')
            owner.setWindowState(C.Qt.WindowState.WindowNoState);owner.show();W.QApplication.processEvents()
            other=W.QWidget();windows.append(other);other.show();other.activateWindow();W.QApplication.processEvents()
            running.SetStatusText('unfocused updates');state('unfocused_is_not_freeze_condition')
            out['constructor_timers']=timers
            return out
        finally:
            c.CallRepeatingQtSafe,c.CallLaterQtSafe=original_repeat,original_later
            if manager is not None:manager.CleanBeforeDestroy()
            HydrusTime.GetNow=original_now
            c.new_options=original_options
            for window in windows:window.hide();window.deleteLater()
            W.QApplication.processEvents()
    return session.controller.CallBlockingToQt(session.controller.gui,drive)

def main():
    import hydrus_driver
    if len(sys.argv)>1:
        import record_api
        Path(sys.argv[2]).write_text(json.dumps(hydrus_driver.run_client(record_api.unpack_fixture('basic'),record)))
        return
    with tempfile.TemporaryDirectory() as folder:
        result=Path(folder)/'result.json'
        hydrus_driver.run_in_subprocess(str(Path(__file__).resolve()),'--child',str(result))
        value=json.loads(result.read_text())
    (HERE/'fixtures/popup_freeze.json').write_text(json.dumps(value,indent=2,ensure_ascii=False)+'\n')
    print('wrote popup_freeze.json')
if __name__=='__main__':main()

#!/usr/bin/env python3
"""Drive the reference's quick text-entry dialog and its startup password gate.

1. The real DialogEdit + EditTextPanel that ClientGUIDialogsQuick.EnterText
   shows (as the tag filter's 'save favourite' asks 'Enter a name for the
   favourite.'), its 'apply' button clicked with blank text, whitespace and a
   name: what the reference says and whether the dialog stays open. The same
   for the password entry (password_entry, allow_blank False).
2. A second boot of the same database after 'set_password', with EnterText
   scripted to answer a wrong password then the right one: every question the
   boot asks, and every message shown between them.

Run: QT_QPA_PLATFORM=offscreen ~/pyenv/bin/python oracle/record_text_entry_checks.py
"""
import json
import os
import sys
import tempfile

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
OUT = os.path.join(HERE, 'fixtures', 'text_entry_checks.json')
PASSWORD = 'hunter2'


def record_dialog(session):
    from qtpy import QtWidgets as W
    from hydrus.client.gui import ClientGUIDialogsMessage as M
    from hydrus.client.gui import ClientGUITopLevelWindowsPanels as TLWP
    from hydrus.client.gui.panels import ClientGUIScrolledPanelsTextEntry as Entry
    controller = session.controller

    def qt():
        shown = []
        for kind in ('ShowCritical', 'ShowWarning', 'ShowInformation'):
            setattr(M, kind, lambda parent, *args, kind=kind: shown.append({'kind': kind, 'args': list(args)}))
        cases = []
        for (label, message, kwargs) in (
            ('favourite name', 'Enter a name for the favourite.', {}),
            ('password', 'Enter your password.', {'allow_blank': False, 'password_entry': True, 'min_char_width': 36}),
        ):
            for typed in ('', '  ', 'zebra'):
                dlg = TLWP.DialogEdit(controller.gui, 'Enter Text', frame_key='quick_entry_dialog')
                panel = Entry.EditTextPanel(dlg, message, **kwargs)
                dlg.SetPanel(panel)
                dlg.setModal(True)
                dlg.show()
                panel._text.setText(typed)
                del shown[:]
                dlg._apply.click()
                W.QApplication.processEvents()
                result = dlg.result()
                still_open = dlg.isVisible()
                value = panel.GetValue() if result == W.QDialog.DialogCode.Accepted else None
                cases.append({
                    'dialog': label,
                    'typed': typed,
                    'title': dlg.windowTitle(),
                    'echo_password': panel._text.echoMode() == W.QLineEdit.EchoMode.Password,
                    'shown': list(shown),
                    'still_open': still_open,
                    'accepted': result == W.QDialog.DialogCode.Accepted,
                    'value': value,
                })
                if still_open:
                    dlg._cancel.click()
                    W.QApplication.processEvents()
                dlg.deleteLater()
        return cases

    cases = controller.CallBlockingToQt(controller.gui, qt)
    session.write('set_password', PASSWORD)
    return cases


def child_dialog(db_dir, out):
    import hydrus_driver
    result = hydrus_driver.run_client(db_dir, record_dialog)
    with open(out, 'w') as f:
        json.dump(result, f)


def child_gate(db_dir, out):
    import hydrus_driver
    from hydrus.client.gui import ClientGUIDialogsQuick as Q
    from hydrus.client.gui import ClientGUIDialogsMessage as M
    events = []
    answers = ['hunter', PASSWORD]
    for kind in ('ShowCritical', 'ShowWarning', 'ShowInformation'):
        setattr(M, kind, lambda parent, *args, kind=kind: events.append({'shown': kind, 'args': list(args)}))

    def enter_text(win, message, **kwargs):
        answer = answers.pop(0)
        events.append({'asked': message, 'kwargs': kwargs, 'answered': answer})
        return answer

    Q.EnterText = enter_text

    def hook(session):
        return {'booted': session.controller.gui is not None}

    booted = hydrus_driver.run_client(db_dir, hook)
    with open(out, 'w') as f:
        json.dump({'events': events, 'unanswered': answers, **booted}, f)


def child_cancel(db_dir, out):
    # the same boot, the password dialog cancelled (`EnterText` raises
    # `CancelledException`, as the dialog does on cancel)
    import hydrus_driver
    from hydrus.client.gui import ClientGUIDialogsQuick as Q
    from hydrus.client.gui import ClientGUIDialogsMessage as M
    from hydrus.core import HydrusExceptions
    events = []
    for kind in ('ShowCritical', 'ShowWarning', 'ShowInformation'):
        setattr(M, kind, lambda parent, *args, kind=kind: events.append({'shown': kind, 'args': list(args)}))

    def enter_text(win, message, **kwargs):
        events.append({'asked': message, 'kwargs': kwargs, 'cancelled': True})
        raise HydrusExceptions.CancelledException('Dialog cancelled.')

    Q.EnterText = enter_text
    reached_hook = []
    try:
        hydrus_driver.run_client(db_dir, lambda session: reached_hook.append(True))
        booted = True
    except Exception:
        booted = False
    with open(out, 'w') as f:
        json.dump({'events': events, 'booted': booted and bool(reached_hook), 'gui_booted': bool(reached_hook)}, f)


def main():
    if len(sys.argv) > 1 and sys.argv[1] == '--cancel':
        child_cancel(sys.argv[2], sys.argv[3])
        return
    if len(sys.argv) > 1 and sys.argv[1] == '--dialog':
        child_dialog(sys.argv[2], sys.argv[3])
        return
    if len(sys.argv) > 1 and sys.argv[1] == '--gate':
        child_gate(sys.argv[2], sys.argv[3])
        return
    import hydrus_driver, record_api
    db_dir = record_api.unpack_fixture('basic')
    with tempfile.TemporaryDirectory() as d:
        dialog = os.path.join(d, 'dialog.json')
        gate = os.path.join(d, 'gate.json')
        hydrus_driver.run_in_subprocess(os.path.abspath(__file__), '--dialog', db_dir, dialog)
        hydrus_driver.run_in_subprocess(os.path.abspath(__file__), '--gate', db_dir, gate)
        cancel = os.path.join(d, 'cancel.json')
        hydrus_driver.run_in_subprocess(os.path.abspath(__file__), '--cancel', db_dir, cancel)
        with open(dialog) as f:
            dialogs = json.load(f)
        with open(gate) as f:
            gate = json.load(f)
        with open(cancel) as f:
            cancel = json.load(f)
    with open(OUT, 'w') as f:
        json.dump({'dialogs': dialogs, 'password_gate': gate, 'password_cancel': cancel}, f, indent=2, ensure_ascii=False)
        f.write('\n')
    print('wrote', OUT)


if __name__ == '__main__':
    main()

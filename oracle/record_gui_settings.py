#!/usr/bin/env python3
"""Record GUI name editing and real client exit confirmation decisions.

GUIPanel.UpdateOptions verifies empty-name fallback and the exit checkbox. The
live main window TryToExit captures its yes/no prompt, timeout and whether it
requests controller.Exit. Exit scheduling is intercepted so all cases use the
same client; notebook importer reasons are empty, and the force-maintenance
route skips unrelated pending-maintenance review for this focused recording.
"""
import json
import os
import sys
import tempfile

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)


def record(session):
    def qt():
        from qtpy import QtWidgets as QW
        from hydrus.core import HydrusConstants as HC, HydrusGlobals as HG
        from hydrus.client.gui import ClientGUIDialogsQuick, ClientGUIFunctions
        from hydrus.client.gui.panels.options import GUIPanel
        controller = session.controller
        gui = controller.gui
        options = controller.new_options
        panel = GUIPanel.GUIPanel(gui)
        names = []
        for name in ('night library', '', '  '):
            panel._app_display_name.setText(name)
            panel.UpdateOptions()
            ClientGUIFunctions.UpdateAppDisplayName()
            names.append({'typed': name, 'saved': options.GetString('app_display_name'), 'application_title': QW.QApplication.instance().applicationDisplayName()})
        exits = []
        original_idle_work = HG.do_idle_shutdown_work
        original_question = ClientGUIDialogsQuick.GetYesNo
        original_call = controller.CallAfterQtSafe
        original_reasons = gui._notebook.GetAbleToCloseData
        try:
            gui._notebook.GetAbleToCloseData = lambda **kwargs: []
            for confirm, yes in [(False, False), (True, False), (True, True)]:
                events = []
                panel._confirm_client_exit.setChecked(confirm)
                panel.UpdateOptions()
                def question(win, text, **kwargs):
                    events.append({'question': text, 'auto_yes_time': kwargs.get('auto_yes_time')})
                    return QW.QDialog.DialogCode.Accepted if yes else QW.QDialog.DialogCode.Rejected
                def call(win, function, *args, **kwargs):
                    if function == controller.Exit:
                        events.append({'exit_requested': True})
                    else:
                        return original_call(win, function, *args, **kwargs)
                ClientGUIDialogsQuick.GetYesNo = question
                controller.CallAfterQtSafe = call
                gui.TryToExit(force_shutdown_maintenance=True)
                exits.append({'confirm': confirm, 'yes': yes, 'saved': HC.options['confirm_client_exit'], 'events': events})
        finally:
            HG.do_idle_shutdown_work = original_idle_work
            ClientGUIDialogsQuick.GetYesNo = original_question
            controller.CallAfterQtSafe = original_call
            gui._notebook.GetAbleToCloseData = original_reasons
            panel.deleteLater()
        return {'names': names, 'exits': exits}
    return session.controller.CallBlockingToQt(session.controller.gui, qt)


def main():
    import hydrus_driver
    import record_api
    if len(sys.argv) > 1:
        output = sys.argv[2]
        result = hydrus_driver.run_client(record_api.unpack_fixture('basic'), record)
        with open(output, 'w') as stream:
            json.dump(result, stream)
        return
    with tempfile.TemporaryDirectory() as directory:
        path = os.path.join(directory, 'result.json')
        hydrus_driver.run_in_subprocess(os.path.abspath(__file__), '--child', path)
        with open(path) as stream:
            result = json.load(stream)
    output = os.path.join(HERE, 'fixtures', 'gui_settings.json')
    with open(output, 'w') as stream:
        json.dump(result, stream, indent=2)
        stream.write('\n')
    print('wrote ' + output)


if __name__ == '__main__':
    main()

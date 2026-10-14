#!/usr/bin/env python3
"""Record the reference's File menu entries that exit, restart and open folders.

Boots the `basic` fixture and triggers the real File menu actions on the Qt
thread: `exit`, `restart` and `exit/force maintenance` with "confirm client
exit" on and off (the confirmation answered by script, yes or no; the
controller's `Exit` replaced by a note that it was called), and `open >
installation directory` / `open > database directory` with
`HydrusPaths.LaunchDirectory` noted instead of run.

Writes `fixtures/file_menu.json`: each case's menu path, the options, the
questions asked (title, text, keyword arguments), and whether the client
exited, was flagged to restart and was flagged to run shutdown work; the
launched directories as what they are (the controller's database directory,
`HC.BASE_DIR`).
"""
import json, os, sys, tempfile, time
HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)


def record(session):
    from hydrus.core import HydrusConstants as HC, HydrusGlobals as HG, HydrusPaths
    from hydrus.client.gui import ClientGUIDialogsQuick
    from qtpy import QtWidgets as QW
    c = session.controller
    gui = c.gui

    def qt(f):
        return c.CallBlockingToQt(gui, f)

    def find_action(menu, labels):
        for action in menu.actions():
            if action.text().replace('&', '') == labels[0]:
                if len(labels) == 1:
                    return action
                if action.menu() is not None:
                    return find_action(action.menu(), labels[1:])
        return None

    def file_menu():
        for action in gui.menuBar().actions():
            if action.text().replace('&', '') == 'file':
                return action.menu()
        raise Exception('no file menu')

    qt(lambda: gui._menu_updater_file.update())
    time.sleep(0.5)
    out = {'exits': [], 'opens': []}

    for path in (['exit'], ['restart'], ['exit/force maintenance']):
        for confirm in (True, False):
            for answer in ('yes', 'no'):
                if not confirm and answer == 'no':
                    continue
                row = {'menu': path, 'confirm': confirm, 'answer': answer, 'questions': []}
                HC.options['confirm_client_exit'] = confirm
                # (nothing due, so only the exit question can be asked)
                HC.options['idle_shutdown'] = 0
                HG.do_idle_shutdown_work = False
                HG.restart = False
                exits = []
                real_exit = c.Exit
                c.Exit = lambda: exits.append(True)

                def scripted(win, message, title='Are you sure?', **kwargs):
                    row['questions'].append({'title': title, 'message': message, 'kwargs': dict(kwargs)})
                    return QW.QDialog.DialogCode.Accepted if answer == 'yes' else QW.QDialog.DialogCode.Rejected

                real_yes_no = ClientGUIDialogsQuick.GetYesNo
                ClientGUIDialogsQuick.GetYesNo = scripted
                try:
                    qt(lambda: find_action(file_menu(), path).trigger())
                    qt(lambda: None)
                    time.sleep(0.3)
                finally:
                    ClientGUIDialogsQuick.GetYesNo = real_yes_no
                    c.Exit = real_exit
                row['exited'] = len(exits) > 0
                row['restart'] = bool(HG.restart)
                row['do_shutdown_work'] = bool(HG.do_idle_shutdown_work)
                HG.do_idle_shutdown_work = False
                HG.restart = False
                out['exits'].append(row)

    launched = []
    real_launch = HydrusPaths.LaunchDirectory
    HydrusPaths.LaunchDirectory = lambda p: launched.append(p)
    try:
        for path in (['open', 'installation directory'], ['open', 'database directory']):
            launched.clear()
            qt(lambda: find_action(file_menu(), path).trigger())
            qt(lambda: None)
            what = []
            for p in launched:
                if p == HC.BASE_DIR:
                    what.append('install directory (HC.BASE_DIR)')
                elif p == c.GetDBDir():
                    what.append('database directory (controller.GetDBDir)')
                else:
                    what.append(p)
            out['opens'].append({'menu': path, 'launched': what})
    finally:
        HydrusPaths.LaunchDirectory = real_launch
    return out


def main():
    import hydrus_driver
    if len(sys.argv) > 1 and sys.argv[1] == '--child':
        import record_api
        output = sys.argv[2]
        result = hydrus_driver.run_client(record_api.unpack_fixture('basic'), record)
        with open(output, 'w') as f:
            json.dump(result, f)
        return
    with tempfile.TemporaryDirectory() as tmp:
        path = os.path.join(tmp, 'result.json')
        hydrus_driver.run_in_subprocess(os.path.abspath(__file__), '--child', path)
        with open(path) as f:
            result = json.load(f)
    with open(os.path.join(HERE, 'fixtures', 'file_menu.json'), 'w') as f:
        json.dump(result, f, indent=1, ensure_ascii=False)
        f.write('\n')
    print(json.dumps(result, indent=1))


if __name__ == '__main__':
    main()

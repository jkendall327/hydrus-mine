#!/usr/bin/env python3
"""Record the reference's database backup dialogs.

`ClientGUI._SetupBackupPath`, `_BackupDatabase` and `ClientController.RestoreDatabase`
are run on the real client with the dialogs replaced by scripted answers: each
scenario records every message and question asked (in order, with the directories
the scenario made written as `<name>`), what the options were afterwards, and what the
client asked the database or the shutdown to do. The backup itself is not run.
"""
import json
import os
import sys
import tempfile
HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)


def record(session):
    c = session.controller
    gui = c.gui

    def work():
        from hydrus.core import HydrusExceptions, HydrusTime, HydrusGlobals as HG
        from hydrus.client.gui import ClientGUIDialogsMessage as M, ClientGUIDialogsQuick as Q
        from qtpy import QtWidgets as QW
        options = c.new_options
        root = tempfile.mkdtemp()
        names = {}

        def make(name, files=()):
            path = os.path.join(root, name)
            if files is not None:
                os.makedirs(path)
                for f in files:
                    open(os.path.join(path, f), 'w').close()
            names[path] = name
            return path

        dirs = {
            'empty': make('empty'),
            'has_files': make('has_files', ['notes.txt']),
            'has_client_db': make('has_client_db', ['client.db']),
            'missing': make('missing', None),
        }
        names[c.GetDBDir()] = 'db_dir'

        def norm(text):
            for path, name in sorted(names.items(), key=lambda kv: -len(kv[0])):
                text = text.replace(path, '<' + name + '>')
            return text

        def run(method, existing=None, last=None, picks=(), answers=()):
            log = []
            inputs = dict(existing=existing, picks=list(picks), answers=list(answers))
            picks, answers = list(picks), list(answers)
            writes = []
            options.SetNoneableString('backup_path', None if existing is None else dirs.get(existing, existing))
            options.SetNoneableInteger('last_backup_time', last)
            gui._did_a_backup_this_session = False
            old = (M.ShowInformation, M.ShowWarning, Q.PickDirectory, Q.GetYesNo, c.Write, c.Exit)

            def info(parent, text, *a, **k): log.append(['information', norm(text)])
            def warn(parent, text, *a, **k): log.append(['warning', norm(text)])
            def pick(parent, title, *a, **k):
                log.append(['pick directory', title])
                choice = picks.pop(0)
                if choice is None:
                    raise HydrusExceptions.CancelledException()
                return dirs.get(choice, c.GetDBDir() if choice == 'db_dir' else choice)
            def yesno(parent, text, *a, **k):
                log.append(['question', norm(text), k.get('yes_label'), k.get('no_label')])
                return QW.QDialog.DialogCode.Accepted if answers.pop(0) else QW.QDialog.DialogCode.Rejected
            def write(name, *args, **k):
                if name == 'backup':
                    writes.append(['backup', norm(args[0])])
                    return None
                return old[4](name, *args, **k)
            def exit_(*a, **k): writes.append(['exit', dict(restart=HG.restart, fast_exit=c._doing_fast_exit, restore=norm(c._restore_backup_path or ''))])
            M.ShowInformation, M.ShowWarning, Q.PickDirectory, Q.GetYesNo, c.Write, c.Exit = info, warn, pick, yesno, write, exit_
            try:
                existed_before = dict((n, os.path.exists(p)) for p, n in names.items())
                method()
            finally:
                c._restore_backup_path = None
                HG.restart = False
                c._doing_fast_exit = True
                M.ShowInformation, M.ShowWarning, Q.PickDirectory, Q.GetYesNo, c.Write, c.Exit = old
            path = options.GetNoneableString('backup_path')
            return dict(inputs=inputs, dialogs=log, backup_path=None if path is None else norm(path), last_backup_set=options.GetNoneableInteger('last_backup_time') is not None,
                        last_backup_now=abs((options.GetNoneableInteger('last_backup_time') or 0) - HydrusTime.GetNow()) < 5,
                        did_a_backup=gui._did_a_backup_this_session, writes=writes, made=[n for p, n in names.items() if os.path.exists(p) and not existed_before.get(n, False)])

        out = {'setup': {}, 'update': {}, 'restore': {}}
        setup = gui._SetupBackupPath
        out['setup']['cancel'] = run(setup, picks=[None])
        out['setup']['db_dir'] = run(setup, picks=['db_dir'])
        out['setup']['empty_no'] = run(setup, picks=['empty'], answers=[False])
        out['setup']['empty_yes_later'] = run(setup, picks=['empty'], answers=[True, False])
        out['setup']['has_files_no'] = run(setup, picks=['has_files'], answers=[False])
        out['setup']['has_client_db_no'] = run(setup, picks=['has_client_db'], answers=[False])
        out['setup']['missing_no'] = run(setup, picks=['missing'], answers=[False])
        out['setup']['same_as_saved'] = run(setup, existing='empty', picks=['empty'])
        out['setup']['change_has_files_no'] = run(setup, existing='empty', picks=['has_files'], answers=[False])
        out['setup']['empty_yes_now_no'] = run(setup, picks=['empty'], answers=[True, True, False])
        out['setup']['empty_yes_now_yes'] = run(setup, picks=['empty'], answers=[True, True, True])
        update = gui._BackupDatabase
        out['update']['no_path'] = run(update)
        out['update']['missing_no'] = run(update, existing='missing', answers=[False])
        out['update']['empty_no'] = run(update, existing='empty', answers=[False])
        out['update']['has_client_db_no'] = run(update, existing='has_client_db', answers=[False])
        out['update']['has_client_db_yes'] = run(update, existing='has_client_db', answers=[True])
        restore = c.RestoreDatabase
        out['restore']['cancel'] = run(restore, picks=[None])
        out['restore']['no'] = run(restore, picks=['has_client_db'], answers=[False])
        out['restore']['yes'] = run(restore, picks=['has_client_db'], answers=[True])
        return out
    return c.CallBlockingToQt(gui, work)


def main():
    import hydrus_driver
    if len(sys.argv) > 1 and sys.argv[1] == '--child':
        import record_api
        output = sys.argv[2]
        value = hydrus_driver.run_client(record_api.unpack_fixture('basic'), record)
        with open(output, 'w') as f:
            json.dump(value, f)
        return
    with tempfile.TemporaryDirectory() as tmp:
        path = os.path.join(tmp, 'result.json')
        hydrus_driver.run_in_subprocess(os.path.abspath(__file__), '--child', path)
        with open(path) as f:
            value = json.load(f)
    with open(os.path.join(HERE, 'fixtures', 'backup_dialogs.json'), 'w') as f:
        json.dump(value, f, indent=1, ensure_ascii=False)
        f.write('\n')
    print('recorded the backup dialogs')


if __name__ == '__main__':
    main()

#!/usr/bin/env python3
"""Record the reference's Database > locations panel (`MoveMediaFilesPanel`).

The real panel is made on an empty client and driven as a user would: rows are
selected, buttons' enabled states read, and the add, weight, remove, max size,
thumbnail location and "move files now" actions run with the pickers and
dialogs replaced by scripted answers. After every step the list's rows, the
button states, the thumbnail location, the granularity and rebalance labels,
the saved locations and every message or question asked are recorded, with the
directories the run made written as `<name>`.
"""
import json
import os
import sys
import tempfile
HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)


def record(session):
    c = session.controller

    def work():
        from qtpy import QtWidgets as QW
        from hydrus.core import HydrusExceptions, HydrusTime
        from hydrus.client.gui import ClientGUIDialogsMessage as M, ClientGUIDialogsQuick as Q
        from hydrus.client.gui import ClientGUITopLevelWindowsPanels as T
        from hydrus.client.gui.panels import ClientGUIFilesPhysicalStoragePanels as P
        root = tempfile.mkdtemp()
        names = {}

        def make(name, create=True):
            path = os.path.join(root, name)
            if create:
                os.makedirs(path)
            names[path] = name
            return path

        dirs = {n: make(n) for n in ('a', 'b', 'thumbs')}
        dirs['missing'] = make('missing', create=False)
        default = next(iter(c.Read('ideal_client_files_locations')[0])).path
        names[default] = 'default'
        names[c.GetDBDir()] = 'db'
        dirs['default'] = default

        def norm(text):
            for path, name in sorted(names.items(), key=lambda kv: -len(kv[0])):
                text = text.replace(path, '<' + name + '>')
            return text

        log = []
        script = {'pick': [], 'yesno': [], 'yesyesno': [], 'edit': []}
        pubs = []
        old = (M.ShowWarning, Q.PickDirectory, Q.GetYesNo, Q.GetYesYesNo, T.DialogEdit.exec, c.pub)

        def warn(parent, text, *a, **k): log.append(['warning', norm(text)])
        def pick(parent, title, *a, **k):
            log.append(['pick directory', title])
            choice = script['pick'].pop(0)
            if choice is None:
                raise HydrusExceptions.CancelledException()
            return dirs[choice]
        def yesno(parent, text, *a, **k):
            log.append(['question', norm(text)])
            return QW.QDialog.DialogCode.Accepted if script['yesno'].pop(0) else QW.QDialog.DialogCode.Rejected
        def yesyesno(parent, text, yes_tuples=None, no_label='no', **k):
            log.append(['choice', norm(text), [t[0] for t in yes_tuples], no_label])
            choice = script['yesyesno'].pop(0)
            if choice is None:
                raise HydrusExceptions.CancelledException()
            return dict(yes_tuples)[choice]
        def execute(dialog):
            panel = dialog._panel
            action = script['edit'].pop(0)
            entry = dict(title=dialog.windowTitle())
            message = getattr(panel, '_message', None)
            for child in panel.findChildren(QW.QLabel):
                if child.text() and len(child.text()) > 40:
                    entry['message'] = child.text()
            control = panel._control if hasattr(panel, '_control') else None
            entry['initial'] = control.GetValue() if control is not None else None
            if action is not None and action != 'cancel':
                control.SetValue(action['value'])
                entry['entered'] = action['value']
            entry['accepted'] = action is not None and action != 'cancel'
            log.append(['dialog', entry])
            return QW.QDialog.DialogCode.Accepted if entry['accepted'] else QW.QDialog.DialogCode.Rejected
        def pub(topic, *args, **k):
            if topic == 'do_file_storage_rebalance':
                job = args[0]
                pubs.append(dict(topic=topic, stop_in=None if job._stop_time is None else job._stop_time - HydrusTime.GetNow(), cancellable=job.IsCancellable()))
                return None
            return old[5](topic, *args, **k)
        M.ShowWarning, Q.PickDirectory, Q.GetYesNo, Q.GetYesYesNo, T.DialogEdit.exec, c.pub = warn, pick, yesno, yesyesno, execute, pub

        panel = P.MoveMediaFilesPanel(c.gui, c)
        panel._OKParent = lambda: log.append(['closed the panel'])
        listctrl = panel._current_media_base_locations_listctrl

        def select(name):
            for location in listctrl.GetData():
                if location.path == dirs[name]:
                    listctrl.SelectDatas([location], deselect_others=True)
                    return
            raise KeyError(name)

        def snap(step):
            QW.QApplication.processEvents()
            selected = listctrl.GetData(only_selected=True)
            rows = []
            for location in listctrl.GetData():
                tup = list(panel._ConvertLocationToDisplayTuple(location))
                tup[0] = norm(tup[0])
                tup[2] = '<free space>' if tup[2] not in ('DOES NOT EXIST', 'unknown') else tup[2]
                rows.append(tup)
            out = dict(step=step, rows=rows, selected=[norm(s.path) for s in selected],
                       buttons=[panel._CanIncreaseWeight(), panel._CanDecreaseWeight(), panel._CanSetMaxNumBytes(), panel._CanRemoveLocation()],
                       thumbnails=[norm(panel._thumbnails_location.text()), panel._thumbnails_location_clear.isEnabled()],
                       granularity=panel._granularity_status_st.text(), rebalance=[panel._rebalance_status_st.text(), panel._rebalance_button.isEnabled()],
                       saved=[[norm(l.path), l.ideal_weight, l.max_num_bytes] for l in sorted(c.Read('ideal_client_files_locations')[0], key=lambda l: l.path)],
                       saved_thumbnails=None if c.Read('ideal_client_files_locations')[1] is None else norm(c.Read('ideal_client_files_locations')[1].path),
                       dialogs=list(log), pubs=list(pubs))
            log.clear()
            pubs.clear()
            return out

        steps = []
        def do(step, action, fn, pick=(), yesno=(), yesyesno=(), edit=()):
            script['pick'][:] = pick
            script['yesno'][:] = yesno
            script['yesyesno'][:] = yesyesno
            script['edit'][:] = edit
            fn()
            steps.append(snap(step))
            steps[-1]['action'] = action
            steps[-1]['inputs'] = dict(pick=list(pick), yesno=list(yesno), yesyesno=list(yesyesno), edit=list(edit))
            steps[-1]['unused_script'] = [key for key in script if script[key]]
        try:
            first = snap('opened')
            first['action'] = ['open']
            first['panel'] = dict(db=norm(panel._current_db_path_st.text()), media=panel._current_media_paths_st.text(), tooltip=panel._current_media_paths_st.toolTip(), install_starts=panel._current_install_path_st.text()[:9])
            steps.append(first)
            sel = lambda name: (lambda: select(name))
            do('select the default location', ['select', 'default'], sel('default'))
            do('add a', ['add'], panel._SelectPathToAdd, pick=['a'])
            do('add a again', ['add'], panel._SelectPathToAdd, pick=['a'])
            do('cancel the picker', ['add'], panel._SelectPathToAdd, pick=[None])
            do('select a', ['select', 'a'], sel('a'))
            do('increase a', ['increase'], panel._IncreaseWeight)
            do('decrease a', ['decrease'], panel._DecreaseWeight)
            do('decrease a to nothing, no', ['decrease'], panel._DecreaseWeight, yesno=[False])
            do('set a max size to 5 GB', ['max'], panel._SetMaxNumBytes, edit=[dict(value=5 * 1024 ** 3)])
            do('cancel the max size dialog', ['max'], panel._SetMaxNumBytes, edit=['cancel'])
            do('clear the max size of a', ['max'], panel._SetMaxNumBytes, edit=[dict(value=None)])
            do('set a max size again', ['max'], panel._SetMaxNumBytes, edit=[dict(value=5 * 1024 ** 3)])
            do('select the default', ['select', 'default'], sel('default'))
            do('set the default max size', ['max'], panel._SetMaxNumBytes, edit=[dict(value=2 * 1024 ** 3)])
            do('remove a, yes', ['select', 'a', 'remove'], lambda: (select('a'), panel._RemoveSelectedBaseLocation()), yesno=[True])
            do('remove the last location', ['select', 'default', 'remove'], lambda: (select('default'), panel._RemoveSelectedBaseLocation()), yesno=[False])
            do('thumbnail location: a regular location', ['set thumbnails'], panel._SetThumbnailLocation, pick=['default'])
            do('thumbnail location: cancel', ['set thumbnails'], panel._SetThumbnailLocation, pick=[None])
            do('thumbnail location: set', ['set thumbnails'], panel._SetThumbnailLocation, pick=['thumbs'])
            do('add the thumbnail location as a file location', ['add'], panel._SelectPathToAdd, pick=['thumbs'])
            do('clear the thumbnail location, no', ['clear thumbnails'], panel._ClearThumbnailLocation, yesno=[False])
            do('clear the thumbnail location, yes', ['clear thumbnails'], panel._ClearThumbnailLocation, yesno=[True])
            do('add b', ['add'], panel._SelectPathToAdd, pick=['b'])
            do('add a path that does not exist', ['add'], panel._SelectPathToAdd, pick=['missing'])
            do('move files now with a missing path', ['rebalance'], panel._Rebalance)
            do('select the missing path', ['select', 'missing'], sel('missing'))
            do('remove the missing path, yes', ['remove'], panel._RemoveSelectedBaseLocation, yesno=[True])
            do('move files now, forget it', ['rebalance'], panel._Rebalance, yesyesno=[None])
            do('move files now, 10 minutes', ['rebalance'], panel._Rebalance, yesyesno=['run for 10 minutes'])
            do('move files now, 30 minutes', ['rebalance'], panel._Rebalance, yesyesno=['run for 30 minutes'])
            do('move files now, 1 hour', ['rebalance'], panel._Rebalance, yesyesno=['run for 1 hour'])
            do('move files now, indefinitely', ['rebalance'], panel._Rebalance, yesyesno=['run indefinitely'])
            do('move files now, custom, accepted', ['rebalance'], panel._Rebalance, yesyesno=['run for custom time'], edit=[dict(value=5400)])
            do('move files now, custom, cancelled', ['rebalance'], panel._Rebalance, yesyesno=['run for custom time'], edit=['cancel'])
        finally:
            M.ShowWarning, Q.PickDirectory, Q.GetYesNo, Q.GetYesYesNo, T.DialogEdit.exec, c.pub = old
            panel.deleteLater()
        return dict(steps=steps, default_is_under_db=default.startswith(c.GetDBDir()))
    return c.CallBlockingToQt(c.gui, work)


def main():
    import hydrus_driver
    if len(sys.argv) > 1 and sys.argv[1] == '--child':
        output = sys.argv[2]
        value = hydrus_driver.run_client(os.path.join(tempfile.mkdtemp(), 'db'), record)
        with open(output, 'w') as f:
            json.dump(value, f)
        return
    with tempfile.TemporaryDirectory() as tmp:
        path = os.path.join(tmp, 'result.json')
        hydrus_driver.run_in_subprocess(os.path.abspath(__file__), '--child', path)
        with open(path) as f:
            value = json.load(f)
    with open(os.path.join(HERE, 'fixtures', 'locations_panel.json'), 'w') as f:
        json.dump(value, f, indent=1, ensure_ascii=False)
        f.write('\n')
    print('recorded the locations panel')


if __name__ == '__main__':
    main()

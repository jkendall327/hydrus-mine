#!/usr/bin/env python3
"""Drive the actual File > open > quick export directory QAction.

Only the OS opener, home expansion and exception reporting are captured. The real
saved legacy export_path, portable resolver, fallback directory creation and
ExportingPanel staging/Cancel/Apply remain active. All paths are local synthetic
fixtures beneath the copied reference database; no OS application is launched.
"""
import json
import os
import sys
import tempfile
from pathlib import Path

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))


def record(session):
    def work():
        from qtpy import QtWidgets as QW
        from hydrus.core import HydrusData, HydrusPaths
        from hydrus.client.exporting import ClientExportingFiles as F
        from hydrus.client.gui.panels.options import ExportingPanel as O

        c = session.controller
        db = c.GetDBDir()
        home = os.path.join(db, 'synthetic-home')
        old_path = c.options['export_path']
        old_launch, old_expand, old_hook = HydrusPaths.LaunchDirectory, F.os.path.expanduser, sys.excepthook
        old_text = HydrusData.ShowText
        launched, messages, errors, panels = [], [], [], []
        home_mode = 'resolved'
        def clean(value):
            return value.replace(db, '<DB>') if isinstance(value, str) else value
        def expand(value):
            if value == os.path.join('~', 'hydrus_export'):
                return value if home_mode == 'unresolved' else os.path.join(home, 'hydrus_export')
            return old_expand(value)
        def exception(kind, error, traceback):
            errors.append({'kind': kind.__name__, 'message': clean(str(error))})
        def find(menu, label):
            for action in menu.actions():
                if action.text().replace('&', '') == label:
                    return action
            raise AssertionError(label)
        file_menu = find(c.gui._menubar, 'file').menu()
        open_action = find(file_menu, 'open')
        open_menu = open_action.menu()
        action = find(open_menu, 'quick export directory')
        out = {'label': action.text(), 'enabled': action.isEnabled(), 'tooltip': action.statusTip(),
               'open_labels': [a.text() for a in open_menu.actions()], 'cases': []}
        def trigger(name, stored):
            c.options['export_path'] = stored
            launched.clear(); messages.clear(); errors.clear()
            action.trigger()
            QW.QApplication.processEvents()
            out['cases'].append({'case': name, 'stored': clean(stored),
                'launched': [clean(v) for v in launched], 'messages': list(messages),
                'errors': list(errors), 'saved_after': clean(c.options['export_path'])})
        try:
            HydrusPaths.LaunchDirectory = lambda path: launched.append(path)
            F.os.path.expanduser = expand
            HydrusData.ShowText = lambda text: messages.append(str(text))
            sys.excepthook = exception
            trigger('fallback', None)
            out['fallback_created'] = os.path.isdir(os.path.join(home, 'hydrus_export'))
            custom = os.path.join(db, 'synthetic exports 日本')
            os.makedirs(custom)
            trigger('configured_existing', custom)
            missing = os.path.join(db, 'not created exports')
            trigger('configured_missing', missing)
            out['configured_missing_created'] = os.path.exists(missing)
            trigger('portable', 'relative exports/../synthetic exports 日本')
            trigger('empty_raw', '')
            trigger('literal_spaces', ' \t ')
            trigger('legacy_backslash_missing', 'old\\export folder')
            literal = os.path.join(db, 'literal\\export')
            os.makedirs(literal)
            trigger('legacy_backslash_existing', literal)
            trigger('invalid_legacy_value', 7)
            home_mode = 'unresolved'
            trigger('unresolved_home', None)
            home_mode = 'resolved'
            # Actual fallback creation exception is delivered through the real
            # QAction's Qt/Python exception hook; there is no modal confirmation.
            os.rmdir(os.path.join(home, 'hydrus_export'))
            Path(home, 'hydrus_export').write_text('synthetic conflicting file')
            trigger('fallback_conflicting_file', None)
            # Staging a directory does not affect this menu until UpdateOptions.
            c.options['export_path'] = custom
            panel = O.ExportingPanel(c.gui); panels.append(panel)
            panel._export_location.SetPath(os.path.join(db, 'cancelled draft'))
            trigger('options_cancel', custom)
            panel.hide(); panel.deleteLater(); panels.remove(panel)
            panel = O.ExportingPanel(c.gui); panels.append(panel)
            panel._export_location.SetPath(os.path.join(db, 'saved exports 日本'))
            panel.UpdateOptions()
            saved = c.options['export_path']
            trigger('options_apply', saved)
            reopened = O.ExportingPanel(c.gui); panels.append(reopened)
            out['reopened_control'] = clean(reopened._export_location.GetPath())
            open_menu.popup(c.gui.mapToGlobal(c.gui.rect().topLeft()))
            QW.QApplication.processEvents()
            open_menu.grab().save(str(HERE / 'fixtures/quick_export_directory_reference.png'))
            open_menu.hide()
            return out
        finally:
            HydrusPaths.LaunchDirectory, F.os.path.expanduser, sys.excepthook = old_launch, old_expand, old_hook
            HydrusData.ShowText = old_text
            c.options['export_path'] = old_path
            for panel in panels:
                panel.hide(); panel.deleteLater()
    return session.controller.CallBlockingToQt(session.controller.gui, work)


def main():
    import hydrus_driver
    if len(sys.argv) > 1 and sys.argv[1] == '--child':
        import record_api
        output = Path(sys.argv[2])
        result = hydrus_driver.run_client(record_api.unpack_fixture('basic'), record)
        output.write_text(json.dumps(result))
        return
    with tempfile.TemporaryDirectory() as directory:
        output = Path(directory, 'result.json')
        hydrus_driver.run_in_subprocess(str(Path(__file__).resolve()), '--child', str(output))
        result = json.loads(output.read_text())
    target = HERE / 'fixtures/quick_export_directory.json'
    target.write_text(json.dumps(result, indent=2, ensure_ascii=False) + '\n')
    print('wrote', target)


if __name__ == '__main__':
    main()

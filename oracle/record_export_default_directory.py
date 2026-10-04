#!/usr/bin/env python3
"""Record the actual exporting option path, browse/cancel and manual export consumer.

Only dialog answers and the fallback home directory are controlled. ExportingPanel
UpdateOptions and ReviewExportFilesPanel resolve the configured portable path.
All paths/documents are local synthetic fixtures; no exports run.
"""
import json
import os
import sys
import tempfile
HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)


def record(session):
    def work():
        from qtpy import QtWidgets as QW
        from hydrus.core import HydrusConstants as HC, HydrusExceptions, HydrusPaths
        from hydrus.client.exporting import ClientExportingFiles as F
        from hydrus.client.gui.exporting import ClientGUIExport as E
        from hydrus.client.gui.panels.options import ExportingPanel as O
        from hydrus.client.gui import ClientGUIDialogsQuick as Q
        from hydrus.client.media import ClientMediaSingle as M
        c = session.controller
        db = c.GetDBDir()
        home = os.path.join(db, 'synthetic-home')
        custom = os.path.join(db, 'synthetic exports 日本')
        old_path = HC.options['export_path']
        old_expand, old_pick = F.os.path.expanduser, Q.PickDirectory
        F.os.path.expanduser = lambda p: os.path.join(home, 'hydrus_export') if p == os.path.join('~', 'hydrus_export') else old_expand(p)
        panels = []
        def clean(value):
            return value.replace(db, '<DB>') if isinstance(value, str) else value
        def consumer():
            media = [M.MediaSingle(m) for m in c.Read('media_results_from_ids', [1])]
            panel = E.ReviewExportFilesPanel(c.gui, media)
            panels.append(panel)
            return clean(panel._directory_picker.GetPath())
        out = {'label': 'Default export directory: ', 'box': 'export folder', 'cases': [], 'browse': []}
        try:
            for name, stored, entered in [('fallback', None, ''), ('custom', None, custom), ('spaces', custom, ' \t '), ('literal_space', None, custom + ' '), ('relative', None, 'relative exports'), ('portable', 'synthetic exports 日本', None)]:
                HC.options['export_path'] = stored
                panel = O.ExportingPanel(c.gui)
                panels.append(panel)
                item = {'case': name, 'stored_before': clean(stored), 'shown': clean(panel._export_location.GetPath())}
                if entered is not None:
                    panel._export_location.SetPath(entered)
                item['entered'] = clean(panel._export_location.GetPath())
                item['staged'] = clean(HC.options['export_path'])
                panel.UpdateOptions()
                item['saved'] = clean(HC.options['export_path'])
                item['manual'] = consumer()
                out['cases'].append(item)
                if name == 'custom':
                    panel.resize(950, 600); panel.show(); QW.QApplication.processEvents()
                    panel.grab().save(os.path.join(HERE, 'fixtures/export_default_directory_editor.png'))
                    panel.hide()
            HC.options['export_path'] = 'synthetic exports 日本'
            panel = O.ExportingPanel(c.gui); panels.append(panel)
            def pick(owner, caption, existing):
                out['browse'].append({'caption': caption, 'existing': clean(existing)})
                return custom
            Q.PickDirectory = pick
            panel._export_location._Browse()
            out['picked'] = clean(panel._export_location.GetPath())
            def cancelled(*args): raise HydrusExceptions.CancelledException()
            Q.PickDirectory = cancelled
            panel._export_location._Browse()
            out['cancelled_browse'] = clean(panel._export_location.GetPath())
            panel._export_location.SetPath(os.path.join(db, 'discarded'))
            out['cancelled_option'] = clean(HC.options['export_path'])
            out['manual_after_cancel'] = consumer()
            manual = panels[-1]
            manual._directory_picker.SetPath(os.path.join(db, 'one off destination'))
            manual._RefreshPaths()
            out['one_off_shown'] = clean(manual._directory_picker.GetPath())
            out['saved_after_one_off'] = clean(HC.options['export_path'])
            out['manual_after_one_off'] = consumer()
            out['fallback_exists'] = os.path.isdir(os.path.join(home, 'hydrus_export'))
            return out
        finally:
            F.os.path.expanduser, Q.PickDirectory = old_expand, old_pick
            HC.options['export_path'] = old_path
            for panel in panels: panel.hide(); panel.deleteLater()
    return session.controller.CallBlockingToQt(session.controller.gui, work)


def main():
    import hydrus_driver
    if len(sys.argv) > 1 and sys.argv[1] == '--child':
        import record_api
        output = sys.argv[2]
        result = hydrus_driver.run_client(record_api.unpack_fixture('basic'), record)
        with open(output, 'w') as f: json.dump(result, f)
        return
    with tempfile.TemporaryDirectory() as directory:
        output = os.path.join(directory, 'result.json')
        hydrus_driver.run_in_subprocess(os.path.abspath(__file__), '--child', output)
        with open(output) as f: result = json.load(f)
    with open(os.path.join(HERE, 'fixtures/export_default_directory.json'), 'w') as f:
        json.dump(result, f, indent=1, ensure_ascii=False); f.write('\n')
    print('wrote export_default_directory.json')
if __name__ == '__main__': main()

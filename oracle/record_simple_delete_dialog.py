#!/usr/bin/env python3
"""Record the reference's simple (not advanced) "Delete files?" dialog.

With `use_advanced_file_deletion_dialog` off, `ClientGUIDialogsQuick.GetDeleteFilesJobs`
still opens a `DialogEdit` holding `EditDeleteFilesPanel`; it hides the reason
panel, and shows either the one action's label (`_simple_description`) or the
action radio. For authored files of the `basic` fixture (in one local domain,
in two, in the trash) and each `confirm_trash` case (with a suggested domain or a remembered action), this
records what the panel shows (title, visible parts, action labels and the
selected one), whether the question is already resolved, and what `GetValue`
would do for each action: the content updates (service, action, reason, file
count), the physical deletions, and the remembered options afterwards.

Usage: QT_QPA_PLATFORM=offscreen ~/pyenv/bin/python oracle/record_simple_delete_dialog.py
       (writes oracle/fixtures/simple_delete_dialog.json)
"""
import json
import os
import sys
import tempfile

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)


def record(session):
    c = session.controller

    def drive():
        from qtpy import QtWidgets as QW
        from hydrus.core import HydrusConstants as HC
        from hydrus.client import ClientConstants as CC
        from hydrus.client.gui import ClientGUIDialogsQuick as Q
        from hydrus.client.gui.panels import ClientGUIScrolledPanelsEdit as E
        from hydrus.client.gui.widgets import ClientGUICommon  # noqa: F401
        from hydrus.client.media import ClientMediaSingle as M

        results = c.Read('media_results_from_ids', list(range(1, 41)))
        by_domains = {}
        for result in results:
            domains = [key for key in result.GetLocationsManager().GetCurrent() if c.services_manager.GetServiceType(key) == HC.LOCAL_FILE_DOMAIN]
            if domains:
                by_domains.setdefault(len(domains), result)
        single, double = by_domains[1], by_domains[2]

        keys = ('use_advanced_file_deletion_dialog', 'remember_last_advanced_file_deletion_special_action', 'remember_last_advanced_file_deletion_reason')
        old_flags = {k: c.new_options.GetBoolean(k) for k in keys}
        old_last = {k: c.new_options.GetNoneableString(k) for k in ('last_advanced_file_deletion_special_action', 'last_advanced_file_deletion_reason')}
        old_confirm = HC.options['confirm_trash']
        panels = []

        def name_of(key):
            if key == CC.COMBINED_LOCAL_FILE_DOMAINS_SERVICE_KEY:
                return 'all local file domains'
            if key == CC.HYDRUS_LOCAL_FILE_STORAGE_SERVICE_KEY:
                return 'local file storage'
            return c.services_manager.GetName(key)

        def key_named(name):
            for service in c.services_manager.GetServices((HC.LOCAL_FILE_DOMAIN,)):
                if service.GetName() == name:
                    return service.GetServiceKey()
            raise RuntimeError('no local file domain named ' + name)

        def describe_value(panel):
            physical, packages = panel.GetValue()
            updates = []
            for package in packages:
                for (service_key, content_updates) in package.IterateContentUpdates():
                    for u in content_updates:
                        updates.append({'service': name_of(service_key), 'action': u.GetAction(), 'reason': u.GetReason(), 'files': len(u.GetHashes())})
            return {'physically_deleted': len(physical), 'updates': updates,
                    'saved_action': c.new_options.GetNoneableString('last_advanced_file_deletion_special_action'),
                    'saved_reason': c.new_options.GetNoneableString('last_advanced_file_deletion_reason')}

        def shown(panel, dialog_title):
            panel.show()
            QW.QApplication.processEvents()
            radios = panel._action_radio._radio_buttons
            return {
                'title': dialog_title,
                'resolved': panel.QuestionIsAlreadyResolved(),
                'description': panel._simple_description.text() if panel._simple_description.isVisible() else None,
                'actions': [b.text() for b in radios] if panel._action_radio.isVisible() else None,
                'selected': next(i for i, b in enumerate(radios) if b.isChecked()),
                'reason_panel_visible': panel._reason_panel.isVisible(),
            }

        out = {'cases': []}
        try:
            c.new_options.SetBoolean('use_advanced_file_deletion_dialog', False)
            c.new_options.SetBoolean('remember_last_advanced_file_deletion_special_action', True)
            c.new_options.SetBoolean('remember_last_advanced_file_deletion_reason', True)
            default_reason = 'Deleted from Preview or Media Viewer.'

            def trashed(result):
                result = result.Duplicate()
                result.GetLocationsManager()._current = {CC.TRASH_SERVICE_KEY, CC.HYDRUS_LOCAL_FILE_STORAGE_SERVICE_KEY}
                return result

            cases = [
                ('one domain', [single], None, None),
                ('two domains', [double], None, None),
                ('two domains, suggested my files', [double], 'my files', None),
                ('both files', [single, double], None, None),
                ('trashed', [trashed(single)], None, None),
                ('last action', [double], None, 'my files'),
            ]
            for name, files, suggested, last in cases:
                for confirm in (True, False):
                    HC.options['confirm_trash'] = confirm
                    media = [M.MediaSingle(r) for r in files]
                    suggested_key = key_named(suggested) if suggested is not None else None
                    c.new_options.SetNoneableString('last_advanced_file_deletion_special_action', None)
                    c.new_options.SetNoneableString('last_advanced_file_deletion_reason', 'remembered reason')
                    first = E.EditDeleteFilesPanel(c.gui, media, default_reason, suggested_file_service_key=suggested_key)
                    panels.append(first)
                    item = {'case': name, 'confirm_trash': confirm, 'suggested': suggested}
                    item['panel'] = shown(first, 'Delete files?')
                    item['values'] = []
                    count = len(first._action_radio._radio_buttons)
                    for index in range(count):
                        c.new_options.SetNoneableString('last_advanced_file_deletion_special_action', None)
                        c.new_options.SetNoneableString('last_advanced_file_deletion_reason', 'remembered reason')
                        panel = E.EditDeleteFilesPanel(c.gui, media, default_reason, suggested_file_service_key=suggested_key)
                        panels.append(panel)
                        panel._action_radio.Select(index)
                        panel._UpdateControls()
                        item['values'].append(describe_value(panel))
                    if last is not None:
                        c.new_options.SetNoneableString('last_advanced_file_deletion_special_action', key_named(last).hex())
                        item['last_action'] = last
                        again = E.EditDeleteFilesPanel(c.gui, media, default_reason, suggested_file_service_key=suggested_key)
                        panels.append(again)
                        item['remembered'] = shown(again, 'Delete files?')
                    out['cases'].append(item)
            return out
        finally:
            HC.options['confirm_trash'] = old_confirm
            for key, value in old_flags.items():
                c.new_options.SetBoolean(key, value)
            for key, value in old_last.items():
                c.new_options.SetNoneableString(key, value)
            for panel in panels:
                panel.deleteLater()

    return c.CallBlockingToQt(c.gui, drive)


def main():
    import hydrus_driver
    if len(sys.argv) > 1 and sys.argv[1] == '--child':
        output = sys.argv[2]
        import record_api
        result = hydrus_driver.run_client(record_api.unpack_fixture('basic'), record)
        with open(output, 'w') as f:
            json.dump(result, f, ensure_ascii=False, indent=2)
        return
    with tempfile.TemporaryDirectory() as work:
        output = os.path.join(work, 'result.json')
        hydrus_driver.run_in_subprocess(os.path.abspath(__file__), '--child', output)
        with open(output) as f:
            result = json.load(f)
        with open(os.path.join(HERE, 'fixtures/simple_delete_dialog.json'), 'w') as f:
            json.dump(result, f, ensure_ascii=False, indent=2)


if __name__ == '__main__':
    main()

#!/usr/bin/env python3
"""Record real GUI Pages checkboxes and chooser order/context for 1, 2, 10 domains.

Synthetic real services replace the service-key lookup only; the actual Qt
Options panel, chooser button layout and page-query manager generate outputs.
All sixteen combinations are recorded, including hidden top preferences,
combined-domain gating, storage priority and reference nine-button truncation.
"""
import itertools
import json
import sys
import tempfile
from pathlib import Path
HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
KEYS = ['show_all_my_files_on_page_chooser', 'show_all_my_files_on_page_chooser_at_top',
        'show_local_files_on_page_chooser', 'show_local_files_on_page_chooser_at_top']


def record(session):
    from qtpy import QtWidgets as QW
    from hydrus.core import HydrusConstants as HC
    from hydrus.client import ClientConstants as CC, ClientServices
    from hydrus.client.gui.pages.ClientGUINewPageChooser import DialogPageChooser
    from hydrus.client.gui.panels.options.GUIPagesPanel import GUIPagesPanel
    controller, gui = session.controller, session.controller.gui
    services = controller.services_manager
    original_keys, original_service = services.GetServiceKeys, services.GetService
    previous = [controller.new_options.GetBoolean(key) for key in KEYS]

    def drive():
        options = controller.new_options.Duplicate()
        panel = GUIPagesPanel(gui, options)
        fields = [panel._show_all_my_files_on_page_chooser,
                  panel._show_all_my_files_on_page_chooser_at_top,
                  panel._show_local_files_on_page_chooser,
                  panel._show_local_files_on_page_chooser_at_top]
        controls = [{'key': key, 'default': field.isChecked(), 'enabled': field.isEnabled()}
                    for key, field in zip(KEYS, fields)]
        control_labels = [label.text() for label in panel.findChildren(QW.QLabel)
                  if 'In new page chooser' in label.text() or label.text() == '  Put it at the top:']
        assert len(control_labels) == 4, control_labels
        registry = [ClientServices.GenerateService(CC.LOCAL_FILE_SERVICE_KEY if i == 0 else bytes([0x71 + i])*32, HC.LOCAL_FILE_DOMAIN,
                                                   f'domain {i+1:02d}') for i in range(10)]
        by_key = {service.GetServiceKey(): service for service in registry}
        services.GetService = lambda key: by_key.get(key) or original_service(key)
        rows = []
        for count in [1, 2, 10]:
            services.GetServiceKeys = lambda kinds, count=count: ([service.GetServiceKey() for service in registry[:count]]
                if kinds == (HC.LOCAL_FILE_DOMAIN,) else [] if kinds == (HC.FILE_REPOSITORY,) else original_keys(kinds))
            for flags in itertools.product([False, True], repeat=4):
                for key, flag, field in zip(KEYS, flags, fields):
                    controller.new_options.SetBoolean(key, flag)
                    field.setChecked(flag)
                panel.UpdateOptions()
                chooser = DialogPageChooser(gui, controller)
                chooser._HitButton(8)
                commands = sorted(chooser._command_dict.items())
                labels = [getattr(chooser, f'_button_{i}').text() if i in chooser._command_dict else '' for i in range(1,10)]
                choices = []
                for number, (_, key) in commands:
                    selected = DialogPageChooser(gui, controller)
                    selected._HitButton(8); selected._HitButton(number)
                    kind, manager = selected.GetValue()
                    context = manager.GetVariable('file_search_context').GetLocationContext()
                    choices.append({'button': number, 'kind': kind, 'key': key.hex(),
                                    'current': sorted(k.hex() for k in context.current_service_keys),
                                    'deleted': sorted(k.hex() for k in context.deleted_service_keys)})
                    selected.deleteLater()
                chooser.done(QW.QDialog.DialogCode.Rejected)
                rows.append({'count': count, 'flags': list(flags), 'labels': labels, 'choices': choices,
                             'cancelled_result': chooser.GetValue(),
                             'applied_flags': [options.GetBoolean(key) for key in KEYS],
                             'enabled': [field.isEnabled() for field in fields]})
                chooser.deleteLater()
        panel.deleteLater()
        return {'controls': controls, 'labels': control_labels,
                'services': [{'key': service.GetServiceKey().hex(), 'name': service.GetName()} for service in registry],
                'builtin_names': {key.hex(): original_service(key).GetName() for key in [CC.COMBINED_LOCAL_FILE_DOMAINS_SERVICE_KEY,
                                                                                       CC.TRASH_SERVICE_KEY, CC.HYDRUS_LOCAL_FILE_STORAGE_SERVICE_KEY]},
                'steps': rows}
    try:
        return controller.CallBlockingToQt(gui, drive)
    finally:
        services.GetServiceKeys, services.GetService = original_keys, original_service
        for key, flag in zip(KEYS, previous): controller.new_options.SetBoolean(key, flag)


def main():
    import hydrus_driver
    import record_api
    if len(sys.argv)>1 and sys.argv[1]=='--child':
        destination=Path(sys.argv[2])
        result=hydrus_driver.run_client(record_api.unpack_fixture('basic'),record)
        destination.write_text(json.dumps(result)); return
    with tempfile.TemporaryDirectory() as work:
        destination=Path(work)/'chooser.json'
        hydrus_driver.run_in_subprocess(str(Path(__file__).resolve()),'--child',str(destination))
        result=json.loads(destination.read_text())
    (HERE/'fixtures/page_chooser_options.json').write_text(json.dumps(result,indent=2)+'\n')

if __name__=='__main__': main()

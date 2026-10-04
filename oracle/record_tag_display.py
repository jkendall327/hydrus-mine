#!/usr/bin/env python3
"""Record the real display/search and ordered relationship application panels.

Captures defaults, checkbox interlocks, preserved keys and empty/reordered queues
through the live Qt panels' own GetValue methods.
"""
import json
import os
import shutil
import sys
HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
from hydrus_driver import run_client
import record_api

def record(session):
    controller = session.controller
    def work():
        from qtpy import QtWidgets as QW
        from hydrus.client.gui.metadata import ClientGUITagDisplayOptions as O
        from hydrus.client.gui.metadata import ClientGUITagDisplayMaintenanceEdit as A
        from hydrus.client.metadata import ClientTagsHandling as T
        from hydrus.core import HydrusConstants as HC
        services = controller.services_manager.GetServices(HC.REAL_TAG_SERVICES)
        manager = O.EditTagDisplayManagerPanel(controller.gui, T.TagDisplayManager())
        out = {'labels': [x.text() for x in manager.findChildren(QW.QLabel)], 'services': [s.GetName() for s in services]}
        panel = O.EditTagAutocompleteOptionsPanel(controller.gui, T.TagAutocompleteOptions(services[0].GetServiceKey()))
        steps = []
        def state(name):
            steps.append({'step': name, 'value': panel.GetValue().GetSerialisableTuple()[2], 'namespace_enabled': [panel._namespace_bare_fetch_all_allowed.isEnabled(), panel._namespace_fetch_all_allowed.isEnabled()]})
        state('default')
        panel._search_namespaces_into_full_tags.setChecked(True)
        state('search namespaces')
        panel._unnamespaced_search_gives_any_namespace_wildcards.setChecked(True)
        state('any namespace')
        panel._fetch_results_automatically.setChecked(False)
        panel._exact_match_character_threshold.SetValue(None)
        state('manual fetch, always autocomplete')
        out['autocomplete_steps'] = steps
        keys = [s.GetServiceKey() for s in services]
        application = A.EditTagDisplayApplication(controller.gui, {keys[0]: keys}, {keys[0]: []})
        first = application._tag_services.widget(0)
        out['application_labels'] = [x.text() for x in application.findChildren(QW.QLabel)]
        def value():
            siblings, parents = application.GetValue()
            return {'siblings': {k.hex(): [x.hex() for x in v] for k,v in siblings.items()}, 'parents': {k.hex(): [x.hex() for x in v] for k,v in parents.items()}}
        out['application_initial'] = value()
        first._sibling_service_keys_listbox.SetData(list(reversed(keys)))
        out['application_reordered'] = value()
        first._sibling_service_keys_listbox.SetData([])
        out['application_empty'] = value()
        manager.deleteLater(); panel.deleteLater(); application.deleteLater()
        return out
    result = controller.CallBlockingToQt(controller.gui, work)
    with open(os.path.join(HERE, 'fixtures', 'tag_display.json'), 'w') as f:
        json.dump(result, f, indent=2, ensure_ascii=False); f.write('\n')

if __name__ == '__main__':
    db_dir = record_api.unpack_fixture('repositories')
    try: run_client(db_dir, record)
    finally: shutil.rmtree(db_dir)

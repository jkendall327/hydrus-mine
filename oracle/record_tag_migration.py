#!/usr/bin/env python3
"""Record the real Qt migration controls/questions and DB source batches.

Service-to-service only: local/repository status and action matrices, both
confirmation questions, DB mapping source batches and real repository pend
readback, plus filtered sibling/parent destinations. Sibling source pairs include
raw cycles and conflicting ideals, which migration preserves in primary storage.
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
        from hydrus.core import HydrusConstants as HC
        from hydrus.client.gui.metadata.ClientGUIMigrateTags import MigrateTagsPanel
        from hydrus.client.gui import ClientGUIDialogsQuick
        local = controller.services_manager.GetServices((HC.LOCAL_TAG,))[0].GetServiceKey()
        repo = controller.services_manager.GetServices((HC.TAG_REPOSITORY,))[0].GetServiceKey()
        panel = MigrateTagsPanel(controller.gui, local)
        matrix = []
        for content in (HC.CONTENT_TYPE_MAPPINGS, HC.CONTENT_TYPE_TAG_SIBLINGS, HC.CONTENT_TYPE_TAG_PARENTS):
            panel._migration_content_type.SetValue(content)
            panel._UpdateMigrationControlsNewType()
            for source_name, source in [('local', local), ('repository', repo)]:
                panel._migration_source.SetValue(source)
                panel._UpdateMigrationControlsNewSource()
                statuses = panel._migration_source_content_status_filter
                for i in range(statuses.count()):
                    statuses.setCurrentIndex(i)
                    for destination_name, destination in [('local', local), ('repository', repo)]:
                        panel._migration_destination.SetValue(destination)
                        panel._UpdateMigrationControlsNewDestination()
                        actions = panel._migration_action
                        matrix.append({'content': HC.content_type_string_lookup[content], 'source': source_name, 'destination': destination_name, 'status': statuses.currentText(), 'actions': [actions.itemText(j) for j in range(actions.count())]})
        panel._migration_content_type.SetValue(HC.CONTENT_TYPE_MAPPINGS)
        panel._UpdateMigrationControlsNewType()
        panel._migration_destination.SetValue(repo)
        panel._UpdateMigrationControlsNewDestination()
        asked = []
        def yes(parent, message, **kwargs):
            asked.append({'message': message, 'yes': kwargs.get('yes_label', 'yes'), 'no': kwargs.get('no_label', 'no')})
            return QW.QDialog.DialogCode.Accepted
        ClientGUIDialogsQuick.GetYesNo = yes
        old = controller.CallToThread
        controller.CallToThread = lambda *a, **kw: None
        try:
            controller.new_options.SetBoolean('advanced_mode', False)
            panel._MigrationGo()
        finally:
            controller.CallToThread = old
        panel.deleteLater()
        return matrix, asked, local, repo
    matrix, asked, local, repo = controller.CallBlockingToQt(controller.gui, work)
    from hydrus.core import HydrusConstants as HC, HydrusTags
    from hydrus.client import ClientMigration, ClientLocation, ClientConstants as CC
    from hydrus.client.metadata import ClientTags
    source = ClientMigration.MigrationSourceTagServiceMappings(controller, local, ClientLocation.LocationContext.STATICCreateSimple(CC.COMBINED_FILE_SERVICE_KEY), 'sha256', None, HydrusTags.TagFilter(), (HC.CONTENT_STATUS_CURRENT,))
    source.Prepare()
    data = []
    while source.StillWorkToDo():
        data.extend(source.GetSomeData())
    source.CleanUp()
    normalized = sorted([{'hash': h.hex(), 'tags': sorted(tags)} for h, tags in data], key=lambda r:r['hash'])
    # Exercise the actual destination and verify its persisted pending state.
    dest = ClientMigration.MigrationDestinationTagServiceMappings(controller, repo, HC.CONTENT_UPDATE_PEND)
    list_source = ClientMigration.MigrationSourceList(controller, data)
    list_source.Prepare()
    while list_source.StillWorkToDo():
        dest.DoSomeWork(list_source)
    media = controller.Read('media_results', [h for h, tags in data])
    pending = sorted([{'hash': m.GetHash().hex(), 'tags': sorted(m.GetTagsManager().GetPending(repo, ClientTags.TAG_DISPLAY_STORAGE))} for m in media], key=lambda r:r['hash'])
    from hydrus.client.metadata import ClientContentUpdates
    pair_results = []
    destination_key = controller.services_manager.GetServices((HC.LOCAL_TAG,))[1].GetServiceKey()
    for kind, read in [(HC.CONTENT_TYPE_TAG_SIBLINGS, 'tag_siblings'), (HC.CONTENT_TYPE_TAG_PARENTS, 'tag_parents')]:
        initial = [('migration:left', 'migration:right'), ('excluded:left', 'migration:right')]
        if kind == HC.CONTENT_TYPE_TAG_SIBLINGS:
            initial.extend([('migration:right', 'migration:left'), ('migration:left', 'migration:other')])
        updates = [ClientContentUpdates.ContentUpdate(kind, HC.CONTENT_UPDATE_ADD, pair) for pair in initial]
        controller.WriteSynchronous('content_updates', ClientContentUpdates.ContentUpdatePackage.STATICCreateFromContentUpdates(local, updates))
        left_filter = HydrusTags.TagFilter()
        left_filter.SetRule('excluded:', HC.FILTER_BLACKLIST)
        source = ClientMigration.MigrationSourceTagServicePairs(controller, local, kind, left_filter, HydrusTags.TagFilter(), (HC.CONTENT_STATUS_CURRENT,), False, False, False, local)
        source.Prepare()
        rows = []
        while source.StillWorkToDo():
            rows.extend(source.GetSomeData())
        source.CleanUp()
        list_source = ClientMigration.MigrationSourceList(controller, rows)
        list_source.Prepare()
        destination = ClientMigration.MigrationDestinationTagServicePairs(controller, destination_key, HC.CONTENT_UPDATE_ADD, kind)
        while list_source.StillWorkToDo():
            destination.DoSomeWork(list_source)
        pairs = controller.Read(read, destination_key)
        pair_results.append({'kind': HC.content_type_string_lookup[kind], 'initial': initial, 'source': sorted([list(p) for p in rows]), 'destination': {str(status): sorted([list(p) for p in values]) for status, values in pairs.items()}})
    with open(os.path.join(HERE, 'fixtures/tag_migration.json'), 'w') as f:
        json.dump({'source_service_key':local.hex(), 'destination_service_key':repo.hex(), 'pair_destination_service_key':destination_key.hex(), 'matrix': matrix, 'asked': asked, 'source':normalized, 'pending':pending, 'pairs':pair_results}, f, indent=2)
        f.write('\n')


if __name__ == '__main__':
    db = record_api.unpack_fixture('repositories')
    try:
        run_client(db, record)
    finally:
        shutil.rmtree(db)

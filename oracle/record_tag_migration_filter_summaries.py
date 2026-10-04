#!/usr/bin/env python3
"""Record actual Qt migration confirmation wording for equal/asymmetric filters."""
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
        from hydrus.core import HydrusConstants as HC, HydrusTags
        from hydrus.client.gui.metadata.ClientGUIMigrateTags import MigrateTagsPanel
        from hydrus.client.gui import ClientGUIDialogsQuick as Q
        key = controller.services_manager.GetServices((HC.LOCAL_TAG,))[0].GetServiceKey()
        panel = MigrateTagsPanel(controller.gui, key)
        old_question = Q.GetYesNo
        questions = []
        def question(parent, message, **kwargs):
            questions.append(message)
            return QW.QDialog.DialogCode.Rejected
        Q.GetYesNo = question
        cases = []
        filters = [
            ('all', [], []),
            ('same blacklist', [('excluded:', 0)], [('excluded:', 0)]),
            ('different blacklist', [('excluded:', 0)], [('other:', 0)]),
            ('left only', [('excluded:', 0)], []),
            ('right only', [], [('excluded:', 0)]),
            ('same text distinct rules', [], [('explicitly allowed', 1)]),
        ]
        try:
            for content, name in [(HC.CONTENT_TYPE_TAG_SIBLINGS, 'siblings'), (HC.CONTENT_TYPE_TAG_PARENTS, 'parents')]:
                panel._migration_content_type.SetValue(content)
                panel._UpdateMigrationControlsNewType()
                panel._migration_source.SetValue(key)
                panel._UpdateMigrationControlsNewSource()
                panel._migration_destination.SetValue(key)
                panel._UpdateMigrationControlsNewDestination()
                for label, left, right in filters:
                    actual = []
                    for rules in (left, right):
                        f = HydrusTags.TagFilter()
                        for tag, rule in rules:
                            f.SetRule(tag, HC.FILTER_BLACKLIST if rule == 0 else HC.FILTER_WHITELIST)
                        actual.append(f)
                    panel._migration_source_left_tag_pair_filter.SetValue(actual[0])
                    panel._migration_source_right_tag_pair_filter.SetValue(actual[1])
                    panel._MigrationGo()
                    cases.append({'content': name, 'label': label, 'left': left, 'right': right, 'left_text': actual[0].ToFilterString(), 'right_text': actual[1].ToFilterString(), 'confirmation': questions[-1]})
            return {'service_key': key.hex(), 'cases': cases}
        finally:
            Q.GetYesNo = old_question
            panel.deleteLater()
    return controller.CallBlockingToQt(controller.gui, work)


if __name__ == '__main__':
    db = record_api.unpack_fixture('basic')
    try:
        result = run_client(db, record)
        with open(os.path.join(HERE, 'fixtures/tag_migration_filter_summaries.json'), 'w') as stream:
            json.dump(result, stream, indent=2)
            stream.write('\n')
    finally:
        shutil.rmtree(db)

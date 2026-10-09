#!/usr/bin/env python3
"""Record the reference's auto-resolution rule editor and its nested editors.

On the `basic` fixture, the real panels are made and driven through their
controls:

- `identity`: `EditDuplicatesAutoResolutionRulePanel` (the rule's name, paused
  box, operation mode choice and maximum pending pairs): the operation mode
  choices and the starting state of a new rule's controls, and for each case
  of `IDENTITY` (name, paused, mode, max pending) what `GetValue` says of the
  rule and whether the max pending box is enabled;
- `names`: `EditDuplicatesAutoResolutionRulesPanel._ImportRule` against rules
  already named (case-folded unique names);
- `hardcoded`: `EditPairComparatorOneFileHardcodedPanel`: its choices, and for
  every pair of target (A, B, either) and test, the comparator's summary and
  values;
- `visual`: `EditPairComparatorRelativeVisualDuplicatesPanel`: its choices,
  and each choice's comparator summary and value;
- `comparator_list`: what `EditComparatorList._AddComparator` offers (the
  choices and their descriptions);
- `note_settings`: the metadata merge options editor's "note merge settings"
  with the real `EditNoteImportOptionsPanel` (simple mode) in the real
  `DialogEdit`, accepted and cancelled, for each starting state and each
  edit: the dialog's title, the rows it shows, the controls' starting values
  and choices, and what the merge options hold after.

Usage: QT_QPA_PLATFORM=offscreen python oracle/record_auto_resolution_editor.py
       (writes fixtures/auto_resolution_editor.json)
"""
import itertools, json, os, sys
HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)

OUT = os.path.join(HERE, 'fixtures', 'auto_resolution_editor.json')

# name, paused, operation mode (1 semi-automatic, 2 fully automatic), max pending
IDENTITY = [
    ('a rule', False, 1, 512),
    ('a rule', True, 1, 512),
    ('a rule', False, 2, 512),
    ('a rule', True, 2, None),
    ('', False, 1, 512),
    ('  spaced  ', False, 1, 1),
    ('77 pending', False, 1, 77),
    ('no limit', False, 1, None),
    ('Ünïcode ß', False, 2, 1000000),
]

# existing names, the name imported
NAMES = [
    ([], 'new rule'),
    (['new rule'], 'new rule'),
    (['New Rule'], 'new rule'),
    (['new rule', 'new rule (1)'], 'new rule'),
    (['new rule', 'NEW RULE (1)'], 'new rule'),
    (['Straße'], 'STRASSE'),
    (['a', 'A (1)', 'a (2)'], 'A'),
    (['x (1)'], 'x (1)'),
]


def record(session):
    c = session.controller
    gui = c.gui

    def f():
        from qtpy import QtWidgets as QW
        from hydrus.core import HydrusConstants as HC
        from hydrus.client import ClientConstants as CC
        from hydrus.client.duplicates import ClientDuplicatesAutoResolution as AR
        from hydrus.client.duplicates import ClientDuplicatesAutoResolutionComparators as C
        from hydrus.client.files.images import ClientVisualData
        from hydrus.client.gui.duplicates import ClientGUIDuplicatesAutoResolution as G
        from hydrus.client.gui.duplicates import ClientGUIDuplicatesContentMergeOptions as M
        from hydrus.client.gui import ClientGUIDialogsQuick
        from hydrus.client.importing.options import NoteImportOptions

        out = {}

        # --- identity
        def choices(ctrl):
            return [[ctrl.itemText(i), ctrl.itemData(i)] for i in range(ctrl.count())]

        rule = AR.DuplicatesAutoResolutionRule('new rule')
        panel = G.EditDuplicatesAutoResolutionRulePanel(gui, rule)
        out['identity_start'] = {
            'name': panel._name.text(), 'paused': panel._paused.isChecked(),
            'modes': choices(panel._operation_mode), 'mode': panel._operation_mode.GetValue(),
            'max_pending': panel._max_pending_pairs.GetValue(),
            'max_pending_enabled': panel._max_pending_pairs.isEnabled(),
            'max_pending_none_phrase': panel._max_pending_pairs._checkbox.text() if hasattr(panel._max_pending_pairs, '_checkbox') else None,
            'tabs': [panel._main_notebook.tabText(i) for i in range(panel._main_notebook.count())],
        }
        out['identity_start']['max_pending_range'] = [panel._max_pending_pairs._number_value.minimum(), panel._max_pending_pairs._number_value.maximum()]
        panel.deleteLater()
        out['identity'] = []

        def rule_with_a_comparator(name):
            # (a "better" action needs a comparator that can tell A from B)
            hard = C.PairComparatorOneFileHardcoded()
            hard.SetLookingAt(C.LOOKING_AT_A)
            hard.SetComparatorType(C.HARDCODED_COMPARATOR_TYPE_ONE_FILE_JPEG_IS_PROGRESSIVE)
            selector = C.PairSelector()
            selector.SetComparators([hard])
            r = AR.DuplicatesAutoResolutionRule(name)
            r.SetPairSelector(selector)
            return r

        for (name, paused, mode, pending) in IDENTITY + [('no comparator', False, 1, 512)]:
            start = rule_with_a_comparator('new rule') if name != 'no comparator' else AR.DuplicatesAutoResolutionRule('new rule')
            panel = G.EditDuplicatesAutoResolutionRulePanel(gui, start)
            panel._name.setText(name)
            panel._paused.setChecked(paused)
            panel._operation_mode.SetValue(mode)
            panel._max_pending_pairs.SetValue(pending)
            enabled = panel._max_pending_pairs.isEnabled()
            row = {'name': name, 'paused': paused, 'mode': mode, 'max_pending': pending, 'max_pending_enabled': enabled}
            try:
                value = panel.GetValue()
                row['rule'] = {'name': value.GetName(), 'paused': value.IsPaused(), 'mode': value.GetOperationMode(),
                               'max_pending': value.GetMaxPendingPairs(), 'action': value.GetAction()}
            except Exception as e:
                row['error'] = type(e).__name__ + ': ' + str(e)
            out['identity'].append(row)
            panel.deleteLater()

        # --- names
        out['names'] = []
        for (existing, imported) in NAMES:
            panel = G.EditDuplicatesAutoResolutionRulesPanel(gui, [AR.DuplicatesAutoResolutionRule(n) for n in existing])
            new = AR.DuplicatesAutoResolutionRule(imported)
            panel._ImportRule(new)
            names = sorted(r.GetName() for r in panel.GetValue())
            out['names'].append({'existing': existing, 'imported': imported, 'names': names, 'got': new.GetName()})
            panel.deleteLater()

        # --- hardcoded
        probe = C.PairComparatorOneFileHardcoded()
        panel = G.EditPairComparatorOneFileHardcodedPanel(gui, probe)
        hardcoded = {'target_choices': choices(panel._looking_at), 'test_choices': choices(panel._comparator_type),
                     'start': [panel._looking_at.GetValue(), panel._comparator_type.GetValue()], 'cases': []}
        panel.deleteLater()
        for (target, test) in itertools.product([v for (_, v) in hardcoded['target_choices']], [v for (_, v) in hardcoded['test_choices']]):
            start = C.PairComparatorOneFileHardcoded()
            panel = G.EditPairComparatorOneFileHardcodedPanel(gui, start)
            panel._looking_at.SetValue(target)
            panel._comparator_type.SetValue(test)
            value = panel.GetValue()
            hardcoded['cases'].append({'target': target, 'test': test, 'summary': value.GetSummary(),
                                       'looking_at': value.GetLookingAt(), 'type': value.GetComparatorType()})
            panel.deleteLater()
        out['hardcoded'] = hardcoded

        # --- visual
        probe = C.PairComparatorRelativeVisualDuplicates(acceptable_confidence=ClientVisualData.VISUAL_DUPLICATES_RESULT_ALMOST_CERTAINLY)
        panel = G.EditPairComparatorRelativeVisualDuplicatesPanel(gui, probe)
        visual = {'choices': choices(panel._acceptable_confidence), 'start': panel._acceptable_confidence.GetValue(), 'cases': []}
        panel.deleteLater()
        for (_, confidence) in visual['choices']:
            panel = G.EditPairComparatorRelativeVisualDuplicatesPanel(gui, C.PairComparatorRelativeVisualDuplicates(acceptable_confidence=confidence))
            value = panel.GetValue()
            visual['cases'].append({'confidence': confidence, 'summary': value.GetSummary(), 'value': value.GetAcceptableConfidence()})
            panel.deleteLater()
        out['visual'] = visual

        # --- the "add comparator" choices
        asked = {}
        original = ClientGUIDialogsQuick.SelectFromListButtons

        def capture(win, title, choice_tuples, **kwargs):
            asked['title'] = title
            asked['choices'] = [[label, description] for (label, comparator, description) in choice_tuples]
            raise __import__('hydrus.core.HydrusExceptions', fromlist=['x']).CancelledException()

        G.ClientGUIDialogsQuick.SelectFromListButtons = capture
        listbox = G.EditComparatorList(gui)
        try:
            listbox._AddComparator()
        except Exception:
            pass
        G.ClientGUIDialogsQuick.SelectFromListButtons = original
        out['comparator_list'] = asked
        listbox.deleteLater()

        # --- note merge settings
        from hydrus.client.gui import ClientGUITopLevelWindowsPanels as TLW
        scripts = []
        seen = []
        original_exec = TLW.DialogEdit.exec

        def exec_(dlg):
            script = scripts.pop(0)
            notes = dlg.findChildren(M.ClientGUIImportOptionsPanels.EditNoteImportOptionsPanel)
            assert len(notes) == 1
            n = notes[0]

            def visible_labels():
                return [w.text() for w in n.findChildren(QW.QLabel) if not w.isHidden() and w.text() != '']

            dlg.show()
            seen.append({
                'title': dlg.windowTitle(),
                'labels': visible_labels(),
                'extend': n._extend_existing_note_if_possible.isChecked(),
                'conflict': n._conflict_resolution.GetValue(),
                'conflict_choices': choices(n._conflict_resolution),
                'hidden': {'get_notes': n._get_notes.isHidden(), 'whitelist': n._name_whitelist.isHidden(),
                           'overrides': n._names_to_name_overrides.isHidden(), 'all_override': n._all_name_override.isHidden()},
            })
            if script is not None:
                (extend, conflict) = script
                n._extend_existing_note_if_possible.setChecked(extend)
                n._conflict_resolution.SetValue(conflict)
                dlg.accept()
                return QW.QDialog.DialogCode.Accepted
            dlg.reject()
            return QW.QDialog.DialogCode.Rejected

        TLW.DialogEdit.exec = exec_
        out['note_settings'] = []
        conflicts = [NoteImportOptions.NOTE_IMPORT_CONFLICT_REPLACE, NoteImportOptions.NOTE_IMPORT_CONFLICT_IGNORE,
                     NoteImportOptions.NOTE_IMPORT_CONFLICT_APPEND, NoteImportOptions.NOTE_IMPORT_CONFLICT_RENAME]
        try:
            for decision in (HC.DUPLICATE_SAME_QUALITY, HC.DUPLICATE_BETTER):
                options = c.new_options.GetDuplicateContentMergeOptions(decision)
                for accept in (True, False):
                    for (start_extend, start_conflict) in [(True, NoteImportOptions.NOTE_IMPORT_CONFLICT_IGNORE), (False, NoteImportOptions.NOTE_IMPORT_CONFLICT_RENAME)]:
                        for (extend, conflict) in itertools.product([False, True], conflicts):
                            note_options = NoteImportOptions.NoteImportOptions()
                            note_options.SetExtendExistingNoteIfPossible(start_extend)
                            note_options.SetConflictResolution(start_conflict)
                            options.SetSyncNoteImportOptions(note_options)
                            widget = M.EditDuplicateContentMergeOptionsWidget(gui, decision, options)
                            scripts.append((extend, conflict) if accept else None)
                            seen.clear()
                            widget._EditNoteImportOptions()
                            held = widget.GetValue().GetSyncNoteImportOptions()
                            out['note_settings'].append({
                                'decision': decision, 'accept': accept, 'start': [start_extend, start_conflict],
                                'edit': [extend, conflict], 'dialog': seen[0],
                                'after': [held.GetExtendExistingNoteIfPossible(), held.GetConflictResolution()],
                                'notes_button_enabled': widget._sync_note_import_options_button.isEnabled(),
                            })
                            widget.deleteLater()
        finally:
            TLW.DialogEdit.exec = original_exec
        return out

    return c.CallBlockingToQt(gui, f)


def main():
    import hydrus_driver, record_api, tempfile
    if len(sys.argv) > 1 and sys.argv[1] == '--child':
        output = sys.argv[2]
        result = hydrus_driver.run_client(record_api.unpack_fixture('basic'), record)
        with open(output, 'w') as fh:
            json.dump(result, fh, ensure_ascii=False)
        return
    with tempfile.TemporaryDirectory() as tmp:
        path = os.path.join(tmp, 'r.json')
        hydrus_driver.run_in_subprocess(os.path.abspath(__file__), '--child', path)
        with open(path) as fh:
            result = json.load(fh)
    with open(OUT, 'w') as fh:
        json.dump(result, fh, indent=1, ensure_ascii=False)
        fh.write('\n')
    print(json.dumps(result['identity_start']))
    print(len(result['identity']), len(result['names']), len(result['hardcoded']['cases']), len(result['visual']['cases']), len(result['note_settings']))


if __name__ == '__main__':
    main()

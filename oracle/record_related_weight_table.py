#!/usr/bin/env python3
"""Actual Qt mixed-case namespace tables, header sorting, Add and separate selection.

Only detached Options drafts in a private basic client are edited. Real table
sorting/selection and namespace/weight Add run unchanged; modal answers are scripted.
"""
import json
import sys
import tempfile
from pathlib import Path
HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))

def record(session):
    from qtpy import QtCore as QC, QtWidgets as QW
    from hydrus.client.gui import ClientGUIDialogsQuick as Q, ClientGUITopLevelWindowsPanels as W
    from hydrus.client.gui.panels.options.TagSuggestionsPanel import TagSuggestionsPanel
    c = session.controller
    def work():
        initial = [('Zulu:', 200), ('alpha:', 200), ('ß:', 400), ('ss:', 100), ('SS:', 400)]
        result = [('Zulu:', 300), ('alpha:', 100), ('ß:', 100), ('ss:', 300)]
        draft = c.new_options.Duplicate()
        draft.SetRelatedTagsTagSliceWeights(initial, result)
        panel = TagSuggestionsPanel(c.gui, draft)
        search, output = panel._search_tag_slices_weights, panel._result_tag_slices_weights
        events = []
        def snap(action):
            events.append(dict(action=action, search=search.GetData(), result=output.GetData(),
                               search_selected=search.GetData(only_selected=True), result_selected=output.GetData(only_selected=True),
                               search_display=[panel._ConvertTagSliceAndWeightToDisplayTuple(x) for x in search.GetData()],
                               result_display=[panel._ConvertTagSliceAndWeightToDisplayTuple(x) for x in output.GetData()]))
        old_enter, old_exec = Q.EnterText, W.DialogEdit.exec
        Q.EnterText = lambda *a, **kw: 'bravo'
        def modal(dialog):
            dialog._panel._control.setValue(200)
            return QW.QDialog.DialogCode.Accepted
        W.DialogEdit.exec = modal
        try:
            snap('initial')
            search.SelectDatas([('alpha:', 200)], deselect_others=True)
            output.SelectDatas([('Zulu:', 300)], deselect_others=True)
            snap('select both')
            search.sortByColumn(1, QC.Qt.SortOrder.AscendingOrder); snap('search weight ascending')
            search.sortByColumn(1, QC.Qt.SortOrder.DescendingOrder); snap('search weight descending')
            panel._AddSearchTagSliceWeight(); snap('add search replaces selection')
            output.sortByColumn(0, QC.Qt.SortOrder.DescendingOrder); snap('result slice descending')
            search.sortByColumn(0, QC.Qt.SortOrder.AscendingOrder); snap('search slice ascending')
            search.sortByColumn(0, QC.Qt.SortOrder.DescendingOrder); snap('search slice descending')
            panel.UpdateOptions()
            return dict(initial=initial, initial_result=result, events=events, saved=draft.GetRelatedTagsTagSliceWeights())
        finally:
            Q.EnterText, W.DialogEdit.exec = old_enter, old_exec
            panel.deleteLater()
    return c.CallBlockingToQt(c.gui, work)

def main():
    import hydrus_driver
    if len(sys.argv) > 1 and sys.argv[1] == '--child':
        import record_api
        Path(sys.argv[2]).write_text(json.dumps(hydrus_driver.run_client(record_api.unpack_fixture('basic'), record)))
        return
    with tempfile.TemporaryDirectory() as folder:
        path = Path(folder) / 'result.json'
        hydrus_driver.run_in_subprocess(str(Path(__file__).resolve()), '--child', str(path))
        result = json.loads(path.read_text())
    (HERE / 'fixtures/related_weight_table.json').write_text(json.dumps(result, indent=2, ensure_ascii=False) + '\n')
    print('wrote related_weight_table.json')
if __name__ == '__main__':
    main()

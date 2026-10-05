#!/usr/bin/env python3
"""Record actual Duplicates Options staging and preparation publisher policy.

The live checkbox UpdateOptions and real preparation publish callable are used;
only its maintenance-number source is held at synthetic counts. The existing
preparation recording's counts/distance boundaries are replayed for both policies.
"""
import json
import sys
import tempfile
from pathlib import Path
HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
KEY = 'hide_duplicates_needs_work_message_when_reasonably_caught_up'


def record(session):
    def work():
        from qtpy import QtWidgets as QW
        from hydrus.client import ClientOptions, ClientConstants as CC
        from hydrus.client.duplicates import ClientPotentialDuplicatesManager
        from hydrus.client.gui.pages import ClientGUISidebarDuplicates as D
        from hydrus.client.gui.panels.options.DuplicatesPanel import DuplicatesPanel
        import record_duplicates_preparation as recorded
        c = session.controller
        numbers = ClientPotentialDuplicatesManager.PotentialDuplicatesMaintenanceNumbersStore.instance()
        old_numbers = numbers.GetMaintenanceNumbers
        old_hide = c.new_options.GetBoolean(KEY)
        old_distance = c.new_options.GetInteger('similar_files_duplicate_pairs_search_distance')
        numbers.GetMaintenanceNumbers = lambda: {-1: 1, 8: 1999}
        owner = QW.QDialog(c.gui)
        layout = QW.QVBoxLayout(owner)
        panel = DuplicatesPanel(owner, c.new_options); layout.addWidget(panel)
        preparation = D.PreparationPanel(c.gui)
        names = []
        preparation.pageTabNameChanged.connect(names.append)
        def publish(counts, distance):
            c.new_options.SetInteger('similar_files_duplicate_pairs_search_distance', distance)
            preparation._maintenance_status_updater._publish_callable(counts)
            return {'counts': counts, 'distance': distance, 'page_name': names[-1],
                    'eligible': preparation._eligible_files.text(),
                    'searched': preparation._num_searched._st.text(),
                    'gauge': list(preparation._num_searched._gauge.GetValueRange()),
                    'can_start': preparation._start_search_button.isEnabled()}
        out = {'label':'Hide the "x% done" notification on preparation tab when >99% searched:',
               'box':'duplicates filter page','factory':ClientOptions.ClientOptions().GetBoolean(KEY),
               'loaded':panel._hide_duplicates_needs_work_message_when_reasonably_caught_up.isChecked(),
               'cases':[]}
        try:
            panel._hide_duplicates_needs_work_message_when_reasonably_caught_up.setChecked(not old_hide)
            out['cancel_staged'] = c.new_options.GetBoolean(KEY)
            out['cancel_publisher'] = publish({-1:1,8:1999},8)
            layout.removeWidget(panel); panel.deleteLater()
            panel = DuplicatesPanel(owner, c.new_options); layout.addWidget(panel)
            out['cancel_reopened'] = panel._hide_duplicates_needs_work_message_when_reasonably_caught_up.isChecked()
            for hidden in (False, True):
                before = c.new_options.GetBoolean(KEY)
                panel._hide_duplicates_needs_work_message_when_reasonably_caught_up.setChecked(hidden)
                staged = c.new_options.GetBoolean(KEY)
                before_publish = publish({-1:1,8:1999},8)
                panel.UpdateOptions()
                saved = c.new_options.GetBoolean(KEY)
                from hydrus.core import HydrusSerialisable
                reopened = HydrusSerialisable.CreateFromString(c.new_options.DumpToString())
                item = {'hide':hidden,'before':before,'staged':staged,'saved':saved,
                        'reopened':reopened.GetBoolean(KEY),'before_publisher':before_publish,
                        'publisher':[publish(dict(counts),distance) for counts,distance,_ in recorded.CASES]}
                out['cases'].append(item)
                if not hidden:
                    out['legacy'] = reopened.GetSerialisableTuple()
                owner.resize(850,1000); owner.show(); QW.QApplication.processEvents()
                owner.grab().save(str(HERE/'fixtures/duplicates_progress_option_reference.png'))
            return out
        finally:
            numbers.GetMaintenanceNumbers = old_numbers
            c.new_options.SetBoolean(KEY, old_hide)
            c.new_options.SetInteger('similar_files_duplicate_pairs_search_distance', old_distance)
            preparation.hide(); preparation.deleteLater(); owner.hide(); owner.deleteLater()
    return session.controller.CallBlockingToQt(session.controller.gui, work)


def main():
    import hydrus_driver
    if len(sys.argv)>1 and sys.argv[1]=='--child':
        import record_api
        output=Path(sys.argv[2])
        result=hydrus_driver.run_client(record_api.unpack_fixture('basic'),record)
        output.write_text(json.dumps(result)); return
    with tempfile.TemporaryDirectory() as directory:
        output=Path(directory,'result.json')
        hydrus_driver.run_in_subprocess(str(Path(__file__).resolve()),'--child',str(output))
        result=json.loads(output.read_text())
    target=HERE/'fixtures/duplicates_progress_option.json'
    target.write_text(json.dumps(result,indent=2,ensure_ascii=False)+'\n')
    print('wrote',target)
if __name__=='__main__': main()

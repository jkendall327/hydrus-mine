#!/usr/bin/env python3
"""Record actual populated lock badge/media and raw Undo names with hidden namespaces.

Actual sidebar lock and FrameGUI QAction consumers; only lock confirmation is
scripted. Loaded media are actual reference DB results from the basic fixture.
"""
import json
import os
import shutil
import sys
import time
HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
from hydrus_driver import run_client
import record_api
from record_main_menu import tree


def record(session):
    c = session.controller
    from hydrus.client import ClientConstants as CC, ClientLocation
    from hydrus.client.search import ClientSearchPredicate as P
    from hydrus.client.gui import ClientGUIDialogsQuick as Q
    from qtpy import QtCore as QC, QtWidgets as QW
    gui = c.gui
    qt = lambda f: c.CallBlockingToQt(gui, f)
    original_question = Q.GetYesNo
    questions = []
    def yes(parent, message, **kwargs):
        questions.append(message)
        return QW.QDialog.DialogCode.Accepted
    manifest = json.load(open(os.path.join(HERE, 'fixtures/legacy_db/basic.manifest.json')))
    results = c.Read('media_results', [bytes.fromhex(f['hash']) for f in manifest['files']])
    results = [r for r in results if CC.LOCAL_FILE_SERVICE_KEY in r.GetLocationsManager().GetCurrent()][:2]
    assert len(results) == 2
    page = qt(lambda: gui._notebook.NewPageQuery(ClientLocation.LocationContext.STATICCreateSimple(CC.LOCAL_FILE_SERVICE_KEY), page_name='Undo populated lock'))
    for _ in range(500):
        sidebar = qt(page.GetSidebar)
        if sidebar is not None and hasattr(sidebar, '_tag_autocomplete'): break
        time.sleep(.01)
    else: raise AssertionError('sidebar did not open')
    keys = ['show_namespaces', 'show_number_namespaces', 'show_subtag_number_namespaces']
    old = qt(lambda:[c.new_options.GetBoolean(key) for key in keys])
    def snapshot():
        for _ in range(3): QC.QCoreApplication.processEvents()
        gui._menu_updater_undo._publish_callable(1)
        return {'predicates':[p.GetSerialisableTuple() for p in sidebar.GetPredicates()],
                'visible_predicate_labels':[p.ToString(with_count=False,render_for_user=True) for p in sidebar.GetPredicates()],
                'media_hashes':sorted(m.GetHash().hex() for m in page.GetMedia()),
                'badge':sidebar._system_hash_lock_panel._unlock_button.text(),
                'locked':sidebar._page_manager.GetVariable('system_hash_locked'),
                'synchronised':sidebar._tag_autocomplete.IsSynchronised(),
                'menu':tree(gui._menubar_undo_searching_submenu)}
    try:
        Q.GetYesNo = yes
        alpha = P.Predicate(P.PREDICATE_TYPE_TAG, 'undo:alpha')
        def setup():
            for key in keys: c.new_options.SetBoolean(key, False)
            gui._predicate_history_added_to_search.clear(); gui._predicate_history_removed_from_search.clear()
            sidebar._tag_autocomplete.SetSynchronised(False)
            page.AddMediaResults(results)
            page.EnterPredicates([alpha])
        qt(setup)
        plain = qt(snapshot)
        qt(sidebar.LockSearch)
        qt(lambda:sidebar._tag_autocomplete.SetSynchronised(True))
        before = qt(snapshot)
        def undo():
            gui._menu_updater_undo._publish_callable(1)
            additions = next(a.menu() for a in gui._menubar_undo_searching_submenu.actions() if a.text() == 'addition')
            next(a for a in additions.actions() if a.text() == 'undo:alpha').trigger()
        qt(undo)
        time.sleep(.1)
        after = qt(snapshot)
        assert before['media_hashes'] == after['media_hashes']
        assert before['badge'] == after['badge'] == 'Locked at 2 files.'
        return {'input_hashes':before['media_hashes'],'plain_menu':plain,'before_undo':before,'after_undo':after,'questions':questions}
    finally:
        Q.GetYesNo = original_question
        qt(lambda:[c.new_options.SetBoolean(key, value) for key, value in zip(keys, old)])


if __name__ == '__main__':
    db = record_api.unpack_fixture('basic')
    try:
        result = run_client(db, record)
        with open(os.path.join(HERE,'fixtures/search_undo_locked.json'),'w') as stream:
            json.dump(result,stream,indent=2,ensure_ascii=False); stream.write('\n')
    finally:
        shutil.rmtree(db)

#!/usr/bin/env python3
"""Record actual system-result activation retaining Shift and read OR drafts.

Real frame query sidebar, real result-list _Activate/FleshOut, real system OK,
real basic OR dialog GetValue, actual queued history and ClientOptions recents.
Only modal exec answers and foreground field values are scripted; only
background autocomplete fetch is suppressed. Query counts use the actual DB.
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


def record(session):
    c = session.controller
    from hydrus.client import ClientConstants as CC, ClientLocation
    from hydrus.client.gui import ClientGUITopLevelWindowsPanels as W
    from hydrus.client.gui.search import ClientGUIACDropdown as A, ClientGUIPredicatesOR as O, ClientGUIPredicatesSingle as S, ClientGUISearch as G
    from hydrus.client.search import ClientSearchFileSearchContext as F, ClientSearchTagContext as T, ClientSearchPredicate as P
    from hydrus.client.metadata import ClientContentUpdates as U
    from hydrus.core import HydrusConstants as HC
    from qtpy import QtCore as QC, QtWidgets as QW
    gui = c.gui
    qt = lambda f: c.CallBlockingToQt(gui, f)
    service = next(s.GetServiceKey() for s in c.services_manager.GetServices((HC.LOCAL_TAG,)) if s.GetName() == 'my tags')
    manifest = json.load(open(os.path.join(HERE,'fixtures/legacy_db/basic.manifest.json')))
    hashes = [bytes.fromhex(row['hash']) for row in manifest['files'][:4]]
    tag = 'parity:system or alpha'
    c.WriteSynchronous('content_updates', U.ContentUpdatePackage.STATICCreateFromContentUpdates(service, [U.ContentUpdate(HC.CONTENT_TYPE_MAPPINGS, HC.CONTENT_UPDATE_ADD, (tag, set(hashes[:2])))]))
    context = F.FileSearchContext(location_context=ClientLocation.LocationContext.STATICCreateSimple(CC.LOCAL_FILE_SERVICE_KEY),tag_context=T.TagContext(service_key=service))
    page = qt(lambda:gui._notebook.NewPageQueryFileSearchContext(context, page_name='System OR activation'))
    for _ in range(500):
        sidebar = qt(page.GetSidebar)
        if sidebar is not None and hasattr(sidebar, '_tag_autocomplete'): break
        time.sleep(.01)
    else: raise AssertionError('sidebar did not open')
    ac = sidebar._tag_autocomplete
    old_thread = c.CallToThread
    old_exec = W.DialogEdit.exec
    c.CallToThread = lambda fn,*args,**kw: None if fn == A.ReadFetch else old_thread(fn,*args,**kw)
    cases = []
    def drain():
        for _ in range(3): QC.QCoreApplication.processEvents()
    def snapshot(control):
        drain()
        predicates = control.GetFileSearchContext().GetPredicates()
        draft = control._under_construction_or_predicate
        recent = c.new_options.GetRecentPredicates([P.PREDICATE_TYPE_SYSTEM_SIZE])
        return {'predicates':[p.GetSerialisableTuple() for p in predicates],
                'labels':sorted(p.ToString(with_count=False) for p in predicates),
                'draft':None if draft is None else [p.GetSerialisableTuple() for p in draft.GetValue()],
                'text':control._text_ctrl.text(),
                'recent':[p.GetSerialisableTuple() for p in recent],
                'added':[p.ToString(with_count=False) for p in gui._predicate_history_added_to_search],
                'removed':[p.ToString(with_count=False) for p in gui._predicate_history_removed_from_search],
                'query_count':None if not predicates else len(c.Read('file_query_ids',control.GetFileSearchContext()))}
    plan = None
    def activate(control):
        if plan['seed_draft']:
            control.BroadcastChoices([P.Predicate(P.PREDICATE_TYPE_TAG,tag)],True)
        control._text_ctrl.setText('')
        plan['before'] = snapshot(control)
        control._search_results_list.SetPredicates([P.Predicate(P.PREDICATE_TYPE_SYSTEM_SIZE,None)])
        assert control._search_results_list._Activate(False,plan['shift']) == plan['accepted']
        plan['after_system'] = snapshot(control)
    def dialog_exec(dialog):
        if dialog.windowTitle() == 'input predicate':
            panel = dialog._panel.findChild(S.PanelPredicateSystemSize)
            assert panel is not None
            panel._sign.SetValue('<'); panel._bytes.SetSeparatedValue(7,1024)
            plan['system_predicates'] = [p.GetSerialisableTuple() for p in panel.GetPredicates()]
            if plan['accepted']:
                panel.parentWidget()._DoOK()
            return QW.QDialog.DialogCode.Accepted if plan['accepted'] else QW.QDialog.DialogCode.Rejected
        control = dialog._panel.findChild(O.ORPredicateControl)
        assert control is not None
        control._search_control.SetSynchronised(False)
        activate(control._search_control)
        plan['outer_value'] = [p.GetSerialisableTuple() for p in control.GetPredicates()]
        return QW.QDialog.DialogCode.Accepted if plan['outer_accepted'] else QW.QDialog.DialogCode.Rejected
    def replay():
        nonlocal plan
        W.DialogEdit.exec = dialog_exec
        for owner in ['main','basic']:
            for seed in [False,True]:
                for shift in [False,True]:
                    for accepted in [False,True]:
                        plan = {'owner':owner,'seed_draft':seed,'shift':shift,'accepted':accepted,'outer_accepted':True}
                        run_case()
        for shift in [False,True]:
            plan = {'owner':'basic','seed_draft':True,'shift':shift,'accepted':True,'outer_accepted':False}
            run_case()
        return {'tag':tag,'corpus':[{'tag':tag,'hashes':[h.hex() for h in hashes[:2]]}],'cases':cases}
    def run_case():
        ac.SetFileSearchContext(context.Duplicate()); ac.SetSynchronised(False); ac._CancelORConstruction()
        for predicate in c.new_options.GetRecentPredicates([P.PREDICATE_TYPE_SYSTEM_SIZE]): c.new_options.RemoveRecentPredicate(predicate)
        drain(); gui._predicate_history_added_to_search.clear(); gui._predicate_history_removed_from_search.clear()
        if plan['owner'] == 'main': activate(ac)
        else: ac._CreateNewOR()
        plan['after_parent'] = snapshot(ac)
        cases.append(plan)
    try:
        return qt(replay)
    finally:
        c.CallToThread = old_thread
        W.DialogEdit.exec = old_exec


if __name__ == '__main__':
    db = record_api.unpack_fixture('basic')
    try:
        result = run_client(db, record)
        with open(os.path.join(HERE,'fixtures/system_or_activation.json'),'w') as stream:
            json.dump(result,stream,indent=2,ensure_ascii=False); stream.write('\n')
    finally:
        shutil.rmtree(db)

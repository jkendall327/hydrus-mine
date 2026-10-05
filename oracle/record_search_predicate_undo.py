#!/usr/bin/env python3
"""Record actual Qt global search-history menus and visible-page toggle consumers.

Uses real query sidebars, their queued delta signals, FrameGUI's actual menu
publisher/QActions, close/restore and FleshOut cancellation. No search-history
implementation or predicates are mocked. Only confirmation answers are scripted.
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
    from hydrus.client.gui.search import ClientGUISearch as S
    from qtpy import QtCore as QC, QtWidgets as QW
    gui = c.gui
    qt = lambda f: c.CallBlockingToQt(gui, f)
    pages = {}
    events = []
    questions = []
    answers = []
    original = qt(lambda: (gui._predicate_history_added_to_search[:], gui._predicate_history_removed_from_search[:]))
    old_question = Q.GetYesNo
    def question(parent, message, **kwargs):
        questions.append(message)
        return QW.QDialog.DialogCode.Accepted if answers.pop(0) else QW.QDialog.DialogCode.Rejected
    Q.GetYesNo = question
    def drain():
        for _ in range(3): QC.QCoreApplication.processEvents()
    def new(name, initial=None):
        page = qt(lambda: gui._notebook.NewPageQuery(ClientLocation.LocationContext.STATICCreateSimple(CC.LOCAL_FILE_SERVICE_KEY), page_name=name, initial_predicates=initial))
        for _ in range(500):
            sidebar = qt(page.GetSidebar)
            if sidebar is not None and hasattr(sidebar, '_tag_autocomplete'): break
            time.sleep(.01)
        else: raise AssertionError('query sidebar did not open')
        qt(lambda: sidebar._tag_autocomplete.SetSynchronised(False))
        pages[name] = page
        return page
    def snapshot(label, operation):
        drain()
        gui._menu_updater_undo._publish_callable(1)
        histories = lambda values: [p.ToString(with_count=False) for p in values]
        searches = {name: sorted(histories(page.GetSidebar()._tag_autocomplete.GetFileSearchContext().GetPredicates())) for name, page in pages.items()}
        events.append({'label':label, 'operation':operation, 'current':gui._notebook.GetCurrentMediaPage().GetName() if gui._notebook.GetCurrentMediaPage() is not None else None, 'searches':searches, 'added':histories(gui._predicate_history_added_to_search), 'removed':histories(gui._predicate_history_removed_from_search), 'menu':tree(gui._menubar_undo_searching_submenu), 'undo_enabled':gui._menubar_undo_submenu.menuAction().isEnabled()})
    def step(label, operation, action):
        qt(lambda: (action(), snapshot(label, operation)))
    def enter(page, label, predicate):
        step(label, {'enter':predicate.GetSerialisableTuple()}, lambda:page.EnterPredicates([predicate]))
    def undo(bucket, label):
        def action():
            gui._menu_updater_undo._publish_callable(1)
            submenu = next(a.menu() for a in gui._menubar_undo_searching_submenu.actions() if a.menu() is not None and a.text()==bucket)
            next(a for a in submenu.actions() if a.text()=='undo:alpha').trigger()
        step(label, {'undo':bucket, 'predicate':'undo:alpha'}, action)
    alpha = P.Predicate(P.PREDICATE_TYPE_TAG, 'undo:alpha')
    beta = P.Predicate(P.PREDICATE_TYPE_TAG, 'undo:beta')
    try:
        a = new('Undo A')
        qt(lambda: (gui._predicate_history_added_to_search.clear(), gui._predicate_history_removed_from_search.clear()))
        step('empty', {}, lambda:None)
        enter(a,'add alpha',alpha)
        enter(a,'add beta',beta)
        enter(a,'remove alpha',alpha)
        undo('addition','addition toggles absent alpha in')
        undo('removal','removal toggles present alpha out')
        b = new('Undo B', [P.Predicate(P.PREDICATE_TYPE_TAG, 'undo:restored')])
        step('new restored predicates do not enter history', {'new':'Undo B'}, lambda:None)
        undo('addition','global addition applies to visible B')
        enter(b,'replace alpha with excluded alpha',P.Predicate(P.PREDICATE_TYPE_TAG,'undo:alpha',False))
        or_pred = P.Predicate(P.PREDICATE_TYPE_OR_CONTAINER, [P.Predicate(P.PREDICATE_TYPE_TAG,'undo:or one'),P.Predicate(P.PREDICATE_TYPE_TAG,'undo:or two')])
        enter(b,'one typed OR history entry',or_pred)
        def cancel_child():
            child = S.FleshOutPredicatePanel(gui,P.Predicate(P.PREDICATE_TYPE_SYSTEM_LIMIT,None))
            child.show(); child.close(); child.deleteLater()
        step('cancel system child adds no history', {'cancel_child':True}, cancel_child)
        step('close B preserves history', {'close':'Undo B'}, lambda:gui._notebook._ClosePage(gui._notebook.indexOf(b),polite=False))
        index = qt(lambda:next(i for i,(_,page) in enumerate(gui._closed_pages) if page is b))
        step('restore B preserves history', {'restore':'Undo B'}, lambda:gui._UnclosePage(index))
        answers.append(False)
        step('cancel clear retains both histories', {'clear':False}, gui.AskToDeleteAllSearchPredHistory)
        answers.append(True)
        step('accept clear keeps search predicates', {'clear':True}, gui.AskToDeleteAllSearchPredHistory)
        enter(b,'seed before locking',alpha)
        answers.append(True)
        locked_op = {'lock': True}
        def lock():
            b.GetSidebar().LockSearch()
            locked_op['replace'] = [p.GetSerialisableTuple() for p in b.GetSidebar().GetPredicates()]
        step('lock search records predicate replacement', locked_op, lock)
        undo('addition','undo enters hidden locked search')
        empty = qt(lambda:gui._notebook.NewPagesNotebook(name='Empty Undo',give_it_a_blank_page=False))
        step('empty notebook has no current media page', {'empty_notebook':True}, lambda:None)
        undo('addition','no current media page retains history')
        return {'events':events,'questions':questions}
    finally:
        Q.GetYesNo = old_question
        qt(lambda: (setattr(gui,'_predicate_history_added_to_search',original[0]),setattr(gui,'_predicate_history_removed_from_search',original[1])))


if __name__ == '__main__':
    db = record_api.unpack_fixture('basic')
    try:
        result = run_client(db, record)
        with open(os.path.join(HERE,'fixtures/search_predicate_undo.json'),'w') as stream:
            json.dump(result,stream,indent=2,ensure_ascii=False)
            stream.write('\n')
    finally:
        shutil.rmtree(db)

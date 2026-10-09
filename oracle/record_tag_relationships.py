#!/usr/bin/env python3
"""Record real siblings/parents panels and TagPairActionContext editing.

Loads the running repositories client's services, constructs both real Qt panels,
records labels/buttons, then primes each panel's own action context with a
small deterministic relationship graph. Executes conflict replacements,
cycle reversals, deletion/rescind questions, and workspace
filters, pending/petitioned prefixes, and final content updates through the
reference routines. No reference code is reimplemented in this recorder.
"""
import json
import os
import sys
HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
from hydrus_driver import run_client
import record_api
import shutil
OUT = os.path.join(HERE, 'fixtures', 'tag_relationships.json')

def record(session):
    controller = session.controller
    def work():
        from qtpy import QtWidgets as QW, QtCore as QC
        from hydrus.client.gui import ClientGUIAsync, ClientGUIDialogsQuick
        from hydrus.client.gui.metadata import ClientGUIManageTagSiblings as S, ClientGUIManageTagParents as P
        from hydrus.core import HydrusConstants as HC
        asked = []
        def yes(win, message, **kwargs):
            asked.append({'message': message, 'yes': kwargs.get('yes_label', 'yes'), 'no': kwargs.get('no_label', 'no')})
            return QW.QDialog.DialogCode.Accepted
        ClientGUIDialogsQuick.GetYesNo = yes
        def reason(win, message, **kwargs):
            asked.append({'message': message, 'yes': 'OK', 'no': 'cancel'})
            return 'oracle reason'
        ClientGUIDialogsQuick.EnterText = reason
        # Panel construction needs the initial status worker. Run it on this
        # same Qt call; list refresh is suppressed and requested explicitly.
        class Job:
            def __init__(self, win, w, p, **kw): self.w, self.p = w, p
            def start(self): self.p(self.w())
        class Updater:
            def __init__(self, *args, **kwargs): pass
            def update(self): pass
        ClientGUIAsync.AsyncQtJob = Job
        ClientGUIAsync.AsyncQtUpdater = Updater
        out = []
        initial = [('a', 'b'), ('b', 'c'), ('cousin', 'b')]
        for name, cls, attr, local in [('siblings', S.ManageTagSiblings, '_sibling_action_context', True), ('parents', P.ManageTagParents, '_parent_action_context', True), ('siblings', S.ManageTagSiblings, '_sibling_action_context', False), ('parents', P.ManageTagParents, '_parent_action_context', False)]:
            panel = cls._Panel(controller.gui, controller.services_manager.GetServices((HC.LOCAL_TAG if local else HC.TAG_REPOSITORY,))[0].GetServiceKey())
            context = getattr(panel, attr)
            context._AddStatusesToPairsToCache({HC.CONTENT_STATUS_CURRENT: set(initial)})
            context._have_fetched_all = True
            context._have_fetched_pending_and_petitioned = True
            buttons = [b.text() for b in panel.findChildren(QW.QPushButton)]
            labels = [l.text() for l in panel.findChildren(QW.QLabel)]
            table = panel._tag_siblings if name == "siblings" else panel._tag_parents
            columns = [table.model().headerData(i, QC.Qt.Orientation.Horizontal) for i in range(table.model().columnCount())]
            steps = []
            def state(step):
                pairs = context.GetPertinentPairsForTags(set(), True, False, True)
                rows = [list(panel._ConvertPairToDisplayTuple(p)) for p in sorted(pairs)]
                updates = sorted([[u.GetAction(), list(u.GetRow()), u.GetReason()] for u in context.GetContentUpdates()], key=str)
                steps.append({'step': step, 'rows': rows, 'updates': updates, 'asked': list(asked)})
                asked.clear()
            state(None)
            actions = [('add', ('c','a')), ('add', ('a','d')), ('delete', ('a','d')), ('delete', ('a','d')), ('delete', ('c','a'))]
            for op, pair in actions:
                context.AutoPetitionConflicts(panel, [pair])
                context.AutoPetitionLoops(panel, [pair])
                context.EnterCleanedPairs(panel, [pair], only_add=op == 'add')
                state([op, list(pair)])
            filtered = {}
            for whole in [False, True]:
                filtered[str(whole).lower()] = sorted([list(p) for p in context.GetPertinentPairsForTags({'cousin'}, False, False, whole)])
            # (and with pending and petitioned groups shown)
            filtered_pending = {}
            for whole in [False, True]:
                filtered_pending[str(whole).lower()] = sorted([list(p) for p in context.GetPertinentPairsForTags({'cousin'}, False, True, whole)])
            out.append({'kind': name, 'local': local, 'initial': [list(p) for p in initial], 'columns': columns, 'default_sort': list(table.model()._column_list_status.GetSort()), 'buttons': buttons, 'labels': labels, 'steps': steps, 'filtered': filtered, 'filtered_pending': filtered_pending})
            panel.deleteLater()
        return out
    result = controller.CallBlockingToQt(controller.gui, work)
    with open(OUT, 'w') as f: json.dump(result, f, indent=2, ensure_ascii=False); f.write('\n')

if __name__ == '__main__':
    db_dir = record_api.unpack_fixture('repositories')
    try:
        run_client(db_dir, record)
    finally:
        shutil.rmtree(db_dir)

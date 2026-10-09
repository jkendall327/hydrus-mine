#!/usr/bin/env python3
"""Record the real siblings/parents panels' clipboard and .txt pair exchange.

For both kinds, on the local tag service and a tag repository, primes each real
panel's own action context with a small current graph, then imports a sequence
of texts through the panel's own `_ImportFromClipboard` and `_ImportFromTXT`
(uneven lines, cleaning, blank lines, pre-existing pairs, a sibling conflict, a
loop, a self-pair, a conflict inside one batch, re-importing a petitioned pair,
more than ten pairs), answering every question as the recording says. After
each import it records the questions and messages shown, the listed rows and
the content updates the panel would commit. Finally it selects rows and records
the export text of `_GetExportString`, with and without a selection.

EnterPairs normally waits for a worker thread before running on the Qt thread;
here the controller's CallToThread/CallAfterQtSafe run their callable at once,
so the reference's own EnterPairs runs unchanged.
"""
import json
import os
import shutil
import sys
import tempfile
HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
from hydrus_driver import run_client
import record_api
OUT = os.path.join(HERE, 'fixtures', 'relationship_exchange.json')

INITIAL = [('a', 'b'), ('b', 'c'), ('cousin', 'b')]

# (how, text, answers to yes/no questions in order)
IMPORTS = [
    ('clipboard', 'x\ny\nodd', []),
    ('txt', '  Upper Case  \nlower\n\n\nx\ny\nSeries:Foo Bar\nseries:foo\n', []),
    ('clipboard', 'a\nb', []),
    ('clipboard', 'a\nnew ideal', []),
    ('txt', 'c\ncousin', []),
    ('clipboard', 'self\nself', []),
    ('clipboard', 'p\nq\np\nr', []),
    ('clipboard', 'a\nb', [True, True]),
    ('clipboard', '\n'.join(f'many {i:02}\nmany ideal' for i in range(12)), []),
    ('clipboard', '', []),
]

EXPORTS = [[], [('a', 'b')], [('x', 'y'), ('a', 'b'), ('upper case', 'lower')]]


def record(session):
    controller = session.controller
    def work():
        from qtpy import QtWidgets as QW
        from hydrus.client import ClientGlobals as CG
        from hydrus.client.gui import ClientGUIAsync, ClientGUIDialogsQuick, ClientGUIDialogsMessage
        from hydrus.client.gui import ClientGUIDialogsFiles
        from hydrus.client.gui.metadata import ClientGUIManageTagSiblings as S, ClientGUIManageTagParents as P
        from hydrus.core import HydrusConstants as HC
        asked = []
        answers = []
        def yes(win, message, **kwargs):
            answer = answers.pop(0) if answers else True
            asked.append({'kind': 'yes_no', 'message': message, 'yes': kwargs.get('yes_label', 'yes'), 'no': kwargs.get('no_label', 'no'), 'answer': answer})
            return QW.QDialog.DialogCode.Accepted if answer else QW.QDialog.DialogCode.Rejected
        def reason(win, message, **kwargs):
            asked.append({'kind': 'reason', 'message': message, 'suggestions': list(kwargs.get('suggestions', [])), 'answer': 'oracle reason'})
            return 'oracle reason'
        def info(win, message):
            asked.append({'kind': 'information', 'message': message})
        def critical(win, title, message):
            asked.append({'kind': 'critical', 'title': title, 'message': message})
        def warning(win, message):
            asked.append({'kind': 'warning', 'message': message})
        ClientGUIDialogsQuick.GetYesNo = yes
        ClientGUIDialogsQuick.EnterText = reason
        ClientGUIDialogsMessage.ShowInformation = info
        ClientGUIDialogsMessage.ShowCritical = critical
        ClientGUIDialogsMessage.ShowWarning = warning
        class Job:
            def __init__(self, win, w, p, **kw): self.w, self.p = w, p
            def start(self): self.p(self.w())
        class Updater:
            def __init__(self, *args, **kwargs): pass
            def update(self): pass
        ClientGUIAsync.AsyncQtJob = Job
        ClientGUIAsync.AsyncQtUpdater = Updater
        clipboard = {'text': ''}
        controller.GetClipboardText = lambda: clipboard['text']
        # only calls made from this (the Qt) thread run at once; the client's
        # own threads keep the real scheduling
        import threading
        qt_thread = threading.current_thread()
        real_to_thread, real_after_qt = controller.CallToThread, controller.CallAfterQtSafe
        def to_thread(c, *a, **k):
            if threading.current_thread() is qt_thread: return c(*a, **k)
            return real_to_thread(c, *a, **k)
        def after_qt(win, c, *a, **k):
            if threading.current_thread() is qt_thread: return c(*a, **k)
            return real_after_qt(win, c, *a, **k)
        def synchronously(call):
            controller.CallToThread, controller.CallAfterQtSafe = to_thread, after_qt
            try: call()
            finally: controller.CallToThread, controller.CallAfterQtSafe = real_to_thread, real_after_qt
        picked = {'path': None}
        class Picker:
            def __init__(self, parent, message, **kwargs):
                asked.append({'kind': 'file_dialog', 'message': message})
            def __enter__(self): return self
            def __exit__(self, *a): return False
            def exec(self): return QW.QDialog.DialogCode.Accepted
            def GetPath(self): return picked['path']
        ClientGUIDialogsFiles.FileDialog = Picker
        # the recent reasons start empty, as in a fresh client
        controller.new_options._dictionary['recent_petition_reasons'] = {}
        tmp = tempfile.mkdtemp()
        out = []
        for name, cls, attr, local in [('siblings', S.ManageTagSiblings, '_sibling_action_context', True), ('parents', P.ManageTagParents, '_parent_action_context', True), ('siblings', S.ManageTagSiblings, '_sibling_action_context', False), ('parents', P.ManageTagParents, '_parent_action_context', False)]:
            service = controller.services_manager.GetServices((HC.LOCAL_TAG if local else HC.TAG_REPOSITORY,))[0]
            panel = cls._Panel(controller.gui, service.GetServiceKey())
            context = getattr(panel, attr)
            context._AddStatusesToPairsToCache({HC.CONTENT_STATUS_CURRENT: set(INITIAL)})
            context._have_fetched_all = True
            context._have_fetched_pending_and_petitioned = True
            table = panel._tag_siblings if name == 'siblings' else panel._tag_parents
            def rows():
                pairs = context.GetPertinentPairsForTags(set(), True, False, True)
                return [list(panel._ConvertPairToDisplayTuple(p)) for p in sorted(pairs)]
            def updates():
                return sorted([[u.GetAction(), list(u.GetRow()), u.GetReason()] for u in context.GetContentUpdates()], key=str)
            steps = []
            for how, text, scripted in IMPORTS:
                answers[:] = scripted
                if how == 'clipboard':
                    clipboard['text'] = text
                    synchronously(lambda: panel._ImportFromClipboard(True))
                else:
                    path = os.path.join(tmp, f'{name}.txt')
                    with open(path, 'w', encoding='utf-8') as f: f.write(text)
                    picked['path'] = path
                    synchronously(lambda: panel._ImportFromTXT(True))
                steps.append({'how': how, 'text': text, 'asked': list(asked), 'rows': rows(), 'updates': updates(), 'workspace': sorted(panel._current_pertinent_tags)})
                asked.clear()
            exports = []
            all_pairs = sorted(context.GetPertinentPairsForTags(set(), True, False, True))
            table.SetData(all_pairs)
            table.Sort()
            for selection in EXPORTS:
                table.clearSelection()
                table.SelectDatas(selection)
                exports.append({'selected': [list(p) for p in selection], 'text': panel._GetExportString()})
            out.append({'kind': name, 'local': local, 'service': service.GetName(), 'initial': [list(p) for p in INITIAL], 'sort': list(table.model()._column_list_status.GetSort()), 'display_order': [list(p) for p in table.GetData()], 'steps': steps, 'exports': exports})
            panel.deleteLater()
        shutil.rmtree(tmp)
        return out
    result = controller.CallBlockingToQt(controller.gui, work)
    with open(OUT, 'w') as f: json.dump(result, f, indent=2, ensure_ascii=False); f.write('\n')

if __name__ == '__main__':
    db_dir = record_api.unpack_fixture('repositories')
    try:
        run_client(db_dir, record)
    finally:
        shutil.rmtree(db_dir)

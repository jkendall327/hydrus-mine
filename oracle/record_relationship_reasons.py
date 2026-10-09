#!/usr/bin/env python3
"""Record the reason suggestions of the siblings/parents petition questions.

On the repositories fixture's tag repository (where the account is not a
moderator, so a reason is asked), each panel's own TagPairActionContext pends
and petitions pairs. `EnterText` is replaced by a script that records the
message and the suggestions it was offered, then answers with a typed reason
or one of the offered suggestions. After every answer the client options'
recent petition reasons are recorded. The count of reasons remembered is set
through the options object, as the Options page does; the 0 case shows that
nothing is then offered or remembered.
"""
import json
import os
import shutil
import sys
HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
from hydrus_driver import run_client
import record_api
OUT = os.path.join(HERE, 'fixtures', 'relationship_reasons.json')

def record(session):
    controller = session.controller
    def work():
        from hydrus.client.gui import ClientGUIAsync, ClientGUIDialogsQuick
        from hydrus.client.gui.metadata import ClientGUIManageTagSiblings as S, ClientGUIManageTagParents as P
        from hydrus.core import HydrusConstants as HC, HydrusExceptions
        class Job:
            def __init__(self, win, w, p, **kw): self.w, self.p = w, p
            def start(self): self.p(self.w())
        class Updater:
            def __init__(self, *args, **kwargs): pass
            def update(self): pass
        ClientGUIAsync.AsyncQtJob = Job
        ClientGUIAsync.AsyncQtUpdater = Updater
        options = controller.new_options
        script = []
        asked = []
        def enter_text(win, message, **kwargs):
            suggestions = list(kwargs.get('suggestions') or [])
            answer = script.pop(0)
            asked.append({'message': message, 'suggestions': suggestions, 'answer': answer})
            if answer is None:
                raise HydrusExceptions.CancelledException('cancelled')
            if isinstance(answer, int):
                return suggestions[answer]
            return answer
        ClientGUIDialogsQuick.EnterText = enter_text
        repository = controller.services_manager.GetServices((HC.TAG_REPOSITORY,))[0].GetServiceKey()
        out = {'default_count': options.GetInteger('num_recent_petition_reasons'), 'runs': []}
        initial = [('a', 'b'), ('b', 'c')]
        # (action, pair, answer): an answer is typed text, the index of the
        # suggestion button pressed, or None for cancel
        actions = [
            ('add', ('x1', 'y1'), 'first typed'),
            ('add', ('x2', 'y2'), 'second typed'),
            ('add', ('x3', 'y3'), 'third typed'),
            ('add', ('x4', 'y4'), -1),
            ('add', ('x5', 'y5'), 1),
            ('add', ('x6', 'y6'), None),
            ('delete', ('a', 'b'), 'removal typed'),
            ('delete', ('b', 'c'), -1),
        ]
        for count in [2, 0]:
            for name, cls, attr in [('siblings', S.ManageTagSiblings, '_sibling_action_context'), ('parents', P.ManageTagParents, '_parent_action_context')]:
                options.SetInteger('num_recent_petition_reasons', count)
                options._dictionary['recent_petition_reasons'] = type(options._dictionary['recent_petition_reasons'])()
                panel = cls._Panel(controller.gui, repository)
                context = getattr(panel, attr)
                context._AddStatusesToPairsToCache({HC.CONTENT_STATUS_CURRENT: set(initial)})
                context._have_fetched_all = True
                context._have_fetched_pending_and_petitioned = True
                content_type = HC.CONTENT_TYPE_TAG_SIBLINGS if name == 'siblings' else HC.CONTENT_TYPE_TAG_PARENTS
                steps = []
                for op, pair, answer in actions:
                    script[:] = [answer]
                    context.EnterCleanedPairs(panel, [pair], only_add=op == 'add', only_remove=op == 'delete')
                    steps.append({
                        'action': op,
                        'pair': list(pair),
                        'asked': list(asked),
                        'recent_add': options.GetRecentPetitionReasons(content_type, HC.CONTENT_UPDATE_ADD),
                        'recent_delete': options.GetRecentPetitionReasons(content_type, HC.CONTENT_UPDATE_DELETE),
                        'updates': sorted([[u.GetAction(), list(u.GetRow()), u.GetReason()] for u in context.GetContentUpdates()], key=str),
                    })
                    asked.clear()
                out['runs'].append({'kind': name, 'count': count, 'initial': [list(p) for p in initial], 'steps': steps})
                panel.deleteLater()
        return out
    result = controller.CallBlockingToQt(controller.gui, work)
    with open(OUT, 'w') as f:
        json.dump(result, f, indent=2, ensure_ascii=False)
        f.write('\n')

if __name__ == '__main__':
    db_dir = record_api.unpack_fixture('repositories')
    try:
        run_client(db_dir, record)
    finally:
        shutil.rmtree(db_dir)

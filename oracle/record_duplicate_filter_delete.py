#!/usr/bin/env python3
"""Record deleting from the duplicate filter (`CanvasFilterDuplicates._Delete`),
on the auto-resolution fixture's potential pairs, in batches of six as
record_viewer_top_hover.py's duplicates scenario has them.

The trash button of the top hover (`canvas_delete`) and the delete key
(`SIMPLE_DELETE_FILE`, the 'media' shortcut set's) both ask "Delete just this
file, or both?" (`GetYesYesNo`, answered here as scripted), then show the
delete files dialog (`GetDeleteFilesJobs`: the real `EditDeleteFilesPanel`,
accepted as it opens), and record the deletion as the pair's decision, going
on to the next pair. Recorded, step by step: the question and its answers,
the delete dialog's files, default reason and choices, the index string and
the pair shown after; going back over a deletion; the end of the batch's
"commit ... and continue?" question (accepted); and, once committed, each
file's local domains, trash and deletion reason.

Time is held still. Usage:
QT_QPA_PLATFORM=offscreen ~/pyenv/bin/python oracle/record_duplicate_filter_delete.py
(writes fixtures/duplicate_filter_delete.json)
"""

import json
import os
import sys
import time

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)

OUT = os.path.join(HERE, 'fixtures', 'duplicate_filter_delete.json')

NOW = 1900000000


def hold_time():
    from hydrus.core import HydrusTime
    HydrusTime.GetNow = lambda: NOW
    HydrusTime.GetNowMS = lambda: NOW * 1000
    HydrusTime.GetNowFloat = lambda: float(NOW)


def record(session):
    controller = session.controller
    gui = controller.gui
    hold_time()

    manager = controller.duplicates_auto_resolution_manager
    manager._AbleToWorkIdleNormal = lambda: False
    manager._AbleToWorkActiveNormal = lambda: False

    from qtpy import QtWidgets as QW
    from hydrus.core import HydrusExceptions
    from hydrus.client import ClientConstants as CC
    from hydrus.client import ClientApplicationCommand as CAC
    from hydrus.client import ClientGlobals as CG
    from hydrus.client.duplicates import ClientDuplicates
    from hydrus.client.duplicates import ClientPotentialDuplicatesPairFactory
    from hydrus.client.duplicates import ClientPotentialDuplicatesSearchContext
    from hydrus.client.gui import ClientGUIDialogsQuick
    from hydrus.client.gui import ClientGUITopLevelWindowsPanels
    from hydrus.client.gui.canvas import ClientGUICanvasDuplicates, ClientGUICanvasFrame
    from hydrus.client.gui.panels import ClientGUIScrolledPanelsCommitFiltering
    from hydrus.client.gui.panels import ClientGUIScrolledPanelsEdit
    from hydrus.core import HydrusLists

    # the batch, as record_viewer_top_hover.py fetches it
    import sqlite3
    master = sqlite3.connect('file:' + os.path.join(controller.db_dir, 'client.master.db') + '?mode=ro', uri=True)
    hash_ids = {bytes(h): i for (i, h) in master.execute('SELECT hash_id, hash FROM hashes;')}
    master.close()

    def do_search_work(factory, *args):
        search = factory._potential_duplicate_pairs_fragmentary_search
        rows = []
        while not search.SearchDone():
            found = CG.client_controller.Read('potential_duplicate_media_result_pairs_and_distances_fragmentary', search, no_more_than=10 ** 9)
            rows.extend(list(found.IterateRows()))
        rows.sort(key=lambda row: (hash_ids[row[0].GetHash()], hash_ids[row[1].GetHash()]))
        everything = ClientPotentialDuplicatesSearchContext.PotentialDuplicateMediaResultPairsAndDistances(rows)
        everything.Sort(factory._duplicate_pair_sort_type, factory._duplicate_pair_sort_asc)
        chosen = list(everything.IterateRows())[:factory._no_more_than]
        chosen.sort(key=lambda row: (hash_ids[row[0].GetHash()], hash_ids[row[1].GetHash()]))
        for row in chosen:
            factory._fetched_media_result_pairs_and_distances.AppendRow(row)
        return True
    ClientPotentialDuplicatesPairFactory.PotentialDuplicatePairFactoryDBMixed.DoSearchWork = do_search_work
    HydrusLists.RandomiseListByChunks = lambda items, chunk: list(items)

    events = []
    answers = []

    def yes_yes_no(win, message, title='Are you sure?', yes_tuples=None, no_label='no'):
        answer = answers.pop(0)
        events.append({'asked': message, 'title': title, 'yes': [label for (label, value) in yes_tuples], 'no': no_label, 'answer': answer})
        if answer == 'no':
            raise HydrusExceptions.CancelledException('no')
        return dict((label, value) for (label, value) in yes_tuples)[answer]
    ClientGUIDialogsQuick.GetYesYesNo = yes_yes_no

    panels = []
    original_panel_init = ClientGUIScrolledPanelsEdit.EditDeleteFilesPanel.__init__

    def panel_init(self, parent, media, default_reason, suggested_file_service_key=None):
        original_panel_init(self, parent, media, default_reason, suggested_file_service_key=suggested_file_service_key)
        panels.append(self)
        events.append({
            'delete dialog': [m.GetHash().hex() for m in media],
            'default_reason': default_reason,
            'suggested': None if suggested_file_service_key is None else suggested_file_service_key.hex(),
            'actions': [label for (label, value) in self._permitted_action_choices],
            'action': self._action_radio.GetValue()[4],
            'reason': self._GetReason(),
        })
    ClientGUIScrolledPanelsEdit.EditDeleteFilesPanel.__init__ = panel_init
    # (the dialog accepted as it opens, or cancelled where scripted)
    exec_answers = []

    def dialog_exec(self):
        accepted = exec_answers.pop(0) if exec_answers else True
        events.append({'delete dialog answer': 'delete' if accepted else 'cancel'})
        return QW.QDialog.DialogCode.Accepted if accepted else QW.QDialog.DialogCode.Rejected
    ClientGUITopLevelWindowsPanels.DialogEdit.exec = dialog_exec

    def finish(win, label):
        events.append({'finish': label, 'answer': 'forget'})
        return (QW.QDialog.DialogCode.Rejected, False)
    ClientGUIScrolledPanelsCommitFiltering.GetFinishFilteringAnswer = finish

    def interstitial(win, label):
        events.append({'interstitial': label})
        return QW.QDialog.DialogCode.Accepted
    ClientGUIScrolledPanelsCommitFiltering.GetInterstitialFilteringAnswer = interstitial

    holder = {}

    def qt(f):
        return controller.CallBlockingToQt(gui, f)

    def make():
        context = ClientPotentialDuplicatesSearchContext.PotentialDuplicatesSearchContext()
        context.SetMaxHammingDistance(4)
        factory = ClientPotentialDuplicatesPairFactory.PotentialDuplicatePairFactoryDBMixed(context, ClientDuplicates.DUPE_PAIR_SORT_MAX_FILESIZE, False, 6)
        frame = ClientGUICanvasFrame.CanvasFrame(gui)
        canvas = ClientGUICanvasDuplicates.CanvasFilterDuplicates(frame, factory)
        frame.SetCanvas(canvas)
        frame.showNormal()
        frame.resize(1000, 750)
        holder['frame'] = frame
        holder['canvas'] = canvas
    qt(make)

    deadline = time.time() + 60
    while time.time() < deadline:
        time.sleep(0.3)
        if qt(lambda: holder['canvas']._current_media is not None and len(holder['canvas']._batch_of_pairs_to_process) > 0):
            break
    else:
        raise Exception('the filter never showed a pair')
    time.sleep(1.0)

    # the batch's files archived, and the delete lock's reinbox option on, so
    # deleting re-inboxes them (`delete_lock_reinbox_deletees_after_duplicate_filter`)
    from hydrus.core import HydrusConstants as HC
    from hydrus.client.metadata import ClientContentUpdates
    batch_hashes = qt(lambda: sorted({r.GetHash() for pair in holder['canvas']._batch_of_pairs_to_process for r in pair}))
    cu = ClientContentUpdates.ContentUpdate(HC.CONTENT_TYPE_FILES, HC.CONTENT_UPDATE_ARCHIVE, batch_hashes)
    controller.WriteSynchronous('content_updates', ClientContentUpdates.ContentUpdatePackage.STATICCreateFromContentUpdate(CC.HYDRUS_LOCAL_FILE_STORAGE_SERVICE_KEY, cu))
    controller.new_options.SetBoolean('delete_lock_reinbox_deletees_after_duplicate_filter', True)

    def settle(seconds=0.5):
        deadline = time.time() + seconds
        while time.time() < deadline:
            QW.QApplication.processEvents()
            time.sleep(0.02)

    def drive():
        canvas = holder['canvas']

        def now():
            media = canvas._current_media
            return {
                'index': canvas._GetIndexString(),
                'shown': None if media is None else media.GetHash().hex(),
                'pair': canvas._current_pair_index,
            }

        steps = [{'start': now()}]

        def step(what, do, scripted=()):
            answers.extend(scripted)
            events.clear()
            before = now()
            do()
            settle()
            steps.append({'did': what, 'answers': list(scripted), 'before': before, 'events': list(events), 'after': now()})

        delete_button = lambda: CG.client_controller.pub('canvas_delete', canvas._canvas_key)
        delete_key = lambda: canvas.ProcessApplicationCommand(CAC.ApplicationCommand.STATICCreateSimpleCommand(CAC.SIMPLE_DELETE_FILE))
        back = lambda: canvas.ProcessApplicationCommand(CAC.ApplicationCommand.STATICCreateSimpleCommand(CAC.SIMPLE_DUPLICATE_FILTER_BACK))
        skip = lambda: canvas.ProcessApplicationCommand(CAC.ApplicationCommand.STATICCreateSimpleCommand(CAC.SIMPLE_DUPLICATE_FILTER_SKIP))
        switch = lambda: canvas.ProcessApplicationCommand(CAC.ApplicationCommand.STATICCreateSimpleCommand(CAC.SIMPLE_VIEW_NEXT))

        step('trash button', delete_button, ['no'])
        step('trash button', delete_button, ['delete just this one'])
        step('switch', switch)
        step('delete key', delete_key, ['delete both'])
        step('back', back)
        step('skip', skip)
        exec_answers.append(False)
        step('trash button', delete_button, ['delete just this one'])
        step('delete key', delete_key, ['delete both'])
        # nothing is deleted before the commit
        steps.append({'files': state_of(batch_hex)})
        # on to the batch's end, skipping, until it commits
        for _ in range(8):
            committed = any('interstitial' in e for s in steps for e in s.get('events', []))
            if committed or canvas._current_media is None:
                break
            step('skip', skip)
        steps.append({'files': state_of(batch_hex)})
        # in the next batch: a deletion, then closing, forgetting it
        step('trash button', delete_button, ['delete just this one'])
        forgotten = [e['delete dialog'] for e in steps[-1]['events'] if 'delete dialog' in e][0]
        step('close', canvas.UserOKToClose)
        steps.append({'files': state_of(forgotten)})
        return steps

    def state_of(hexes):
        results = controller.Read('media_results', [bytes.fromhex(h) for h in sorted(hexes)])
        state = {}
        for r in results:
            lm = r.GetLocationsManager()
            state[r.GetHash().hex()] = {
                'in_my_files': CC.LOCAL_FILE_SERVICE_KEY in lm.GetCurrent(),
                'trashed': lm.IsTrashed(),
                'deletion_reason': lm.GetLocalFileDeletionReason() if hasattr(lm, 'GetLocalFileDeletionReason') else None,
                'inbox': r.GetInbox(),
            }
        return state

    batch_hex = [h.hex() for h in batch_hashes]

    out = {'archived': batch_hex, 'steps': qt(drive)}

    def done():
        holder['canvas']._duplicate_pair_decisions = []
        holder['canvas']._TryToCloseWindow = lambda: None
        holder['frame'].hide()
    qt(done)
    return out


def main():
    import hydrus_driver
    import record_api
    if len(sys.argv) > 1 and sys.argv[1] == '--child':
        output = sys.argv[2]
        result = hydrus_driver.run_client(record_api.unpack_fixture('auto_resolution'), record)
        with open(output, 'w') as stream:
            json.dump(result, stream)
        return
    import tempfile
    with tempfile.TemporaryDirectory() as work:
        out = os.path.join(work, 'out.json')
        hydrus_driver.run_in_subprocess(os.path.abspath(__file__), '--child', out)
        with open(out) as stream:
            recorded = json.load(stream)
    with open(OUT, 'w') as stream:
        json.dump(recorded, stream, indent=1)
        stream.write('\n')
    print('wrote', OUT)


if __name__ == '__main__':
    main()

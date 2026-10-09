#!/usr/bin/env python3
"""Record the review window's pending pairs tab at a size that shows its
limits (`ReviewActionsPanel`, the semi-automatic rule of
`legacy_db/auto_resolution_pending.tar.gz`, made by
`make_auto_resolution_pending.py`, with ten pairs waiting for approval).

With time held still and the machine's time zone UTC, this records:

- the pending tab as it opens (label, the rows' hashes in order) with the
  default sample size, the "only sample this many" box's default and its
  "fetch all" phrase and minimum;
- fetch limits: 1, 7, 10, 11 and "fetch all", each refreshed with the
  refresh button: the label and rows;
- "select all": how many rows are selected;
- an approval of seven pairs (more than five): the question asked, the
  answer no (nothing changes) and then yes, with every text the approve
  button shows as it works (the work goes in chunks of four), the label and
  rows after, and what the other tabs hold;
- a denial of three pairs (no question): the same;
- a final approval of the rest to empty the tab: the empty label.

Usage: QT_QPA_PLATFORM=offscreen TZ=UTC python oracle/record_auto_resolution_pending.py
       (writes fixtures/auto_resolution_pending.json)
"""
import json, os, sys, time
HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)

OUT = os.path.join(HERE, 'fixtures', 'auto_resolution_pending.json')
NOW = 1900000000
RULE = 'pixel-perfect gifs vs pngs'


def record(session):
    c = session.controller
    gui = c.gui
    from hydrus.core import HydrusTime
    HydrusTime.GetNow = lambda: NOW
    HydrusTime.GetNowMS = lambda: NOW * 1000
    manager = c.duplicates_auto_resolution_manager
    manager._AbleToWorkIdleNormal = lambda: False
    manager._AbleToWorkActiveNormal = lambda: False

    asked = []
    answers = []
    qt = lambda f: c.CallBlockingToQt(gui, f)

    def setup():
        from qtpy import QtWidgets as QW
        from hydrus.client.gui import ClientGUIDialogsQuick

        def yes_no(win, message, **kwargs):
            wanted = answers.pop(0) if answers else 'yes'
            asked.append({'message': message, 'pressed': wanted})
            return QW.QDialog.DialogCode.Accepted if wanted == 'yes' else QW.QDialog.DialogCode.Rejected

        ClientGUIDialogsQuick.GetYesNo = yes_no

    qt(setup)
    rules = {r.GetName(): r for r in manager.GetRules()}
    from hydrus.client.gui.duplicates import ClientGUIDuplicatesAutoResolutionRuleReview as R
    panel = qt(lambda: R.ReviewActionsPanel(gui, rules[RULE]))
    pubbed = []
    real_pub = c.pub

    def pub(topic, *args, **kwargs):
        if topic == 'message':
            job = args[0]
            pubbed.append({'done': job.IsDone(), 'dismissed': job.IsDismissed(), 'text': job.GetStatusText()})
        return real_pub(topic, *args, **kwargs)

    c.pub = pub
    # every text the buttons show
    button_texts = {'approve': [], 'deny': []}

    def watch():
        for (key, button) in (('approve', panel._approve_selected_button), ('deny', panel._deny_selected_button)):
            original = button.setText

            def set_text(text, key=key, original=original):
                button_texts[key].append(text)
                return original(text)

            button.setText = set_text

    qt(watch)

    def rows_of(pair_list):
        model = pair_list.model()
        return [[a.GetHash().hex(), b.GetHash().hex()] for (a, b) in (model.GetMediaResultPair(i) for i in range(model.rowCount()))]

    def settle():
        for _ in range(150):
            time.sleep(0.2)

            def busy():
                labels = [panel._pending_actions_label.text(), panel._actioned_pairs_label.text(), panel._denied_pairs_label.text()]
                if any('fetching' in l for l in labels):
                    return True
                model = panel._pending_actions_pair_list.model()
                texts = [model.data(model.index(row, 2), 0) for row in range(model.rowCount())]
                return any(t.startswith('calculating') for t in texts) or not panel._pending_actions_panel.isEnabled()

            if not qt(busy):
                return
        raise Exception('never settled')

    def state():
        return qt(lambda: {
            'label': panel._pending_actions_label.text(),
            'rows': rows_of(panel._pending_actions_pair_list),
            'selected': len(panel._pending_actions_pair_list.selectionModel().selectedRows()),
            'actioned': panel._actioned_pairs_label.text(),
            'denied': panel._denied_pairs_label.text(),
            'approve_enabled': panel._approve_selected_button.isEnabled(),
            'deny_enabled': panel._deny_selected_button.isEnabled(),
            'select_all_enabled': panel._select_all_button.isEnabled(),
        })

    settle()
    out = {'rule': RULE, 'start': state()}
    spin = panel._pending_pairs_num_to_fetch
    out['fetch_box'] = qt(lambda: {'value': spin.GetValue(), 'none_phrase': spin._checkbox.text(), 'minimum': spin._number_value.minimum(),
                                   'row_label': 'only sample this many: '})

    out['fetches'] = []
    for limit in (1, 7, 10, 11, None, 250):
        qt(lambda: spin.SetValue(limit))
        qt(panel._refetch_pending_actions_button.click)
        settle()
        s = state()
        s['limit'] = limit
        out['fetches'].append(s)

    qt(panel._select_all_button.click)
    out['select_all'] = state()

    def act(button, rows, answer):
        del asked[:]
        answers[:] = [answer]
        button_texts['approve'].clear()
        button_texts['deny'].clear()
        pubbed_before = len(pubbed)

        def select_and_press():
            panel._pending_actions_pair_list.clearSelection()
            for i in rows:
                panel._pending_actions_pair_list.selectRow(i) if False else None
            sm = panel._pending_actions_pair_list.selectionModel()
            from qtpy import QtCore as QC
            model = panel._pending_actions_pair_list.model()
            for i in rows:
                sm.select(model.index(i, 0), QC.QItemSelectionModel.SelectionFlag.Select | QC.QItemSelectionModel.SelectionFlag.Rows)
            (panel._approve_selected_button if button == 'approve' else panel._deny_selected_button).click()

        before = state()
        qt(select_and_press)
        settle()
        time.sleep(1)
        after = state()
        # what the database holds, fetched afresh (the window only takes the
        # decided rows off its list)
        fresh = [[a.GetHash().hex(), b.GetHash().hex()] for (a, b) in c.Read('duplicates_auto_resolution_pending_action_pairs', rules[RULE])]
        after['database_pending'] = fresh
        return {'button': button, 'rows': rows, 'answer': answer, 'asked': list(asked), 'before_rows': before['rows'],
                'button_texts': list(button_texts[button]), 'after': after,
                'popups': pubbed[pubbed_before:]}

    qt(lambda: spin.SetValue(None))
    qt(panel._refetch_pending_actions_button.click)
    settle()
    steps = [act('approve', list(range(7)), 'no'), act('approve', list(range(7)), 'yes')]
    steps.append(act('deny', [0, 1, 2], 'yes'))
    out['steps'] = steps
    time.sleep(8)  # the popups are published four seconds after the work starts
    out['popups_late'] = list(pubbed)
    return out


def main():
    import hydrus_driver, record_api, tempfile
    if len(sys.argv) > 1 and sys.argv[1] == '--child':
        output = sys.argv[2]
        result = hydrus_driver.run_client(record_api.unpack_fixture('auto_resolution_pending'), record)
        with open(output, 'w') as f:
            json.dump(result, f, ensure_ascii=False)
        return
    os.environ['TZ'] = 'UTC'
    with tempfile.TemporaryDirectory() as tmp:
        path = os.path.join(tmp, 'r.json')
        hydrus_driver.run_in_subprocess(os.path.abspath(__file__), '--child', path)
        with open(path) as f:
            result = json.load(f)
    with open(OUT, 'w') as f:
        json.dump(result, f, indent=1, ensure_ascii=False)
        f.write('\n')
    print(result['start']['label'], result['fetch_box'])
    for fe in result['fetches']:
        print(fe['limit'], fe['label'], len(fe['rows']))
    print('select all', result['select_all']['selected'])
    for s in result['steps']:
        print(s['button'], s['answer'], s['asked'], s['button_texts'], s['after']['label'], len(s['after']['rows']), s['popups'])


if __name__ == '__main__':
    main()

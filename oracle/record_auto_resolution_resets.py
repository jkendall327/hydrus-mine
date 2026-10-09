#!/usr/bin/env python3
"""Record the auto-resolution tab's maintenance buttons on the reference's
own database (`ReviewDuplicatesAutoResolutionPanel`).

On the database `record_auto_resolution.py` left (`legacy_db/auto_resolution
.tar.gz`: rules with pairs in several statuses, one with a denied pair), the
real panel is made over the real rules and, in a fixed order, each button is
pressed with some rules selected or none, its question answered by script
(yes, or no): "reset search", "reset test", "reset denied", "regen numbers"
and "resync". After each the record has the question asked (none if there
were no rules), the answer, and every rule's pair counts by status, read from
the database (the manager's cached numbers regenerated first, so they are
the database's own).

Statuses are `DUPLICATE_STATUS_*`: 0 did not match the search, 1 matches but
not tested, 2 failed the test, 3 actioned, 4 not searched, 5 ready to action,
6 denied.

Usage: QT_QPA_PLATFORM=offscreen TZ=UTC python oracle/record_auto_resolution_resets.py
       (writes fixtures/auto_resolution_resets.json)
"""
import json, os, sys, time
HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)

OUT = os.path.join(HERE, 'fixtures', 'auto_resolution_resets.json')
NOW = 1900000000

# (button, rules selected by name (None: none selected), answer)
STEPS = [
    ('reset_search', ['pixel-perfect gifs vs pngs'], 'no'),
    ('reset_denied', ['pixel-perfect gifs vs pngs'], 'yes'),
    ('reset_test', ['pixel-perfect pairs'], 'yes'),
    ('reset_search', ['pixel-perfect pairs'], 'yes'),
    ('reset_test', None, 'yes'),
    ('reset_denied', None, 'no'),
    ('reset_denied', None, 'yes'),
    ('reset_search', None, 'yes'),
    ('regen_numbers', None, 'no'),
    ('regen_numbers', None, 'yes'),
    ('resync', None, 'yes'),
]


def record(session):
    c = session.controller
    gui = c.gui
    from hydrus.core import HydrusTime

    HydrusTime.GetNow = lambda: NOW
    HydrusTime.GetNowMS = lambda: NOW * 1000
    HydrusTime.GetNowFloat = lambda: float(NOW)
    manager = c.duplicates_auto_resolution_manager
    manager._AbleToWorkIdleNormal = lambda: False
    manager._AbleToWorkActiveNormal = lambda: False

    asked = []
    answers = []

    def setup():
        from qtpy import QtWidgets as QW
        from hydrus.client.gui import ClientGUIDialogsQuick

        def yes_no(win, message, **kwargs):
            wanted = answers.pop(0) if answers else 'yes'
            asked.append({'message': message, 'title': kwargs.get('title', 'Are you sure?'), 'pressed': wanted})
            return QW.QDialog.DialogCode.Accepted if wanted == 'yes' else QW.QDialog.DialogCode.Rejected

        ClientGUIDialogsQuick.GetYesNo = yes_no

    c.CallBlockingToQt(gui, setup)

    def make():
        from hydrus.client.gui.duplicates import ClientGUIDuplicatesAutoResolution as G
        return G.ReviewDuplicatesAutoResolutionPanel(gui)

    panel = c.CallBlockingToQt(gui, make)
    c.CallBlockingToQt(gui, lambda: (panel.resize(900, 600), panel.show(), panel.PageShown()))

    def rows():
        model = panel._duplicates_auto_resolution_rules.model()
        return [model.GetData(i) if hasattr(model, 'GetData') else None for i in range(model.rowCount())]

    # wait for the rules to be listed
    for _ in range(200):
        time.sleep(0.2)
        if c.CallBlockingToQt(gui, lambda: len(panel._duplicates_auto_resolution_rules.GetData())) > 0:
            break
    else:
        raise Exception('the rules never listed')

    def counts():
        c.WriteSynchronous('duplicates_auto_resolution_maintenance_regen_numbers')
        out = {}
        for rule in c.Read('duplicates_auto_resolution_rules_with_counts'):
            out[rule.GetName()] = {str(k): v for (k, v) in sorted(rule.GetCountsCacheDuplicate().items()) if v}
        return out

    buttons = {'reset_search': lambda: panel._ResetSearch(), 'reset_test': lambda: panel._ResetTest(),
               'reset_denied': lambda: panel._ResetDenied(), 'regen_numbers': lambda: panel._RegenNumbers(),
               'resync': lambda: panel._ResyncRulesToLocationContexts()}

    def select(names):
        lst = panel._duplicates_auto_resolution_rules
        lst.clearSelection()
        if names:
            lst.SelectDatas([rule for rule in lst.GetData() if rule.GetName() in names])

    out = {'start': counts(), 'steps': []}
    for (button, names, answer) in STEPS:
        c.CallBlockingToQt(gui, lambda: select(names))
        del asked[:]
        answers[:] = [answer]
        c.CallBlockingToQt(gui, buttons[button])
        time.sleep(1)
        out['steps'].append({'button': button, 'selected': names, 'answer': answer, 'asked': list(asked), 'counts': counts()})
    return out


def main():
    import hydrus_driver, record_api, tempfile
    if len(sys.argv) > 1 and sys.argv[1] == '--child':
        output = sys.argv[2]
        result = hydrus_driver.run_client(record_api.unpack_fixture('auto_resolution'), record)
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
    print(json.dumps(result['start']))
    for s in result['steps']:
        print(s['button'], s['selected'], s['answer'], [a['message'][:60] for a in s['asked']], json.dumps(s['counts']))


if __name__ == '__main__':
    main()

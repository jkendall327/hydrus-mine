#!/usr/bin/env python3
"""Record the duplicates page's filtering tab: its pair sort and group mode
choosers, and its "quick and dirty processing" buttons.

On the database `record_auto_resolution.py` left (`legacy_db/auto_resolution
.tar.gz`), a real duplicates page is opened (on "my files"). The record holds:

- `sort`: the pair sort chooser (`PotentialDuplicatesSortWidget`) for each
  sort type: its label, whether the direction chooser shows, and the
  direction choices (label and value), with the value read back;
- `group_mode`: the "mixed pairs"/"group mode" chooser's choices and what
  choosing each sets on the page;
- `buttons`: the quick processing buttons' labels;
- `steps`: "show some random potential duplicates" (its random pick pinned
  to the lowest file, as hydrus-rs's tests pin theirs), then the "set current
  media as ..." buttons with the "Are you sure?" question answered no and
  yes: after each, the question asked, the files the page shows (in order),
  and the relationships of the files concerned.

Usage: QT_QPA_PLATFORM=offscreen TZ=UTC python oracle/record_duplicates_quick_processing.py
       (writes fixtures/duplicates_quick_processing.json)
"""
import json, os, sys, time
HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)

OUT = os.path.join(HERE, 'fixtures', 'duplicates_quick_processing.json')
NOW = 1900000000

# (button, answer to "Are you sure?")
STEPS = [
    ('show', None),
    ('alternates', 'no'),
    ('alternates', 'yes'),
    ('false_positive', 'yes'),
    ('same_quality', 'yes'),
    ('show', None),
]


def record(session):
    c = session.controller
    gui = c.gui
    from qtpy import QtWidgets as QW
    from hydrus.core import HydrusConstants as HC, HydrusTime, HydrusLists
    from hydrus.client import ClientConstants as CC, ClientLocation
    from hydrus.client.duplicates import ClientDuplicates
    from hydrus.client.gui import ClientGUIDialogsQuick, ClientGUIDialogsMessage
    from hydrus.client.gui.duplicates import ClientGUIPotentialDuplicatesSearchContext as SC
    from hydrus.client.db import ClientDBFilesDuplicatesFileSearch

    HydrusTime.GetNow = lambda: NOW
    HydrusTime.GetNowMS = lambda: NOW * 1000
    manager = c.duplicates_auto_resolution_manager
    manager._AbleToWorkIdleNormal = lambda: False
    manager._AbleToWorkActiveNormal = lambda: False

    class Lowest:
        @staticmethod
        def choice(seq):
            return min(seq)

    ClientDBFilesDuplicatesFileSearch.random = Lowest
    HydrusLists.RandomiseListByChunks = lambda items, chunk: list(items)

    asked = []
    answers = []

    def yes_no(win, message, **kwargs):
        wanted = answers.pop(0) if answers else 'yes'
        asked.append({'yes_no': message, 'title': kwargs.get('title', 'Are you sure?'), 'pressed': wanted})
        return QW.QDialog.DialogCode.Accepted if wanted == 'yes' else QW.QDialog.DialogCode.Rejected

    def info(win, message, *a, **k):
        asked.append({'information': message})

    ClientGUIDialogsQuick.GetYesNo = yes_no
    ClientGUIDialogsMessage.ShowInformation = info
    qt = lambda f: c.CallBlockingToQt(gui, f)

    out = {}

    def widgets():
        w = SC.PotentialDuplicatesSortWidget(gui, ClientDuplicates.DUPE_PAIR_SORT_MAX_FILESIZE, False)
        sorts = []
        for (label, sort_type) in list(w._sort_type._choice_tuples):
            w._sort_type.SetValue(sort_type)
            row = {'label': label, 'type': sort_type,
                   'direction_shown': not w._sort_asc.isHidden(),
                   'directions': [{'label': l, 'asc': v} for (l, v) in w._sort_asc._choice_tuples]}
            for (l, v) in w._sort_asc._choice_tuples:
                w._sort_asc.SetValue(v)
                row.setdefault('values', []).append(list(w.GetValue()))
            sorts.append(row)
        w.deleteLater()
        return sorts

    out['sort'] = qt(widgets)

    def open_page():
        page = gui._notebook.NewPageDuplicateFilter(location_context=ClientLocation.LocationContext.STATICCreateSimple(CC.LOCAL_FILE_SERVICE_KEY))
        return page

    page = qt(open_page)
    time.sleep(2)
    sidebar = page.GetSidebar()
    panel = sidebar._filter_panel

    def group_choices():
        rows = []
        for (label, value) in panel._filter_group_mode._choice_tuples:
            panel._filter_group_mode.SetValue(value)
            rows.append({'label': label, 'value': value, 'page': panel._page_manager.GetVariable('filter_group_mode')})
        panel._filter_group_mode.SetValue(False)
        return rows

    out['group_mode'] = qt(group_choices)
    out['buttons'] = qt(lambda: [b.text() for b in (panel._show_some_dupes, panel._set_random_as_same_quality_button,
                                                    panel._set_random_as_alternates_button, panel._set_random_as_false_positives_button)])
    out['launch'] = qt(lambda: panel._launch_filter.text())

    def shown():
        mp = page.GetMediaResultsPanel()
        return [m.GetHash().hex() for m in mp._sorted_media]

    def wait_shown(before):
        deadline = time.time() + 30
        while time.time() < deadline:
            time.sleep(0.3)
            now = qt(shown)
            if now != before and len(now) > 0:
                return now
        return qt(shown)

    buttons = {'show': lambda: panel.ShowRandomPotentialDupes(),
               'alternates': lambda: panel._set_random_as_alternates_button.click(),
               'false_positive': lambda: panel._set_random_as_false_positives_button.click(),
               'same_quality': lambda: panel._set_random_as_same_quality_button.click()}
    steps = []
    for (button, answer) in STEPS:
        before = qt(shown)
        asked_before = len(asked)
        answers[:] = [answer] if answer else []
        qt(buttons[button])
        if button == 'show' or answer == 'yes':
            after = wait_shown(before)
        else:
            time.sleep(1)
            after = qt(shown)
        time.sleep(1)
        after = qt(shown)
        concerned = sorted(set(before) | set(after))
        location = ClientLocation.LocationContext.STATICCreateSimple(CC.COMBINED_LOCAL_FILE_DOMAINS_SERVICE_KEY)
        rel = c.Read('file_relationships_for_api', location, [bytes.fromhex(h) for h in concerned]) if concerned else {}
        steps.append({'button': button, 'answer': answer, 'asked': asked[asked_before:], 'before': before, 'shown': after,
                      'relationships': {(h.hex() if isinstance(h, bytes) else h): r for (h, r) in rel.items()}})
    out['steps'] = steps
    return out


def main():
    import hydrus_driver, record_api, tempfile
    if len(sys.argv) > 1 and sys.argv[1] == '--child':
        output = sys.argv[2]
        result = hydrus_driver.run_client(record_api.unpack_fixture('auto_resolution'), record)
        with open(output, 'w') as f:
            json.dump(result, f, ensure_ascii=False)
        return
    with tempfile.TemporaryDirectory() as tmp:
        path = os.path.join(tmp, 'r.json')
        hydrus_driver.run_in_subprocess(os.path.abspath(__file__), '--child', path)
        with open(path) as f:
            result = json.load(f)
    with open(OUT, 'w') as f:
        json.dump(result, f, indent=1, ensure_ascii=False)
        f.write('\n')
    for s in result['steps']:
        print(s['button'], s['answer'], [a.get('yes_no', a.get('information', ''))[:90] for a in s['asked']], len(s['before']), len(s['shown']))


if __name__ == '__main__':
    main()

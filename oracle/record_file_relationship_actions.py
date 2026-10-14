#!/usr/bin/env python3
"""Record the thumbnail menu's manage > file relationships actions.

On a page of the `basic` fixture's JPEGs (none of them have relationships to
begin with), files are selected (the first is the focus) and the real grid's
`GetMenu` is built; the "file relationships" submenu is recorded as a tree,
one of its rows is triggered with `GetYesNo` and `ShowWarning` captured (every
question is asked of the scripted answer, yes), and the relationships of the
files involved are read back with the reference's own `file_duplicate_info`.
"View N duplicates" rows open a page, whose name and files are recorded.

Usage: QT_QPA_PLATFORM=offscreen python oracle/record_file_relationship_actions.py
       (writes fixtures/file_relationship_actions.json)
"""
import json
import os
import sys
import time

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
OUT = os.path.join(HERE, 'fixtures', 'file_relationship_actions.json')
MANIFEST = json.load(open(os.path.join(HERE, 'fixtures', 'legacy_db', 'basic.manifest.json')))
NAMES = {f['name']: f['hash'] for f in MANIFEST['files']}

A, B, E, F, C, D = ('jpeg_03.jpg', 'jpeg_04.jpg', 'jpeg_05.jpg', 'jpeg_06.jpg', 'jpeg_08.jpg', 'jpeg_09.jpg')
WIPE = 'DUPLICATE WIPE: dissolve this file\'s duplicate group'
# (selection, the focus first; the path of the row under "file relationships")
BETTER = 'set this file as better than the 1 other selected'
ALTERNATES = 'set all selected as alternates'
STEPS = [
    ([A, B], [BETTER]),
    ([A], ['view 1 duplicates']),
    ([B], ['view 1 duplicates']),
    ([B, A], ['set all selected as same quality duplicates']),
    ([A], ['set this file as the best quality of its group']),
    ([B], ['set this file as the best quality of its group']),
    ([A], ['remove for this file', 'remove this file from its duplicate group']),
    ([A, B], [BETTER]),
    ([A], ['reset for this file', WIPE]),
    ([A, B], [BETTER]),
    ([A, B], ['advanced: reset for all selected', 'DUPLICATE WIPE: completely dissolve these files\' duplicate groups']),
    ([E, F], [ALTERNATES]),
    ([E], ['view 1 alternates']),
    ([E], ['remove for this file', 'remove this file\'s duplicate group from its alternate group']),
    ([E, F], [ALTERNATES]),
    ([E], ['reset for this file', 'EVEN BIGGER WIPE: dissolve this file\'s alternate group']),
    ([E, F], [ALTERNATES]),
    ([E, F], ['advanced: reset for all selected', 'EVEN BIGGER WIPE: completely dissolve these files\' alternate groups']),
    ([C, D], ['set all possible pair combinations as \'potential\' duplicates for the duplicates filter.']),
    ([C], ['view 1 potential duplicates']),
    ([C], ['reset for this file', 'schedule this file to be searched for potentials again']),
    ([C], ['reset for this file', 'delete all this file\'s potential relationships']),
    ([C], ['view 1 potential duplicates']),
    ([C, D], ['set all possible pair combinations as \'potential\' duplicates for the duplicates filter.']),
    ([C, D], ['advanced: reset for all selected', 'delete these files\' potential relationships']),
    # false positives (set between E and F directly, as the duplicate filter's
    # "not related" decision does)
    ('fp', [E, F]),
    ([E], ['view 1 not related/false positive']),
    ([E], ['reset for this file', 'delete all false-positive relationships this file\'s alternate group has with other groups']),
    ('fp', [E, F]),
    ([E, F], ['remove for all selected', 'delete all false-positive relationships these files\' alternate groups have between each other']),
    ('fp', [E, F]),
    ([E, F], ['advanced: reset for all selected', 'delete all false-positive relationships these files\' alternate groups have with other groups']),
]
INVOLVED = [A, B, E, F, C, D]


def record(session):
    try:
        return _record(session)
    except BaseException:
        import traceback
        traceback.print_exc()
        sys.stderr.flush()
        os._exit(1)


def _record(session):
    import inspect
    from qtpy import QtWidgets as QW
    from hydrus.core import HydrusConstants as HC
    from hydrus.client import ClientConstants as CC, ClientLocation
    from hydrus.client.gui import ClientGUIDialogsMessage, ClientGUIDialogsQuick
    from hydrus.client.media import ClientMediaFileFilter
    controller = session.controller
    gui = controller.gui
    hashes = [bytes.fromhex(h) for h in NAMES.values() if True]
    jpegs = [bytes.fromhex(NAMES[n]) for n in NAMES if n.endswith('.jpg')]
    defaults = inspect.signature(ClientGUIDialogsQuick.GetYesNo).parameters

    def qt(f):
        return controller.CallBlockingToQt(gui, f)

    location = ClientLocation.LocationContext.STATICCreateSimple(CC.LOCAL_FILE_SERVICE_KEY)
    page = qt(lambda: gui._notebook.NewPageQuery(location, initial_hashes=jpegs, page_name='oracle jpegs'))
    panel = None
    for _ in range(600):
        panel = qt(lambda: page.GetMediaResultsPanel())
        if hasattr(panel, 'GetMenu') and len(panel._sorted_media) > 0:
            break
        time.sleep(0.05)
    media_by_hash = qt(lambda: {m.GetHash(): m for m in panel._sorted_media})
    asked = []
    old_yesno = ClientGUIDialogsQuick.GetYesNo
    old_warning = ClientGUIDialogsMessage.ShowWarning

    def yesno(win, message, **kwargs):
        asked.append(dict(message=message, title=kwargs.get('title', defaults['title'].default),
                          yes=kwargs.get('yes_label', defaults['yes_label'].default),
                          no=kwargs.get('no_label', defaults['no_label'].default)))
        return QW.QDialog.DialogCode.Accepted

    def warn(win, message):
        asked.append(dict(warning=message))
    ClientGUIDialogsQuick.GetYesNo = yesno
    ClientGUIDialogsMessage.ShowWarning = warn

    def tree(menu):
        out = []
        for action in menu.actions():
            if action.isSeparator():
                continue
            if action.menu() is not None:
                out.append({'menu': action.text(), 'entries': tree(action.menu())})
            else:
                out.append(action.text())
        return out

    def select(names):
        panel._Select(ClientMediaFileFilter.FileFilter(ClientMediaFileFilter.FILE_FILTER_NONE))
        picked = [media_by_hash[bytes.fromhex(NAMES[n])] for n in names]
        for i, m in enumerate(picked):
            panel._HitMedia(m, i > 0, False)
        panel.SetFocusedMedia(picked[0])

    def relationships_menu():
        menu = panel.GetMenu()
        for part in ('manage', 'file relationships'):
            menu = [a for a in menu.actions() if a.menu() is not None and a.text() == part][0].menu()
        return menu

    def info(name):
        i = controller.Read('file_duplicate_info', location, bytes.fromhex(NAMES[name]))
        return {'is_king': i['is_king'], 'counts': {HC.duplicate_type_string_lookup[k]: v for k, v in sorted(i['counts'].items())}}

    def everything():
        return {n: info(n) for n in INVOLVED}

    steps = []
    try:
        initial = everything()
        for names, path in STEPS:
            if names == 'fp':
                controller.WriteSynchronous('duplicate_pair_status', [(HC.DUPLICATE_FALSE_POSITIVE, bytes.fromhex(NAMES[path[0]]), bytes.fromhex(NAMES[path[1]]), [])])
                steps.append({'setup': 'false positive', 'selection': [NAMES[n] for n in path], 'after': everything()})
                continue
            asked.clear()
            tabs = qt(lambda: gui._notebook.count())

            built = {}

            def before():
                select(names)
                built['menu'] = relationships_menu()
                return tree(built['menu'])
            menu_tree = qt(before)

            def trigger():
                # (the very menu recorded)
                menu = built['menu']
                for part in path:
                    found = [a for a in menu.actions() if a.text() == part]
                    if not found:
                        return False
                    action = found[0]
                    if action.menu() is not None:
                        menu = action.menu()
                    else:
                        action.trigger()
                        return True
            offered = qt(trigger)
            time.sleep(0.6)

            def opened():
                if gui._notebook.count() == tabs:
                    return None
                new = gui._notebook.GetPageFromIndex(gui._notebook.count() - 1) if hasattr(gui._notebook, 'GetPageFromIndex') else gui._notebook.widget(gui._notebook.count() - 1)
                return {'name': new.GetName(), 'tabs_added': gui._notebook.count() - tabs, 'hashes': None}
            made = qt(opened)
            if made is not None:
                for _ in range(200):
                    panel2 = qt(lambda: gui._notebook.widget(gui._notebook.count() - 1).GetMediaResultsPanel())
                    if hasattr(panel2, 'GetMenu') and len(panel2._sorted_media) > 0:
                        break
                    time.sleep(0.05)
                made['hashes'] = sorted(qt(lambda: [m.GetHash().hex() for m in panel2._sorted_media]))
                qt(lambda: gui._notebook.setCurrentIndex(0))
            steps.append({'selection': [NAMES[n] for n in names], 'path': path, 'offered': bool(offered), 'menu': menu_tree,
                          'asked': list(asked), 'page': made, 'after': everything()})
    finally:
        ClientGUIDialogsQuick.GetYesNo = old_yesno
        ClientGUIDialogsMessage.ShowWarning = old_warning
    return {'initial': initial, 'steps': steps, 'names': NAMES}


def main():
    import hydrus_driver
    import record_api
    result = hydrus_driver.run_client(record_api.unpack_fixture('basic'), record)
    with open(OUT, 'w') as f:
        json.dump(result, f, indent=1, ensure_ascii=False)
        f.write('\n')
    print(f'wrote {OUT}')


if __name__ == '__main__':
    main()

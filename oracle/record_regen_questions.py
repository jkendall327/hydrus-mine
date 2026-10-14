#!/usr/bin/env python3
"""Record the question each thumbnail "manage > maintenance" job asks.

On a page of the `basic` fixture's files, with one, then three, selected, the
grid's own `GetMenu` is built and its "manage" > "maintenance" submenu read
(each job's label, in the menu's order); then each job's row is triggered
with `GetYesNo` captured: the message, the dialog's title and its buttons,
answered "no" so nothing is queued.

Usage: QT_QPA_PLATFORM=offscreen python oracle/record_regen_questions.py
       (writes fixtures/regen_questions.json)
"""
import json
import os
import sys
import time

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
OUT = os.path.join(HERE, 'fixtures', 'regen_questions.json')
MANIFEST = json.load(open(os.path.join(HERE, 'fixtures', 'legacy_db', 'basic.manifest.json')))


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
    from hydrus.client import ClientConstants as CC, ClientLocation
    from hydrus.client.gui import ClientGUIDialogsQuick
    from hydrus.client.media import ClientMediaFileFilter
    controller = session.controller
    gui = controller.gui
    hashes = [bytes.fromhex(f['hash']) for f in MANIFEST['files']]
    defaults = inspect.signature(ClientGUIDialogsQuick.GetYesNo).parameters

    def qt(f):
        return controller.CallBlockingToQt(gui, f)
    location = ClientLocation.LocationContext.STATICCreateSimple(CC.LOCAL_FILE_SERVICE_KEY)
    page = qt(lambda: gui._notebook.NewPageQuery(location, initial_hashes=hashes))
    panel = None
    for _ in range(600):
        panel = qt(lambda: page.GetMediaResultsPanel())
        if hasattr(panel, 'GetMenu') and len(panel._sorted_media) > 0:
            break
        time.sleep(0.05)
    media = qt(lambda: list(panel._sorted_media))
    asked = []
    old = ClientGUIDialogsQuick.GetYesNo

    def yesno(win, message, **kwargs):
        asked.append(dict(message=message, title=kwargs.get('title', defaults['title'].default),
            yes=kwargs.get('yes_label', defaults['yes_label'].default), no=kwargs.get('no_label', defaults['no_label'].default)))
        return QW.QDialog.DialogCode.Rejected
    ClientGUIDialogsQuick.GetYesNo = yesno
    out = []
    try:
        for count in (1, 3):
            picked = media[:count]

            def select():
                panel._Select(ClientMediaFileFilter.FileFilter(ClientMediaFileFilter.FILE_FILTER_NONE))
                for i, m in enumerate(picked):
                    panel._HitMedia(m, i > 0, False)

            def menu_rows():
                select()
                menu = panel.GetMenu()
                for action in menu.actions():
                    if action.menu() is not None and action.text() == 'manage':
                        for sub in action.menu().actions():
                            if sub.menu() is not None and sub.text() == 'maintenance':
                                return [a for a in sub.menu().actions() if not a.isSeparator()]
                return []
            labels = qt(lambda: [a.text() for a in menu_rows()])
            rows = []
            for label in labels:
                asked.clear()

                def trigger():
                    for a in menu_rows():
                        if a.text() == label:
                            a.trigger()
                            return
                qt(trigger)
                rows.append({'label': label, 'asked': list(asked)})
            out.append({'selected': count, 'hashes': [m.GetHash().hex() for m in picked], 'files': [m.GetHash().hex() for m in media], 'rows': rows})
    finally:
        ClientGUIDialogsQuick.GetYesNo = old
    return {'cases': out}


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

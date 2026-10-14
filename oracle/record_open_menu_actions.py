#!/usr/bin/env python3
"""Record the thumbnail menu's "open" submenu and what each row does.

On a page of the `basic` fixture's JPEGs two are selected (the first the
focus); the real grid's `GetMenu` is built with advanced mode off and on and
the "open" submenu recorded as a tree. Each row is then triggered with the
launches captured instead of run: the path the default file launch is sent,
the URL the web browser is sent, the path the file browser is shown, and the
`new_page_query` / `new_page_duplicates` pubs that open pages (their location,
files, predicates and name).

Usage: QT_QPA_PLATFORM=offscreen python oracle/record_open_menu_actions.py
       (writes fixtures/open_menu_actions.json)
"""
import json
import os
import sys
import time

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
OUT = os.path.join(HERE, 'fixtures', 'open_menu_actions.json')
MANIFEST = json.load(open(os.path.join(HERE, 'fixtures', 'legacy_db', 'basic.manifest.json')))
NAMES = {f['name']: f['hash'] for f in MANIFEST['files']}
FOCUS, OTHER = 'jpeg_03.jpg', 'jpeg_05.jpg'


def record(session):
    try:
        return _record(session)
    except BaseException:
        import traceback
        traceback.print_exc()
        sys.stderr.flush()
        os._exit(1)


def _record(session):
    from qtpy import QtWidgets as QW
    from hydrus.client import ClientConstants as CC, ClientLocation, ClientPaths
    from hydrus.client.executables import ClientExecutableActualCall as Call
    from hydrus.client.media import ClientMediaFileFilter
    controller = session.controller
    gui = controller.gui
    jpegs = [bytes.fromhex(NAMES[n]) for n in NAMES if n.endswith('.jpg')]

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

    def select():
        panel._Select(ClientMediaFileFilter.FileFilter(ClientMediaFileFilter.FILE_FILTER_NONE))
        picked = [media_by_hash[bytes.fromhex(NAMES[n])] for n in (FOCUS, OTHER)]
        for i, m in enumerate(picked):
            panel._HitMedia(m, i > 0, False)
        panel.SetFocusedMedia(picked[0])

    def open_menu():
        select()
        menu = panel.GetMenu()
        return [a for a in menu.actions() if a.menu() is not None and a.text() == 'open'][0].menu()

    def describe(value):
        if isinstance(value, (list, tuple, set, frozenset)):
            return [describe(v) for v in value]
        if isinstance(value, bytes):
            return value.hex()
        if hasattr(value, 'ToString') and hasattr(value, 'GetType'):
            return value.ToString()
        if isinstance(value, (str, int, float, bool)) or value is None:
            return value
        return str(value)

    log = []
    original = dict(file=Call.ExecutableLocalProcessDefaultLaunchFile._DoCall, url=Call.ExecutableLocalProcessDefaultLaunchURL._DoCall,
                    location=ClientPaths.OpenFileLocation, pub=controller.pub)

    def fake_file(self, input_parameters, for_user_test=False):
        log.append({'launch file': [describe(v) for v in input_parameters.values()]})
        return {}

    def fake_url(self, input_parameters, for_user_test=False):
        log.append({'launch url': [describe(v) for v in input_parameters.values()]})
        return {}

    def fake_location(path):
        log.append({'show in file browser': path})

    def pub(topic, *args, **kwargs):
        if topic in ('new_page_query', 'new_page_duplicates'):
            log.append({'pub': topic, 'args': describe(args),
                        'kwargs': {k: describe(v) for k, v in kwargs.items() if k in ('initial_hashes', 'initial_predicates', 'page_name')}})
        return original['pub'](topic, *args, **kwargs)
    # (a normal Linux install has `showinfm`, which advanced mode's "in file
    # browser" row needs; this environment may not)
    ClientPaths.CAN_OPEN_FILE_LOCATION = True
    Call.ExecutableLocalProcessDefaultLaunchFile._DoCall = fake_file
    Call.ExecutableLocalProcessDefaultLaunchURL._DoCall = fake_url
    ClientPaths.OpenFileLocation = fake_location
    controller.pub = pub
    out = []
    try:
        for advanced in (False, True):
            controller.new_options.SetBoolean('advanced_mode', advanced)
            menu_tree = qt(lambda: tree(open_menu()))
            entry = {'advanced': advanced, 'selection': [NAMES[FOCUS], NAMES[OTHER]], 'menu': menu_tree,
                     'file paths': {n: controller.client_files_manager.GetFilePath(bytes.fromhex(NAMES[n]), 0 + __import__('hydrus.core.HydrusConstants', fromlist=['x']).IMAGE_JPEG) for n in (FOCUS, OTHER)}, 'rows': []}
            leaves = []

            def collect(items, path):
                for e in items:
                    if isinstance(e, str):
                        leaves.append(path + [e])
                    else:
                        collect(e['entries'], path + [e['menu']])
            collect(menu_tree, [])
            for path in leaves:
                custom_dialogs = []
                if path[-1] == 'custom':
                    # (the "choose distance" dialog: its control as it opens, then
                    # accepted with a value, and cancelled)
                    from hydrus.client.gui import ClientGUITopLevelWindowsPanels as Windows
                    original_exec = Windows.DialogEdit.exec
                    answer = {'accept': True}

                    def fake_exec(self, *args, **kwargs):
                        box = self.findChildren(QW.QSpinBox)[0]
                        custom_dialogs.append({'title': self.windowTitle(), 'value': box.value(), 'minimum': box.minimum(),
                                               'maximum': box.maximum(), 'step': box.singleStep(), 'answered': 6 if answer['accept'] else None})
                        if answer['accept']:
                            box.setValue(6)
                            return QW.QDialog.DialogCode.Accepted
                        return QW.QDialog.DialogCode.Rejected
                    Windows.DialogEdit.exec = fake_exec
                    try:
                        for accept in (False, True):
                            answer['accept'] = accept
                            log.clear()
                            tabs_before = qt(lambda: gui._notebook.count())

                            def trigger_custom():
                                menu = open_menu()
                                for part in path:
                                    action = [a for a in menu.actions() if a.text() == part][0]
                                    if action.menu() is not None:
                                        menu = action.menu()
                                    else:
                                        action.trigger()
                            qt(trigger_custom)
                            time.sleep(0.7)
                            row = {'path': path, 'accepted': accept, 'dialogs': list(custom_dialogs), 'log': list(log),
                                   'tabs_added': qt(lambda: gui._notebook.count()) - tabs_before}
                            if row['tabs_added']:
                                row['page name'] = qt(lambda: gui._notebook.widget(gui._notebook.count() - 1).GetName())
                                qt(lambda: gui._notebook.setCurrentIndex(0))
                            custom_dialogs.clear()
                            entry['rows'].append(row)
                    finally:
                        Windows.DialogEdit.exec = original_exec
                    continue
                log.clear()

                def trigger():
                    menu = open_menu()
                    for part in path:
                        action = [a for a in menu.actions() if a.text() == part][0]
                        if action.menu() is not None:
                            menu = action.menu()
                        else:
                            action.trigger()
                time_before = qt(lambda: gui._notebook.count())
                qt(trigger)
                time.sleep(0.7)
                row = {'path': path, 'log': list(log), 'tabs_added': qt(lambda: gui._notebook.count()) - time_before}
                if row['tabs_added']:
                    def newest():
                        return gui._notebook.widget(gui._notebook.count() - 1)
                    row['page name'] = qt(lambda: newest().GetName())
                    for _ in range(200):
                        p2 = qt(lambda: newest().GetMediaResultsPanel())
                        if hasattr(p2, 'GetMenu') and len(p2._sorted_media) > 0:
                            break
                        time.sleep(0.05)
                    row['page hashes'] = sorted(qt(lambda: [m.GetHash().hex() for m in p2._sorted_media])) if hasattr(p2, '_sorted_media') else None
                    qt(lambda: gui._notebook.setCurrentIndex(0))
                entry['rows'].append(row)
            out.append(entry)
    finally:
        Call.ExecutableLocalProcessDefaultLaunchFile._DoCall = original['file']
        Call.ExecutableLocalProcessDefaultLaunchURL._DoCall = original['url']
        ClientPaths.OpenFileLocation = original['location']
        controller.pub = original['pub']
        controller.new_options.SetBoolean('advanced_mode', False)
    return {'names': NAMES, 'cases': out}


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

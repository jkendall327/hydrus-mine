#!/usr/bin/env python3
"""Help > debug > report modes > shortcut report mode and gui report mode, on
the real client.

The real Help QAction turns each mode on; key presses are then delivered to
the real main window (the reference's `ShortcutsHandler` event filter catches
them) and the manage tags panel is opened on real media, and every
`HydrusData.ShowText` they make is read back, in order. Each scenario is run
with shortcut report mode alone, gui report mode alone, and neither.

Also recorded: the reference's own words (`ApplicationCommand.ToString`) for
the simple commands hydrus-rs runs, which the reports quote.

Usage: QT_QPA_PLATFORM=offscreen ~/pyenv/bin/python oracle/record_debug_report_modes_gui.py
       (writes fixtures/debug_report_modes_gui.json)
"""
import json
import sys
import tempfile
import threading
from pathlib import Path

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))

API_PORT = 45898

# (name, Qt key, modifiers, text of the key) -- the key itself, as one press
# (QTest.keyClick also sends the modifier keys' own presses, which a shortcut
# set has no use for)
SCENARIOS = [
    ('j', 'Key_J', [], 'j'),
    ('f5', 'Key_F5', [], ''),
    ('shift+j', 'Key_J', ['ShiftModifier'], 'J'),
    ('ctrl+j', 'Key_J', ['ControlModifier'], 'j'),
    ('alt+j', 'Key_J', ['AltModifier'], 'j'),
    ('ctrl+f5', 'Key_F5', ['ControlModifier'], ''),
    ('numpad 5', 'Key_5', ['KeypadModifier'], '5'),
    ('return', 'Key_Return', [], '\r'),
    ('space', 'Key_Space', [], ' '),
    ('escape', 'Key_Escape', [], ''),
]
COMMANDS = [78, 7, 56, 6, 99, 100, 101, 102, 92]


def record(session):
    from qtpy import QtCore as C, QtGui as G, QtTest as T, QtWidgets as W
    from hydrus.core import HydrusData
    from hydrus.core import HydrusGlobals as HG
    from hydrus.client import ClientApplicationCommand as CAC
    from hydrus.client.media import ClientMediaList
    controller = session.controller
    gui = controller.gui
    import re
    seen = []
    original = HydrusData.ShowText

    def tidy(text):
        # (object addresses differ run to run)
        text = re.sub(r'<hydrus\.client\.gui\.ClientGUI\.FrameGUI\(0x[0-9a-f]+\) at 0x[0-9a-f]+>', '<main window>', text)
        text = re.sub(r'<hydrus\.client\.ClientServices\.\w+ object at 0x[0-9a-f]+>', '<service>', text)
        return re.sub(r'(\(| at )0x[0-9a-f]+', r'\g<1>0x_', text)

    HydrusData.ShowText = lambda text, *a, **k: seen.append(tidy(str(text)))

    def find(menu, labels):
        for action in menu.actions():
            if action.text() == labels[0]:
                return action if len(labels) == 1 else find(action.menu(), labels[1:])
        raise ValueError(labels)

    status, _, body = session.api.request('GET', '/get_files/search_files', query={'tags': json.dumps(['system:everything']), 'return_file_ids': 'true'})
    file_ids = json.loads(body)['file_ids'][:2]
    out = {'menu_path': ['help', 'debug', 'report modes']}

    def go():
        holder = []

        def switch(label):
            if not holder:
                holder.append(gui._InitialiseMenuInfoHelp())
            find(holder[0][0], ['debug', 'report modes', label]).trigger()

        gui.show()
        gui.activateWindow()
        gui.setFocus()
        W.QApplication.processEvents()
        out['commands'] = {str(i): CAC.ApplicationCommand.STATICCreateSimpleCommand(i).ToString() for i in COMMANDS}

        def press(key, modifiers, text):
            mods = C.Qt.KeyboardModifier.NoModifier
            for m in modifiers:
                mods |= getattr(C.Qt.KeyboardModifier, m)
            event = G.QKeyEvent(C.QEvent.Type.KeyPress, getattr(C.Qt.Key, key), mods, text)
            seen.clear()
            W.QApplication.sendEvent(gui, event)
            W.QApplication.processEvents()
            return list(seen)

        out['keys'] = []
        for name, key, modifiers, text in SCENARIOS:
            row = {'name': name}
            for state, label in (('off', None), ('shortcut', 'shortcut report mode'), ('gui', 'gui report mode')):
                if label:
                    switch(label)
                row[state] = press(key, modifiers, text)
                if label:
                    switch(label)
            out['keys'].append(row)
        assert not HG.shortcut_report_mode and not HG.gui_report_mode

        # the manage tags panel opened on real media
        from hydrus.client import ClientConstants as CC
        from hydrus.client import ClientLocation
        location_context = ClientLocation.LocationContext.STATICCreateSimple(CC.LOCAL_FILE_SERVICE_KEY)
        media_list = ClientMediaList.MediaList(location_context, controller.Read('media_results_from_ids', file_ids))
        media = ClientMediaList.FlattenMedia(media_list.GetSortedMedia())
        from hydrus.client.gui.metadata import ClientGUIManageTags
        from hydrus.client.metadata import ClientContentUpdates
        from hydrus.client.search import ClientSearchFileSearchContext  # noqa: F401
        out['manage_tags'] = {}
        for state, label in (('off', None), ('gui', 'gui report mode')):
            if label:
                switch(label)
            seen.clear()
            panel = ClientGUIManageTags.ManageTagsPanel(gui, location_context, CC.TAG_PRESENTATION_SEARCH_PAGE_MANAGE_TAGS, media)
            W.QApplication.processEvents()
            out['manage_tags'][state] = list(seen)
            panel.deleteLater()
            if label:
                switch(label)
        services = controller.services_manager.GetServices((HC_LOCAL_TAG, HC_TAG_REPOSITORY))
        out['tag_services'] = [{'name': s.GetName(), 'key': s.GetServiceKey().hex()} for s in services]
        return out

    from hydrus.core import HydrusConstants as HC
    global HC_LOCAL_TAG, HC_TAG_REPOSITORY
    HC_LOCAL_TAG, HC_TAG_REPOSITORY = HC.LOCAL_TAG, HC.TAG_REPOSITORY
    try:
        return controller.CallBlockingToQt(gui, go)
    finally:
        HydrusData.ShowText = original
        HG.shortcut_report_mode = HG.gui_report_mode = False


def main():
    import hydrus_driver
    import record_api
    if len(sys.argv) > 1 and sys.argv[1] == '--child':
        destination = sys.argv[2]  # (run_client rewrites sys.argv)
        database = record_api.unpack_fixture('basic')
        Path(destination).write_text(json.dumps(hydrus_driver.run_client(database, record, port=API_PORT)))
        return
    with tempfile.TemporaryDirectory() as directory:
        output = Path(directory) / 'result.json'
        hydrus_driver.run_in_subprocess(str(Path(__file__).resolve()), '--child', str(output))
        result = json.loads(output.read_text())
    (HERE / 'fixtures/debug_report_modes_gui.json').write_text(json.dumps(result, indent=2) + '\n')
    print('wrote debug_report_modes_gui.json')


if __name__ == '__main__':
    main()

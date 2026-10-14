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
    import threading
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
    all_file_ids = json.loads(body)['file_ids']
    file_ids = all_file_ids[:2]
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
        # blurhash mode: the thumbnail cache's own bitmap for each file, as
        # the grid asks for it, with the mode off and on
        import hashlib
        out['blurhash'] = []
        cache = controller.thumbnails_cache
        for result in controller.Read('media_results_from_ids', all_file_ids):
            row = {'hash': result.GetHash().hex(), 'mime': result.GetMime(), 'blurhash': result.GetFileInfoManager().blurhash, 'resolution': list(result.GetResolution() or [])}

            def bitmap():
                image = cache._GetThumbnailHydrusBitmap(result).GetNumpyImage()
                height, width = image.shape[0], image.shape[1]
                samples = [[int(v) for v in image[min(height - 1, (y * height) // 4 + height // 8), min(width - 1, (x * width) // 4 + width // 8)]] for y in range(4) for x in range(4)]
                return {'shape': list(image.shape), 'sha256': hashlib.sha256(image.tobytes()).hexdigest(), 'samples': samples}
            row['off'] = bitmap()
            switch('blurhash mode')
            assert HG.blurhash_mode
            row['on'] = bitmap()
            switch('blurhash mode')
            out['blurhash'].append(row)
        services = controller.services_manager.GetServices((HC_LOCAL_TAG, HC_TAG_REPOSITORY))
        out['tag_services'] = [{'name': s.GetName(), 'key': s.GetServiceKey().hex()} for s in services]
        return out

    from hydrus.core import HydrusConstants as HC
    global HC_LOCAL_TAG, HC_TAG_REPOSITORY
    HC_LOCAL_TAG, HC_TAG_REPOSITORY = HC.LOCAL_TAG, HC.TAG_REPOSITORY
    def autocomplete():
        """Help > debug > gui actions > autocomplete delay mode: how long the
        reference's autocomplete reads take with it on and off."""
        import time
        from hydrus.client import ClientConstants as CC
        from hydrus.client.metadata import ClientTags
        from hydrus.client.search import ClientSearchFileSearchContext, ClientSearchTagContext
        from hydrus.client import ClientThreading

        def toggle():
            def flip():
                menu, _ = gui._InitialiseMenuInfoHelp()
                find(menu, ['debug', 'gui actions', 'autocomplete delay mode']).trigger()
                menu.deleteLater()
            controller.CallBlockingToQt(gui, flip)

        context = ClientSearchFileSearchContext.FileSearchContext(tag_context=ClientSearchTagContext.TagContext())

        def timed(read, *args, **kwargs):
            start = time.monotonic()
            result = read(*args, **kwargs)
            return {'seconds': round(time.monotonic() - start, 1), 'results': len(result)}

        def reads():
            return {
                'wildcard search': timed(controller.Read, 'autocomplete_predicates', ClientTags.TAG_DISPLAY_DISPLAY_ACTUAL, context, search_text='b*', job_status=ClientThreading.JobStatus()),
                'exact match search': timed(controller.Read, 'autocomplete_predicates', ClientTags.TAG_DISPLAY_DISPLAY_ACTUAL, context, search_text='blue', exact_match=True, job_status=ClientThreading.JobStatus()),
                'media predicates': timed(controller.Read, 'media_predicates', ClientSearchTagContext.TagContext(), {}, job_status=ClientThreading.JobStatus()),
            }

        result = {'menu_path': ['help', 'debug', 'gui actions', 'autocomplete delay mode'], 'off': reads()}
        toggle()
        assert HG.autocomplete_delay_mode
        result['on'] = reads()
        # cancelled half a second in, a delayed read gives up at once
        job = ClientThreading.JobStatus(cancellable=True)
        threading.Timer(0.5, job.Cancel).start()
        result['cancelled'] = timed(controller.Read, 'media_predicates', ClientSearchTagContext.TagContext(), {}, job_status=job)
        toggle()
        assert not HG.autocomplete_delay_mode
        return result

    try:
        out = controller.CallBlockingToQt(gui, go)
        out['autocomplete'] = autocomplete()
        return out
    finally:
        HydrusData.ShowText = original
        HG.shortcut_report_mode = HG.gui_report_mode = HG.autocomplete_delay_mode = HG.blurhash_mode = False


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

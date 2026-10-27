#!/usr/bin/env python3
"""Record the media viewer's clicks on its notes hover and on the top-right
hover's inbox icon.

A real CanvasMediaListBrowser over two of the basic fixture's JPEGs, both in
the inbox: the first with notes "alpha" and "beta", the second with "alpha".
On the notes hover (`CanvasHoverFrameRightNotes`, its `NotePanel`s): the
panels and their tooltip, then real mouse presses (`QTest.mousePress`) on a
note's name and text, left (edit the note: Manage Notes is recorded, not
opened), right (fold or unfold its text) and middle (copy: the clipboard
message recorded), and which notes show their text after each, also after
moving to the other file and back. On the top-right hover
(`CanvasHoverFrameTopRight`): its inbox and trash icons and location text,
then a right and a left press on the inbox icon, with the commands sent and
the file's inbox state after.

Usage: QT_QPA_PLATFORM=offscreen ~/pyenv/bin/python oracle/record_viewer_hover_clicks.py
(writes fixtures/viewer_hover_clicks.json)
"""

import json
import os
import sys
import tempfile
import time

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)

OUT = os.path.join(HERE, 'fixtures', 'viewer_hover_clicks.json')

NOW = 1900000000

FILES = [('jpeg_04.jpg', ['alpha', 'beta']), ('jpeg_05.jpg', ['alpha'])]


def settle(seconds=0.5):
    from qtpy import QtWidgets as QW
    deadline = time.time() + seconds
    while time.time() < deadline:
        QW.QApplication.processEvents()
        time.sleep(0.02)


def press(widget, button):
    from qtpy import QtCore as QC
    from qtpy import QtTest
    qt_button = {'left': QC.Qt.MouseButton.LeftButton, 'right': QC.Qt.MouseButton.RightButton,
                 'middle': QC.Qt.MouseButton.MiddleButton}[button]
    QtTest.QTest.mousePress(widget, qt_button, QC.Qt.KeyboardModifier.NoModifier, widget.rect().center())
    QtTest.QTest.mouseRelease(widget, qt_button, QC.Qt.KeyboardModifier.NoModifier, widget.rect().center())


def record(session):
    controller = session.controller
    gui = controller.gui
    from hydrus.core import HydrusTime
    HydrusTime.GetNow = lambda: NOW
    HydrusTime.GetNowMS = lambda: NOW * 1000
    HydrusTime.GetNowFloat = lambda: float(NOW)

    from hydrus.core import HydrusConstants as HC
    from hydrus.client import ClientApplicationCommand as CAC, ClientConstants as CC, ClientLocation
    from hydrus.client.metadata import ClientContentUpdates
    from hydrus.client.gui.canvas import ClientGUICanvas, ClientGUICanvasFrame
    from hydrus.client.gui.media import ClientGUIMediaModalActions

    with open(os.path.join(HERE, 'fixtures', 'legacy_db', 'basic.manifest.json')) as stream:
        manifest = json.load(stream)
    by_name = {f['name']: bytes.fromhex(f['hash']) for f in manifest['files']}
    hashes = [by_name[name] for (name, notes) in FILES]

    def write(service_key, content_type, action, row):
        cu = ClientContentUpdates.ContentUpdate(content_type, action, row)
        controller.WriteSynchronous('content_updates', ClientContentUpdates.ContentUpdatePackage.STATICCreateFromContentUpdate(service_key, cu))

    write(CC.HYDRUS_LOCAL_FILE_STORAGE_SERVICE_KEY, HC.CONTENT_TYPE_FILES, HC.CONTENT_UPDATE_INBOX, hashes)
    for (name, notes) in FILES:
        for note in notes:
            write(CC.LOCAL_NOTES_SERVICE_KEY, HC.CONTENT_TYPE_NOTES, HC.CONTENT_UPDATE_SET, (by_name[name], note, 'the ' + note + ' note of ' + name))

    def drive():
        simple_names = {getattr(CAC, n): n for n in dir(CAC) if n.startswith('SIMPLE_') and isinstance(getattr(CAC, n), int)}
        events = []
        ClientGUIMediaModalActions.EditFileNotes = lambda win, media, name_to_start_on=None: events.append({'edit notes': media.GetHash().hex(), 'start on': name_to_start_on})
        original_pub = controller.pub

        def pub(topic, *args, **kwargs):
            if topic == 'clipboard':
                events.append({'clipboard': list(args)})
                return
            return original_pub(topic, *args, **kwargs)
        controller.pub = pub
        results = sorted(controller.Read('media_results', hashes), key=lambda r: r.GetFileInfoManager().size)
        location = ClientLocation.LocationContext.STATICCreateSimple(CC.LOCAL_FILE_SERVICE_KEY)
        frame = ClientGUICanvasFrame.CanvasFrame(gui)
        canvas = ClientGUICanvas.CanvasMediaListBrowser(frame, os.urandom(32), location, results, results[0].GetHash())
        frame.SetCanvas(canvas)
        frame.showNormal()
        frame.resize(1000, 750)
        settle(1.0)
        canvas._top_right_hover.sendApplicationCommand.connect(lambda c: events.append({'command': simple_names.get(c.GetSimpleAction())}))
        notes_hover = canvas._right_notes_hover
        top_right = canvas._top_right_hover

        def show(name):
            media = list(canvas._media_list.GetMediaByHashes({by_name[name]}))[0]
            canvas.SetMedia(media)
            settle()

        def notes_state():
            return [{'name': name, 'text_shown': not panel._note_text.isHidden(), 'text': panel._note_text.text(), 'tooltip': panel.toolTip()}
                    for (name, panel) in notes_hover._names_to_note_panels.items()]

        def top_right_state():
            media = canvas._current_media
            return {'inbox_icon': not top_right._inbox_icon.isHidden(), 'trash_icon': not top_right._trash_icon.isHidden(),
                    'icons': not top_right._icon_panel.isHidden(), 'locations': top_right._location_strings.text(),
                    'inbox': media.HasInbox()}

        out = {'files': [{'name': n, 'hash': by_name[n].hex()} for (n, _) in FILES], 'order': [r.GetHash().hex() for r in results]}
        steps = []
        first = FILES[0][0]
        show(first)
        steps.append({'do': 'open', 'file': first, 'notes': notes_state(), 'top_right': top_right_state()})
        for (note, part, button) in [('alpha', 'name', 'left'), ('beta', 'text', 'left'), ('beta', 'text', 'right'),
                                     ('beta', 'name', 'right'), ('beta', 'name', 'right'), ('alpha', 'text', 'middle'),
                                     ('alpha', 'name', 'middle')]:
            panel = notes_hover._names_to_note_panels[note]
            widget = panel._note_name if part == 'name' else panel._note_text
            events.clear()
            if widget.isHidden():
                steps.append({'do': 'skip', 'note': note, 'part': part, 'button': button})
                continue
            press(widget, button)
            settle()
            steps.append({'do': 'press', 'note': note, 'part': part, 'button': button, 'did': list(events), 'notes': notes_state()})
        # the folded note stays folded on another file, and coming back
        for name in (FILES[1][0], first):
            show(name)
            steps.append({'do': 'show', 'file': name, 'notes': notes_state(), 'top_right': top_right_state()})
        # the inbox icon: a right press, then a left one
        for button in ('right', 'left'):
            events.clear()
            press(top_right._inbox_icon, button)
            settle(1.0)
            steps.append({'do': 'inbox icon', 'button': button, 'did': list(events), 'top_right': top_right_state()})
        out['steps'] = steps
        frame.hide()
        return out
    return controller.CallBlockingToQt(gui, drive)


def main():
    import hydrus_driver
    import record_api
    if len(sys.argv) > 1 and sys.argv[1] == '--child':
        output = sys.argv[2]
        result = hydrus_driver.run_client(record_api.unpack_fixture('basic'), record)
        with open(output, 'w') as stream:
            json.dump(result, stream)
        return
    with tempfile.TemporaryDirectory() as work:
        out = os.path.join(work, 'clicks.json')
        hydrus_driver.run_in_subprocess(os.path.abspath(__file__), '--child', out)
        with open(out) as stream:
            result = json.load(stream)
    with open(OUT, 'w') as stream:
        json.dump(result, stream, indent=1)
        stream.write('\n')
    print('wrote', OUT)


if __name__ == '__main__':
    main()

#!/usr/bin/env python3
"""Record the media viewer's top hover frame (`CanvasHoverFrameTop` and its
subclasses) on the three canvases that make one, and what its buttons do.

- the browser (`CanvasMediaListBrowser`, `CanvasHoverFrameTopNavigableList`)
  on the basic fixture: a JPEG in the inbox, one archived, one with a note,
  an audio file, a video with sound, and one in the trash;
- the archive/delete filter (`CanvasMediaListFilterArchiveDelete`,
  `CanvasHoverFrameTopArchiveDeleteFilter`) on the basic fixture's JPEGs;
- the duplicate filter (`CanvasFilterDuplicates`,
  `CanvasHoverFrameTopDuplicatesFilter`) on the auto-resolution fixture's
  potential pairs.

For each file shown: every widget of the hover's three rows, left to right
(its class, icon by `global_icons()` name, tooltip, whether it is shown, and
the index and zoom labels' text). Then each shown button is clicked with a
real mouse press and release at its centre (`QTest.mouseClick`), with the left
button and, for those that take one, the right, recording what it did: the
application commands the hover sent (and the canvas then ran), the canvas
messages it published (close, fullscreen, delete and undelete are recorded,
not delivered), the menu it opened (its items, checks and submenus; not
shown), dialogs and programs it would open (recorded, not opened), and the
index, zoom and archive button afterwards.

Time is held still. Usage:
QT_QPA_PLATFORM=offscreen ~/pyenv/bin/python oracle/record_viewer_top_hover.py
(writes fixtures/viewer_top_hover.json)
"""

import json
import os
import sys
import tempfile
import time

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)

OUT = os.path.join(HERE, 'fixtures', 'viewer_top_hover.json')

NOW = 1900000000

# the browser's files, in order, and what is done to each before it opens
BROWSER = [
    ('jpeg_00.jpg', 'inbox'),
    ('jpeg_01.jpg', 'archived'),
    ('jpeg_02.jpg', 'note'),
    ('audio.flac', 'inbox'),
    ('jpeg_03.jpg', 'trashed'),
]

# the place in the sorted list the random button's draw lands on
RANDOM_DRAW = 1

ARCHIVE_DELETE = ['jpeg_04.jpg', 'jpeg_05.jpg', 'jpeg_06.jpg', 'jpeg_07.jpg']

# canvas messages the buttons publish that are recorded and not delivered
HELD_TOPICS = ('canvas_close', 'canvas_fullscreen_switch', 'canvas_delete', 'canvas_undelete')


def hold_time():
    from hydrus.core import HydrusTime
    HydrusTime.GetNow = lambda: NOW
    HydrusTime.GetNowMS = lambda: NOW * 1000
    HydrusTime.GetNowFloat = lambda: float(NOW)


class Watcher(object):
    """What the buttons do: commands, messages, menus, dialogs, launches."""

    def __init__(self, controller):
        from hydrus.client import ClientApplicationCommand as CAC
        from hydrus.client.gui import ClientGUICore as CGC
        from hydrus.client.gui import ClientGUIDragDrop
        from hydrus.client.gui.executables import ClientGUIExecutableActions
        from hydrus.client.gui.media import ClientGUIMediaModalActions
        from qtpy import QtCore as QC
        self.controller = controller
        self.CAC = CAC
        self.simple_names = {getattr(CAC, n): n for n in dir(CAC) if n.startswith('SIMPLE_') and isinstance(getattr(CAC, n), int)}
        self.events = []
        original_pub = controller.pub

        def pub(topic, *args, **kwargs):
            if topic.startswith('canvas_'):
                self.events.append({'pub': topic})
                if topic in HELD_TOPICS:
                    return
            return original_pub(topic, *args, **kwargs)
        controller.pub = pub

        core = CGC.core()

        def popup(widget, menu):
            self.events.append({'menu': self.menu(menu)})
        core.PopupMenu = popup

        def metadata(win, media_result):
            self.events.append({'dialog': 'embedded metadata', 'file': media_result.GetHash().hex()})
        ClientGUIMediaModalActions.ShowFileEmbeddedMetadata = metadata

        def drag(drag_object, page_key, media, alt_down):
            self.events.append({'drag': [m.GetHash().hex() for m in media]})
            return QC.Qt.DropAction.IgnoreAction
        ClientGUIDragDrop.DoFileExportDragDrop = drag

        def launch(win, media_result):
            self.events.append({'open externally': media_result.GetHash().hex()})
            return False
        ClientGUIExecutableActions.OpenExternallySingleFileDefault = launch

    def command(self, command):
        entry = {'command': self.simple_names.get(command.GetSimpleAction(), command.GetSimpleAction())}
        data = command.GetSimpleData()
        if data is not None:
            entry['data'] = data
        self.events.append(entry)

    def menu(self, menu):
        items = []
        for action in menu.actions():
            if action.isSeparator():
                items.append({'separator': True})
                continue
            item = {'text': action.text()}
            if action.menu() is not None:
                item['submenu'] = self.menu(action.menu())
            if action.isCheckable():
                item['checked'] = action.isChecked()
            if not action.isEnabled():
                item['enabled'] = False
            items.append(item)
        return items

    def take(self):
        out = list(self.events)
        self.events.clear()
        return out


def icon_name(icon):
    from hydrus.client import ClientConstants as CC
    icons = CC.global_icons()
    for (name, value) in vars(icons).items():
        if value is icon:
            return name
    return None


def describe(hover):
    """The hover's widgets, left to right."""
    from qtpy import QtWidgets as QW
    out = []
    for (group, box) in (('left', hover._top_left_hbox), ('center', hover._top_center_hbox), ('right', hover._top_right_hbox)):
        for i in range(box.count()):
            widget = box.itemAt(i).widget()
            if widget is None:
                continue
            entry = {'group': group, 'class': type(widget).__name__, 'shown': not widget.isHidden()}
            if hasattr(widget, 'last_icon_set'):
                entry['icon'] = icon_name(widget.last_icon_set)
            if widget is getattr(hover, '_index_text', None):
                entry['label'] = 'index'
            if widget is getattr(hover, '_zoom_text', None):
                entry['label'] = 'zoom'
            if isinstance(widget, QW.QLabel):
                entry['text'] = widget.text()
            if type(widget).__name__ == 'VolumeControl':
                entry['icon'] = icon_name(widget._global_mute.last_icon_set)
                entry['tooltip'] = widget._global_mute.toolTip()
                popup = widget._popup_window
                entry['popup'] = {'mute_icon': icon_name(popup._specific_mute.last_icon_set), 'mute_tooltip': popup._specific_mute.toolTip()}
            else:
                entry['tooltip'] = widget.toolTip()
            entry['_widget'] = widget
            out.append(entry)
    return out


def public(widgets):
    return [{k: v for (k, v) in w.items() if not k.startswith('_')} for w in widgets]


def state(canvas, hover):
    media = canvas._current_media
    out = {
        'index': hover._index_text.text(),
        'zoom': hover._zoom_text.text(),
        'shown': None if media is None else media.GetHash().hex(),
    }
    if media is not None:
        out['inbox'] = media.HasInbox()
        out['archive_icon'] = icon_name(hover._archive_button.last_icon_set)
        out['archive_tooltip'] = hover._archive_button.toolTip()
    return out


def media_of(canvas, h):
    return list(canvas._media_list.GetMediaByHashes({h}))[0]


def settle(seconds=0.3):
    from qtpy import QtWidgets as QW
    deadline = time.time() + seconds
    while time.time() < deadline:
        QW.QApplication.processEvents()
        time.sleep(0.02)


def click(widget, button):
    from qtpy import QtCore as QC
    from qtpy import QtTest
    qt_button = {'left': QC.Qt.MouseButton.LeftButton, 'right': QC.Qt.MouseButton.RightButton}[button]
    QtTest.QTest.mouseClick(widget, qt_button, QC.Qt.KeyboardModifier.NoModifier, widget.rect().center())


def press_buttons(canvas, hover, watcher, skip=()):
    """Click each shown button of the hover, left and right."""
    from hydrus.client import ClientGlobals as CG
    presses = []
    for entry in describe(hover):
        widget = entry['_widget']
        if entry['class'] in ('BetterStaticText',) or not entry['shown']:
            continue
        target = widget._global_mute if entry['class'] == 'VolumeControl' else widget
        for button in ('left', 'right'):
            if (entry.get('icon'), button) in skip:
                continue
            before = state(canvas, hover)
            watcher.take()
            mute_before = CG.client_controller.new_options.GetBoolean('global_audio_mute')
            click(target, button)
            settle(1.0)
            after = state(canvas, hover)
            press = {'icon': entry.get('icon'), 'button': button, 'did': watcher.take(), 'before': before, 'after': after}
            mute_after = CG.client_controller.new_options.GetBoolean('global_audio_mute')
            if mute_after != mute_before:
                press['global_mute'] = mute_after
                if entry['class'] == 'VolumeControl':
                    press['icon_after'] = icon_name(widget._global_mute.last_icon_set)
            presses.append(press)
    return presses


def browser(session):
    controller = session.controller
    gui = controller.gui
    hold_time()
    # (the random button's draw, scripted)
    import random
    random.randrange = lambda n: RANDOM_DRAW

    from hydrus.core import HydrusConstants as HC
    from hydrus.client import ClientConstants as CC, ClientLocation
    from hydrus.client.metadata import ClientContentUpdates
    from hydrus.client.gui.canvas import ClientGUICanvas, ClientGUICanvasFrame
    from hydrus.client.gui.canvas import ClientGUIMPV

    with open(os.path.join(HERE, 'fixtures', 'legacy_db', 'basic.manifest.json')) as stream:
        manifest = json.load(stream)
    by_name = {f['name']: bytes.fromhex(f['hash']) for f in manifest['files']}

    def write(service_key, content_type, action, row):
        cu = ClientContentUpdates.ContentUpdate(content_type, action, row)
        controller.WriteSynchronous('content_updates', ClientContentUpdates.ContentUpdatePackage.STATICCreateFromContentUpdate(service_key, cu))

    trashed = controller.Read('trash_hashes')
    if trashed:
        write(CC.HYDRUS_LOCAL_FILE_STORAGE_SERVICE_KEY, HC.CONTENT_TYPE_FILES, HC.CONTENT_UPDATE_DELETE, trashed)
    for (name, how) in BROWSER:
        h = by_name[name]
        write(CC.HYDRUS_LOCAL_FILE_STORAGE_SERVICE_KEY, HC.CONTENT_TYPE_FILES, HC.CONTENT_UPDATE_INBOX, [h])
        if how in ('archived',):
            write(CC.HYDRUS_LOCAL_FILE_STORAGE_SERVICE_KEY, HC.CONTENT_TYPE_FILES, HC.CONTENT_UPDATE_ARCHIVE, [h])
        if how == 'note':
            write(CC.LOCAL_NOTES_SERVICE_KEY, HC.CONTENT_TYPE_NOTES, HC.CONTENT_UPDATE_SET, (h, 'note', 'a note on the file'))
        if how == 'trashed':
            write(CC.LOCAL_FILE_SERVICE_KEY, HC.CONTENT_TYPE_FILES, HC.CONTENT_UPDATE_DELETE, [h])

    hashes = [by_name[name] for (name, how) in BROWSER]

    def drive():
        watcher = Watcher(controller)
        results = controller.Read('media_results', hashes)
        location = ClientLocation.LocationContext.STATICCreateSimple(CC.COMBINED_LOCAL_FILE_DOMAINS_SERVICE_KEY)
        frame = ClientGUICanvasFrame.CanvasFrame(gui)
        canvas = ClientGUICanvas.CanvasMediaListBrowser(frame, os.urandom(32), location, results, hashes[0])
        frame.SetCanvas(canvas)
        frame.showNormal()
        frame.resize(1000, 750)
        settle(1.0)
        hover = canvas._top_hover
        hover.sendApplicationCommand.connect(watcher.command)
        out = {'mpv_available': ClientGUIMPV.MPV_IS_AVAILABLE, 'random_draw': RANDOM_DRAW, 'files': []}
        out['order'] = [m.GetHash().hex() for m in canvas._media_list.GetSortedMedia()]
        for (name, how) in BROWSER:
            canvas.SetMedia(media_of(canvas, by_name[name]))
            settle(0.5)
            out['files'].append({'name': name, 'hash': by_name[name].hex(), 'state': how, 'widgets': public(describe(hover)), 'now': state(canvas, hover)})
        # the buttons pressed, on the first file
        canvas.SetMedia(media_of(canvas, hashes[0]))
        settle(0.5)
        out['presses'] = press_buttons(canvas, hover, watcher)
        # the archive button on an archived file, and on the trashed file the
        # delete-completely and undelete buttons
        for name in ('jpeg_01.jpg', 'jpeg_03.jpg'):
            canvas.SetMedia(media_of(canvas, by_name[name]))
            settle(0.5)
            for entry in describe(hover):
                if entry['group'] == 'center' and entry['shown']:
                    watcher.take()
                    before = state(canvas, hover)
                    click(entry['_widget'], 'left')
                    settle(1.0)
                    out['presses'].append({'file': name, 'icon': entry.get('icon'), 'button': 'left', 'did': watcher.take(), 'before': before, 'after': state(canvas, hover)})
        frame.hide()
        return out
    return controller.CallBlockingToQt(gui, drive)


def archive_delete(session):
    controller = session.controller
    gui = controller.gui
    hold_time()

    from hydrus.core import HydrusConstants as HC
    from hydrus.client import ClientConstants as CC, ClientLocation
    from hydrus.client.metadata import ClientContentUpdates
    from hydrus.client.gui.canvas import ClientGUICanvas, ClientGUICanvasFrame

    with open(os.path.join(HERE, 'fixtures', 'legacy_db', 'basic.manifest.json')) as stream:
        manifest = json.load(stream)
    by_name = {f['name']: bytes.fromhex(f['hash']) for f in manifest['files']}
    hashes = [by_name[n] for n in ARCHIVE_DELETE]
    cu = ClientContentUpdates.ContentUpdate(HC.CONTENT_TYPE_FILES, HC.CONTENT_UPDATE_INBOX, hashes)
    controller.WriteSynchronous('content_updates', ClientContentUpdates.ContentUpdatePackage.STATICCreateFromContentUpdate(CC.HYDRUS_LOCAL_FILE_STORAGE_SERVICE_KEY, cu))

    def drive():
        watcher = Watcher(controller)
        results = controller.Read('media_results', hashes)
        frame = ClientGUICanvasFrame.CanvasFrame(gui)
        canvas = ClientGUICanvas.CanvasMediaListFilterArchiveDelete(frame, os.urandom(32), ClientLocation.LocationContext.STATICCreateSimple(CC.LOCAL_FILE_SERVICE_KEY), results)
        frame.SetCanvas(canvas)
        frame.showNormal()
        frame.resize(1000, 750)
        settle(0.5)
        canvas.SetMedia(canvas._media_list.GetFirst())
        settle(1.0)
        hover = canvas._top_hover
        hover.sendApplicationCommand.connect(watcher.command)
        out = {'order': [m.GetHash().hex() for m in canvas._media_list.GetSortedMedia()]}
        out['widgets'] = public(describe(hover))
        out['now'] = state(canvas, hover)
        # (the keep, back and skip buttons move through the files; the
        # decisions are the filter's: record_archive_delete_filter.py)
        out['presses'] = press_buttons(canvas, hover, watcher)
        out['after'] = {'kept': len(canvas._kept), 'deleted': len(canvas._deleted), 'skipped': len(canvas._skipped)}
        canvas._kept = set()
        canvas._deleted = set()
        canvas.ClearMedia()
        frame.hide()
        frame.deleteLater()
        return out
    return controller.CallBlockingToQt(gui, drive)


def duplicates(session):
    controller = session.controller
    gui = controller.gui
    hold_time()

    manager = controller.duplicates_auto_resolution_manager
    manager._AbleToWorkIdleNormal = lambda: False
    manager._AbleToWorkActiveNormal = lambda: False

    from hydrus.client import ClientGlobals as CG
    from hydrus.client.duplicates import ClientDuplicates
    from hydrus.client.duplicates import ClientPotentialDuplicatesPairFactory
    from hydrus.client.duplicates import ClientPotentialDuplicatesSearchContext
    from hydrus.client.gui.canvas import ClientGUICanvasDuplicates, ClientGUICanvasFrame
    from hydrus.core import HydrusLists

    # the batch: the first pairs by the sort, ties by the files' ids (as
    # record_duplicate_filter_canvas.py fetches it)
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

    holder = {}

    def qt(f):
        return controller.CallBlockingToQt(gui, f)

    def make():
        holder['watcher'] = Watcher(controller)
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
        canvas._top_hover.sendApplicationCommand.connect(holder['watcher'].command)
    qt(make)

    deadline = time.time() + 60
    while time.time() < deadline:
        time.sleep(0.3)
        if qt(lambda: holder['canvas']._current_media is not None and len(holder['canvas']._batch_of_pairs_to_process) > 0):
            break
    else:
        raise Exception('the filter never showed a pair')
    time.sleep(1.0)

    def drive():
        canvas = holder['canvas']
        hover = canvas._top_hover
        watcher = holder['watcher']
        watcher.take()
        out = {'widgets': public(describe(hover)), 'now': state(canvas, hover)}
        # (skip, back and the next/previous buttons move through the pairs;
        # the move button's left press starts a system move)
        out['presses'] = press_buttons(canvas, hover, watcher)
        canvas._duplicate_pair_decisions = []
        canvas._TryToCloseWindow = lambda: None
        holder['frame'].hide()
        return out
    return qt(drive)


SCENARIOS = {
    'browser': ('basic', browser),
    'archive_delete': ('basic', archive_delete),
    'duplicates': ('auto_resolution', duplicates),
}


def main():
    import hydrus_driver
    import record_api
    if len(sys.argv) > 1 and sys.argv[1] == '--child':
        (fixture, hook) = SCENARIOS[sys.argv[2]]
        output = sys.argv[3]
        result = hydrus_driver.run_client(record_api.unpack_fixture(fixture), hook)
        with open(output, 'w') as stream:
            json.dump(result, stream)
        return
    recorded = {}
    with tempfile.TemporaryDirectory() as work:
        for name in SCENARIOS:
            out = os.path.join(work, name + '.json')
            hydrus_driver.run_in_subprocess(os.path.abspath(__file__), '--child', name, out)
            with open(out) as stream:
                recorded[name] = json.load(stream)
    with open(OUT, 'w') as stream:
        json.dump(recorded, stream, indent=1)
        stream.write('\n')
    print('wrote', OUT)


if __name__ == '__main__':
    main()

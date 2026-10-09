#!/usr/bin/env python3
"""Record the reference's manual export panel naming files under the export
options, removing rows, and browsing for and opening its destination.

A private basic-fixture client builds the real ReviewExportFilesPanel over
four local files (apng and png `.png`, flac `.flac`, jpeg `.jpg`). Then:

- names: for each of a range of export option sets (`always apply NTFS
  filename rules`, the path, dirname and filename length limits, from their
  minimums to their maximums), each of four destinations (short, deep,
  multi-byte and long) and each of a list of phrases (long ASCII, two-, three-
  and four-byte characters, characters and names Windows forbids, trailing
  dots and spaces, Unicode whitespace, deep and empty subdirectories, an
  extension already in the phrase, colliding names), the panel's rows as it
  shows them, errors included;
- remove: five files named by `{#}` (and `{#} {file_id}`); selections removed
  (answering yes), the rows as shown at once, and after the phrase is
  re-entered and updated; a declined removal too;
- paths: the destination's browse button with a scripted directory answer
  (a folder with a trailing slash and `..`, a cancel, a folder that does not
  exist), the path field and rows after each; the open-location button with
  an empty, an existing and a missing destination, a stub `xdg-open` first on
  PATH recording what the reference launches, and the critical dialog it
  shows for the missing one.

Usage: QT_QPA_PLATFORM=offscreen ~/pyenv/bin/python oracle/record_export_names.py
"""
import json
import os
import stat
import sys
import tempfile
import time

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
import hydrus_driver
import record_api

OUT = os.path.join(HERE, 'fixtures', 'export_names.json')

# ( always NTFS, path limit, dirname limit, filename limit )
OPTION_SETS = [
    (False, None, None, 220),
    (True, None, None, 220),
    (False, None, None, 16),
    (True, None, None, 16),
    (False, None, None, 40),
    (True, None, None, 40),
    (False, 96, None, 220),
    (True, 96, None, 220),
    (False, 120, 16, 64),
    (True, 120, 16, 64),
    (False, None, 20, 220),
    (True, None, 20, 220),
    (False, 250, 64, 220),
    (True, 250, 64, 220),
    (False, 8192, 8192, 8192),
    (True, 8192, 8192, 8192),
]

DESTINATIONS = [
    '/tmp/hx',
    '/tmp/hx/' + 'd' * 50,
    '/tmp/hx/日本語ディレクトリ',
    '/tmp/hx/' + 'L' * 120,
]

PHRASES = [
    'x' * 300,
    'é' * 150,
    '日本' * 60,
    '😀' * 70,
    'a😀' * 80,
    'a:b|c*d?e"f<g>h\\i',
    'con',
    'CON',
    'Nul.',
    'com1 ',
    'lpt9x',
    'trail. . ',
    '  spaced  ',
    'name.png',
    'name.flac',
    'tail\x1f',
    '　wide　',
    'd' * 80 + '/leaf',
    '日' * 100 + '/leaf',
    'a//b///c',
    '/leading/x',
    'sub/',
    '',
    'one/two/three/four/five/six/seven/eight/nine/ten/' + 'x' * 50,
    'con/aux./ dots. /x',
    '{#} ' + 'y' * 250,
    'same' + 's' * 250,
    '{hash}',
    'é' * 9 + '/' + '日' * 30,
    '.....',
    '   ',
    'same',
    '../up',
    'a/../../b',
    '/abs/../../c',
    '../../../../../../../../../../../../../../../../../../../../deep',
]

FILE_NAMES = ['apng_animated.png', 'audio.flac', 'jpeg_00.jpg', 'png_00.png']
REMOVE_NAMES = ['apng_animated.png', 'audio.flac', 'jpeg_00.jpg', 'png_00.png', 'video_silent.webm']


def record(session, work):
    controller = session.controller
    gui = controller.GetMainTLW()
    manifest = json.load(open(os.path.join(HERE, 'fixtures', 'legacy_db', 'basic.manifest.json')))
    by_name = {f['name']: bytes.fromhex(f['hash']) for f in manifest['files']}
    launched = os.path.join(work, 'launched.txt')

    def qt():
        from qtpy import QtWidgets as QW
        from hydrus.core import HydrusExceptions
        from hydrus.client.gui import ClientGUIDialogsQuick, ClientGUIDialogsMessage, ClientGUITopLevelWindowsPanels
        from hydrus.client.gui.exporting import ClientGUIExport
        from hydrus.client.media import ClientMediaSingle
        options = controller.new_options

        def medias(names):
            results = controller.Read('media_results', [by_name[n] for n in names])
            order = {by_name[n]: i for (i, n) in enumerate(names)}
            results.sort(key=lambda m: order[m.GetHash()])
            return [ClientMediaSingle.MediaSingle(m) for m in results]

        def panel_over(media, directory):
            frame = ClientGUITopLevelWindowsPanels.FrameThatTakesScrollablePanel(gui, 'export files')
            panel = ClientGUIExport.ReviewExportFilesPanel(frame, media)
            panel._directory_picker.SetPath(directory)
            return (frame, panel)

        def shown(panel):
            return [list(panel._ConvertDataToDisplayTuple(m)) for m in panel._paths.GetData()]

        def refresh(panel, phrase):
            panel._pattern.setText(phrase)
            panel._last_phrase_used = None
            panel._RefreshPaths()

        out = {}
        media = medias(FILE_NAMES)
        out['files'] = [{'name': n, 'file_id': m.GetHashId(), 'hash': m.GetHash().hex()} for (n, m) in zip(FILE_NAMES, media)]
        (frame, panel) = panel_over(media, DESTINATIONS[0])
        cases = []
        try:
            for (ntfs, path_limit, dirname_limit, filename_limit) in OPTION_SETS:
                options.SetBoolean('always_apply_ntfs_export_filename_rules', ntfs)
                options.SetNoneableInteger('export_path_character_limit', path_limit)
                options.SetNoneableInteger('export_dirname_character_limit', dirname_limit)
                options.SetInteger('export_filename_character_limit', filename_limit)
                for destination in DESTINATIONS:
                    panel._directory_picker.SetPath(destination)
                    names = []
                    for phrase in PHRASES:
                        refresh(panel, phrase)
                        names.append([row[2] for row in shown(panel)])
                    cases.append({
                        'ntfs': ntfs, 'path_limit': path_limit, 'dirname_limit': dirname_limit,
                        'filename_limit': filename_limit, 'destination': destination, 'names': names,
                    })
        finally:
            options.SetBoolean('always_apply_ntfs_export_filename_rules', False)
            options.SetNoneableInteger('export_path_character_limit', None)
            options.SetNoneableInteger('export_dirname_character_limit', None)
            options.SetInteger('export_filename_character_limit', 220)
            frame.close()
        out['phrases'] = PHRASES
        out['cases'] = cases

        # removal and renumbering
        media = medias(REMOVE_NAMES)
        out['remove_files'] = [{'name': n, 'file_id': m.GetHashId()} for (n, m) in zip(REMOVE_NAMES, media)]
        said = []
        original = ClientGUIDialogsQuick.GetYesNo
        answer = [QW.QDialog.DialogCode.Accepted]
        ClientGUIDialogsQuick.GetYesNo = lambda parent, message, **kwargs: (said.append(message), answer[0])[1]
        removals = []
        try:
            for (phrase, steps) in [
                ('{#}', [([1], True), ([0, 4], True), ([2], False), ([2], True)]),
                ('{#} {file_id}', [([0, 2], True), ([], True)]),
            ]:
                (frame, panel) = panel_over(medias(REMOVE_NAMES), '/tmp/hx')
                ordered = list(panel._paths.GetData())
                refresh(panel, phrase)
                record_steps = [{'rows': shown(panel)}]
                for (selected, yes) in steps:
                    chosen = [ordered[i] for i in selected]
                    panel._paths.SelectDatas(chosen, deselect_others=True)
                    if len(chosen) == 0:
                        panel._paths.clearSelection()
                    answer[0] = QW.QDialog.DialogCode.Accepted if yes else QW.QDialog.DialogCode.Rejected
                    before = len(said)
                    panel._DeletePaths()
                    step = {'selected': selected, 'yes': yes, 'asked': said[before:], 'rows': shown(panel)}
                    panel._pattern.setText('')
                    panel._RefreshPaths()
                    panel._pattern.setText(phrase)
                    panel._RefreshPaths()
                    step['refreshed'] = shown(panel)
                    record_steps.append(step)
                removals.append({'phrase': phrase, 'steps': record_steps})
                frame.close()
        finally:
            ClientGUIDialogsQuick.GetYesNo = original
        out['removals'] = removals

        # browse and open location
        media = medias(FILE_NAMES[:2])
        (frame, panel) = panel_over(media, '/tmp/hx')
        refresh(panel, '{hash}')
        picked = os.path.join(work, 'picked')
        os.makedirs(os.path.join(picked, 'inner'))
        answers = []
        asked = []

        def pick(parent, message, starting_dir_path=None):
            asked.append([message, starting_dir_path])
            a = answers.pop(0)
            if a is None:
                raise HydrusExceptions.CancelledException()
            return a

        criticals = []
        original_pick = ClientGUIDialogsQuick.PickDirectory
        original_critical = ClientGUIDialogsMessage.ShowCritical
        ClientGUIDialogsQuick.PickDirectory = pick
        ClientGUIDialogsMessage.ShowCritical = lambda win, title, message: criticals.append([title, message])
        browses = []
        try:
            for a in [os.path.join(picked, 'inner', '..') + '/', None, os.path.join(work, 'nowhere')]:
                answers.append(a)
                panel._directory_picker._Browse()
                browses.append({'answer': a, 'field': panel._directory_picker.GetPath(), 'rows': shown(panel)})
            opened = []
            for destination in ['', picked, os.path.join(work, 'missing')]:
                panel._directory_picker.SetPath(destination)
                before = len(criticals)
                panel.EventOpenLocation()
                opened.append({'destination': destination, 'criticals': criticals[before:]})
        finally:
            ClientGUIDialogsQuick.PickDirectory = original_pick
            ClientGUIDialogsMessage.ShowCritical = original_critical
            frame.close()
        out['browse_asked'] = asked
        out['browses'] = browses
        out['opened'] = opened
        out['work'] = work
        return out

    result = controller.CallBlockingToQt(gui, qt)
    # the launch runs on its own thread
    deadline = time.time() + 10
    while not os.path.exists(launched) and time.time() < deadline:
        time.sleep(0.1)
    time.sleep(0.5)
    with open(launched) as f:
        result['launched'] = [line.rstrip('\n').split('\x00') for line in f if line.strip()]
    return result


def main():
    if len(sys.argv) > 1 and sys.argv[1] == '--child':
        output = sys.argv[2]
        work = tempfile.mkdtemp(prefix='export-names-')
        bin_dir = os.path.join(work, 'bin')
        os.makedirs(bin_dir)
        stub = os.path.join(bin_dir, 'xdg-open')
        with open(stub, 'w') as f:
            f.write('#!/bin/sh\nprintf "%s" "$0" >> "{0}"\nfor a in "$@"; do printf "\\0%s" "$a" >> "{0}"; done\necho >> "{0}"\n'.format(os.path.join(work, 'launched.txt')))
        os.chmod(stub, os.stat(stub).st_mode | stat.S_IEXEC)
        os.environ['PATH'] = bin_dir + os.pathsep + os.environ['PATH']
        db = record_api.unpack_fixture('basic')
        result = hydrus_driver.run_client(db, lambda session: record(session, work))
        with open(output, 'w') as f:
            json.dump(result, f, ensure_ascii=False)
        return
    with tempfile.TemporaryDirectory() as tmp:
        path = os.path.join(tmp, 'export_names.json')
        hydrus_driver.run_in_subprocess(os.path.abspath(__file__), '--child', path)
        with open(path) as f:
            result = json.load(f)
    # the scratch folder's name changes run to run
    work = result.pop('work')
    text = json.dumps(result, ensure_ascii=False, indent=1).replace(work, '{work}')
    with open(OUT, 'w') as f:
        f.write(text)
        f.write('\n')
    print('wrote', OUT)


if __name__ == '__main__':
    main()

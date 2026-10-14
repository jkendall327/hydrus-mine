#!/usr/bin/env python3
"""Record pause and stop on the reference's "review files to import" dialog.

In the running client, on the `basic` fixture, the real
`ReviewLocalFileImports` panel is given paths while its parsing is held (the
pause event set first, as the pause button sets it) and then:

- "pause": one path added while paused; recorded while it waits (progress
  text, buttons, rows), then the pause button pressed again (resume) and the
  state once parsed;
- "stop": paused, two more paths added, the stop button pressed; the state
  straight after (the unparsed paths dropped, the parsed row kept), then a
  path added again to show the panel still parses.

Each state is `{progress, enabled: {import now, add tags/urls, pause, stop},
rows: [[#, filename, filetype]]}`.

Usage: QT_QPA_PLATFORM=offscreen python oracle/record_local_import_pause.py
       (writes fixtures/local_import_pause.json)
"""
import json, os, shutil, sys, tempfile, time

HERE = os.path.dirname(os.path.abspath(__file__))
OUT = os.path.join(HERE, 'fixtures', 'local_import_pause.json')
MEDIA = os.path.join(HERE, 'fixtures', 'media')
NAMES = ['bmp_24.bmp', 'gif_static.gif', 'apng_rgba.png']


def record(session):
    controller = session.controller
    gui = controller.gui
    from hydrus.client.gui.panels import ClientGUILocalFileImports

    work = tempfile.mkdtemp()
    paths = []
    for name in NAMES:
        shutil.copy(os.path.join(MEDIA, name), os.path.join(work, name))
        paths.append(os.path.join(work, name))

    def qt(f, *args):
        return controller.CallBlockingToQt(gui, f, *args)

    def state(panel):
        listing = sorted(panel._paths_list.GetData(), key=lambda p: p.index)
        rows = [[p.index, os.path.basename(p.path), panel._ConvertPathToDisplayTuple(p)[2]] for p in listing]
        return {
            'progress': panel._progress._st.text(),
            'enabled': {
                'import now': panel._add_button.isEnabled(),
                'add tags/urls with the import >>': panel._tag_button.isEnabled(),
                'pause': panel._progress_pause.isEnabled(),
                'stop': panel._progress_cancel.isEnabled(),
            },
            'rows': rows,
        }

    def settle(panel):
        for _ in range(100):
            time.sleep(0.1)
            if qt(lambda: panel._unparsed_paths_queue.empty() and panel._parsed_paths_queue.empty()):
                break
        time.sleep(0.3)

    def make():
        return ClientGUILocalFileImports.ReviewLocalFileImports(gui)

    out = {'files': NAMES}

    panel = qt(make)
    time.sleep(0.3)
    first, second, third = paths
    # paused before anything is parsed
    qt(panel.PauseProgress)
    qt(panel._AddPathsToList, [first])
    time.sleep(0.5)
    pause = {'held': qt(state, panel)}
    qt(panel.PauseProgress)
    settle(panel)
    pause['resumed'] = qt(state, panel)
    out['pause'] = pause
    # more paths, then stop before they are parsed
    stop = {}
    qt(panel.PauseProgress)
    qt(panel._AddPathsToList, [second, third])
    time.sleep(0.5)
    stop['held'] = qt(state, panel)
    qt(panel.Cancel)
    time.sleep(0.8)
    stop['stopped'] = qt(state, panel)
    qt(panel._AddPathsToList, [second])
    settle(panel)
    stop['again'] = qt(state, panel)
    out['stop'] = stop
    qt(panel.deleteLater)
    return out


def child(out):
    import hydrus_driver, record_api
    sys.path.insert(0, HERE)
    result = hydrus_driver.run_client(record_api.unpack_fixture('basic'), record)
    with open(out, 'w') as f:
        json.dump(result, f, ensure_ascii=False)


def main():
    sys.path.insert(0, HERE)
    if len(sys.argv) > 1 and sys.argv[1] == '--child':
        child(sys.argv[2])
        return
    import hydrus_driver
    with tempfile.TemporaryDirectory() as work:
        path = os.path.join(work, 'r.json')
        hydrus_driver.run_in_subprocess(os.path.abspath(__file__), '--child', path)
        with open(path) as f:
            result = json.load(f)
    with open(OUT, 'w') as f:
        json.dump(result, f, indent=1, ensure_ascii=False)
        f.write('\n')
    print(json.dumps(result, indent=1))


if __name__ == '__main__':
    main()

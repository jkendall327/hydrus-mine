#!/usr/bin/env python3
"""Record the reference's duplicate filter canvas keeping zoom and position between a pair's
files of different sizes and shapes (`ZoomMaintainingZoom`).

On the basic fixture, the real `CanvasFilterDuplicates` (1000x750) is opened on pairs the
reference's `PotentialDuplicatePairFactoryMediaResults` is given: landscape against portrait,
two long thin ones, a large landscape against a small one, a square against a tiny square,
and two portraits. Each pair is zoomed to 100%, switched to the other file, zoomed in, panned
and switched back, recording after every command which file is shown, the zoom and where the
file sits on the canvas.

Usage: QT_QPA_PLATFORM=offscreen ~/pyenv/bin/python oracle/record_duplicate_filter_sizes.py
       (writes fixtures/duplicate_filter_sizes.json)
"""
import json
import os
import sys
import tempfile
import time
HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)

# basic fixture file ids (hash_id) of the pairs
PAIRS = [(10, 23), (25, 27), (33, 39), (29, 37), (12, 33), (31, 23)]
COMMANDS = {
    'switch': 'SIMPLE_VIEW_NEXT',
    'skip': 'SIMPLE_DUPLICATE_FILTER_SKIP',
    'zoom_switch': 'SIMPLE_SWITCH_BETWEEN_100_PERCENT_AND_CANVAS_ZOOM',
    'zoom_in': 'SIMPLE_ZOOM_IN',
    'pan_right': 'SIMPLE_PAN_RIGHT',
    'pan_down': 'SIMPLE_PAN_DOWN',
}
# each pair: shown fitted, switched, then zoomed, switched, zoomed in and panned, switched
STEPS = ['switch', 'switch', 'zoom_switch', 'switch', 'switch', 'zoom_in', 'switch', 'pan_right', 'pan_down', 'switch', 'zoom_in', 'zoom_in', 'switch', 'skip']


def record(session):
    c = session.controller
    qt = lambda f, *a: c.CallBlockingToQt(c.gui, f, *a)

    def make():
        from hydrus.client.duplicates import ClientPotentialDuplicatesPairFactory as F
        from hydrus.client.duplicates import ClientPotentialDuplicatesSearchContext as S
        from hydrus.client.gui.canvas import ClientGUICanvasDuplicates, ClientGUICanvasFrame
        results = {m.GetHash(): m for m in c.Read('media_results_from_ids', list(range(1, 41)))}
        by_id = c.Read('media_results_from_ids', list(range(1, 41)))
        pairs = [(by_id[a - 1], by_id[b - 1], 0) for (a, b) in PAIRS]
        factory = F.PotentialDuplicatePairFactoryMediaResults(S.PotentialDuplicateMediaResultPairsAndDistances(pairs))
        frame = ClientGUICanvasFrame.CanvasFrame(c.gui)
        canvas = ClientGUICanvasDuplicates.CanvasFilterDuplicates(frame, factory)
        frame.SetCanvas(canvas)
        frame.showNormal()
        frame.resize(1000, 750)
        return frame, canvas

    frame, canvas = qt(make)

    def observe():
        media = canvas._current_media
        container = canvas._media_container
        g = container.geometry()
        return dict(index=canvas._GetIndexString(), shown=None if media is None else media.GetHash().hex(),
                    zoom=container.GetCurrentZoom(), canvas=[canvas.width(), canvas.height()],
                    container=[g.x(), g.y(), g.width(), g.height()],
                    resolution=None if media is None else list(media.GetResolution()))

    def settle():
        deadline = time.time() + 30
        last = None
        while time.time() < deadline:
            time.sleep(0.4)
            now = qt(observe)
            if now['shown'] is not None and now == last:
                return now
            last = now
        return last

    def command(name):
        from hydrus.client import ClientApplicationCommand as CAC
        action = getattr(CAC, COMMANDS[name])
        qt(lambda: canvas.ProcessApplicationCommand(CAC.ApplicationCommand.STATICCreateSimpleCommand(action)))

    pairs_out = []
    first = settle()
    current = dict(start=first, steps=[])
    for i in range(len(PAIRS)):
        for step in STEPS:
            if step == 'skip' and i == len(PAIRS) - 1:
                break
            command(step)
            current['steps'].append(dict(do=step, after=settle()))
        pairs_out.append(current)
        current = dict(start=current['steps'][-1]['after'] if current['steps'] else None, steps=[])
    qt(frame.deleteLater)
    hashes = qt(lambda: [[m.GetHash().hex() for m in (c.Read('media_results_from_ids', [a])[0], c.Read('media_results_from_ids', [b])[0])] for (a, b) in PAIRS])
    return dict(pairs=hashes, scenario=pairs_out)


def main():
    import hydrus_driver
    if len(sys.argv) > 1 and sys.argv[1] == '--child':
        import record_api
        output = sys.argv[2]
        value = hydrus_driver.run_client(record_api.unpack_fixture('basic'), record)
        with open(output, 'w') as f:
            json.dump(value, f)
        return
    with tempfile.TemporaryDirectory() as tmp:
        path = os.path.join(tmp, 'result.json')
        hydrus_driver.run_in_subprocess(os.path.abspath(__file__), '--child', path)
        with open(path) as f:
            value = json.load(f)
    with open(os.path.join(HERE, 'fixtures', 'duplicate_filter_sizes.json'), 'w') as f:
        json.dump(value, f, indent=1, ensure_ascii=False)
        f.write('\n')
    print('recorded the duplicate filter sizes')


if __name__ == '__main__':
    main()

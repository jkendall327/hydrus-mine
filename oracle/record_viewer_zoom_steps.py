#!/usr/bin/env python3
"""Record the reference's media viewer zooming in and out, under the media
zooms list and the zoom centrepoint options.

A private basic-fixture client opens a media viewer (`CanvasMediaListBrowser`)
on one small and one large jpeg of its files, sized so the canvas is
800 x 600. For each of a few lists of "Media zooms", each of the four
centrepoints, and each of a few positions of the mouse over (or outside) the
viewer, and with the file dragged away from the middle first or not, the
viewer zooms in until it can zoom in no more and then out until it can zoom out
no more (through the real application commands the keyboard and wheel send),
recording after each step the file's zoom and where the media container
(its x, y, width and height) is on the canvas.

Usage: QT_QPA_PLATFORM=offscreen ~/pyenv/bin/python oracle/record_viewer_zoom_steps.py
"""
import json
import os
import sys
import tempfile
import time

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)

OUT = os.path.join(HERE, 'fixtures', 'viewer_zoom_steps.json')
MANIFEST = json.load(open(os.path.join(HERE, 'fixtures', 'legacy_db', 'basic.manifest.json')))

ZOOM_LISTS = [None, [0.5, 1.0, 3.0], [1.0, 2.0, 4.0, 8.0], [2.0], [0.25, 0.3333, 0.75, 1.5, 7.0]]
# the reference's constants: 1 viewer centre, 2 mouse, 0 media centre, 3 media top-left
CENTRES = [1, 2, 0, 3]
MICE = [(600, 150), (50, 500), (-30, 20), (400, 300)]
DRAGS = [(0, 0), (-37, 21)]
PARSE_TEXTS = ['0.5, 1, 3', '1', '', ' 2 ', '0', '-1, 2', '0, 2, -3', '0.5,,2', 'abc', '1_0', '1e1', '.5, 5.', '\uff11, \u0663', 'nan, 2', '0.1,0.2,0.3,0.4,0.5,0.6,0.7,0.8,0.9', '3,1,2']


def record(session):
    controller = session.controller
    gui = controller.gui
    qt = lambda f: controller.CallBlockingToQt(gui, f)

    from qtpy import QtCore as QC
    from hydrus.client import ClientApplicationCommand as CAC
    from hydrus.client import ClientConstants as CC
    from hydrus.client import ClientLocation
    from hydrus.client.gui import ClientGUIFunctions
    from hydrus.client.gui.canvas import ClientGUICanvas
    from hydrus.client.gui.canvas import ClientGUICanvasFrame

    options = controller.new_options
    names = {bytes.fromhex(f['hash']): f['name'] for f in MANIFEST['files']}
    results = controller.Read('media_results', list(names))
    location = ClientLocation.LocationContext.STATICCreateSimple(CC.LOCAL_FILE_SERVICE_KEY)
    in_viewer = [m for m in results if CC.LOCAL_FILE_SERVICE_KEY in m.GetLocationsManager().GetCurrent()]
    jpegs = [(m, m.GetResolution()) for m in in_viewer if names[m.GetHash()].endswith('.jpg')]
    small = [m for (m, (w, h)) in jpegs if w < 400 and h < 300][0]
    large = [m for (m, (w, h)) in jpegs if w > 1000 and h > 1000][0]
    default_zooms = options.GetMediaZooms()
    default_centre = options.GetInteger('media_viewer_zoom_center')
    original_mouse = ClientGUIFunctions.GetMousePos

    out = {'default_zooms': default_zooms, 'files': [], 'cases': []}
    for (label, media) in (('small', small), ('large', large)):
        out['files'].append({'label': label, 'name': names[media.GetHash()], 'hash': media.GetHash().hex(), 'size': list(media.GetResolution())})

    holder = {}

    def open_viewer(media):
        frame = ClientGUICanvasFrame.CanvasFrame(gui)
        canvas = ClientGUICanvas.CanvasMediaListBrowser(frame, os.urandom(32), location, in_viewer, media.GetHash())
        frame.SetCanvas(canvas)
        frame.resize(800, 600)
        holder['frame'] = frame
        holder['canvas'] = canvas

    def fit_canvas():
        canvas = holder['canvas']
        for _ in range(4):
            size = canvas.size()
            if (size.width(), size.height()) == (800, 600):
                return
            frame = holder['frame']
            frame.resize(frame.width() + 800 - size.width(), frame.height() + 600 - size.height())
            for _ in range(5):
                from qtpy import QtWidgets as QW
                QW.QApplication.processEvents()

    def geometry():
        container = holder['canvas']._media_container
        return {'x': container.x(), 'y': container.y(), 'w': container.width(), 'h': container.height(), 'zoom': container._current_zoom}

    for (label, media) in (('small', small), ('large', large)):
        qt(lambda: open_viewer(media))
        time.sleep(0.8)
        qt(fit_canvas)
        time.sleep(0.3)
        canvas = holder['canvas']
        container = canvas._media_container
        canvas_size = qt(lambda: [canvas.width(), canvas.height()])

        def command(action):
            canvas.ProcessApplicationCommand(CAC.ApplicationCommand.STATICCreateSimpleCommand(action))
            from qtpy import QtWidgets as QW
            for _ in range(3):
                QW.QApplication.processEvents()

        for zooms in ZOOM_LISTS:
            options.SetMediaZooms(list(default_zooms if zooms is None else zooms))
            for centre in CENTRES:
                options.SetInteger('media_viewer_zoom_center', centre)
                for mouse in (MICE if centre == 2 else MICE[:1]):
                    for drag in DRAGS:

                        def run():
                            ClientGUIFunctions.GetMousePos = lambda: canvas.mapToGlobal(QC.QPoint(*mouse))
                            try:
                                # back to the opening zoom, the file in the middle
                                canvas.ProcessApplicationCommand(CAC.ApplicationCommand.STATICCreateSimpleCommand(CAC.SIMPLE_ZOOM_CANVAS))
                                for _ in range(3):
                                    from qtpy import QtWidgets as QW
                                    QW.QApplication.processEvents()
                                container.ResetCenterPosition()
                                if drag != (0, 0):
                                    container.move(container.pos() + QC.QPoint(*drag))
                                steps = [{'op': 'start', **geometry()}]
                                for (op, action) in [('in', CAC.SIMPLE_ZOOM_IN)] * 12 + [('out', CAC.SIMPLE_ZOOM_OUT)] * 14:
                                    command(action)
                                    steps.append({'op': op, **geometry()})
                                return steps
                            finally:
                                ClientGUIFunctions.GetMousePos = original_mouse

                        steps = qt(run)
                        out['cases'].append({
                            'file': label,
                            'zooms': options.GetMediaZooms(),
                            'centre': centre,
                            'mouse': list(mouse),
                            'drag': list(drag),
                            'canvas': canvas_size,
                            'steps': steps,
                        })

        def close():
            holder['frame'].close()

        qt(close)
        time.sleep(0.3)
    options.SetInteger('media_viewer_zoom_center', default_centre)

    # the text of the Media zooms box, as the options panel reads it
    from hydrus.client.gui.panels.options.MediaPlaybackPanel import MediaPlaybackPanel
    parses = []
    for text in PARSE_TEXTS:
        options.SetMediaZooms([0.5, 2.0])

        def type_it():
            panel = MediaPlaybackPanel(gui)
            panel._media_zooms.setText(text)
            panel.UpdateOptions()
            panel.deleteLater()
            return options.GetMediaZooms()

        parses.append({'text': text, 'saved': qt(type_it)})
    out['parses'] = parses
    options.SetMediaZooms(default_zooms)
    options.SetInteger('media_viewer_zoom_center', default_centre)
    return out


def child(out):
    import hydrus_driver
    import record_api
    result = hydrus_driver.run_client(record_api.unpack_fixture('basic'), record)
    with open(out, 'w') as f:
        json.dump(result, f)


def main():
    if len(sys.argv) > 1 and sys.argv[1] == '--child':
        child(sys.argv[2])
        return
    import hydrus_driver
    with tempfile.TemporaryDirectory() as tmp:
        path = os.path.join(tmp, 'out.json')
        hydrus_driver.run_in_subprocess(os.path.abspath(__file__), '--child', path)
        with open(path) as f:
            result = json.load(f)
    with open(OUT, 'w') as f:
        json.dump(result, f)
        f.write('\n')
    print('wrote', OUT)


if __name__ == '__main__':
    main()

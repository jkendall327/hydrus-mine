#!/usr/bin/env python3
"""Record real duplicate Options staging and real A/B canvas colour consumers.

Drive NoneableSpinCtrl/checks, UpdateOptions, serialization and reopen. A real
CanvasFilterDuplicates over existing fixture media switches A/B; its own colour
generator and StaticImage painter record normal/checker/greenscreen outputs.
No canvas business handlers, media writes, relationship decisions or global
colour-adjustment implementation are replaced.
"""
import json
import os
import sys
import tempfile

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
OUT = os.path.join(HERE, 'fixtures', 'duplicate_colours.json')
A, B = 'duplicate_background_switch_intensity_a', 'duplicate_background_switch_intensity_b'
CHECK = 'draw_transparency_checkerboard_media_canvas_duplicates'


def record(session):
    def work():
        from qtpy import QtCore as QC, QtGui as QG, QtWidgets as QW, QtTest as QT
        from hydrus.core import HydrusSerialisable
        from hydrus.client import ClientConstants as CC
        from hydrus.client.duplicates import ClientPotentialDuplicatesPairFactory as Factory
        from hydrus.client.duplicates import ClientPotentialDuplicatesSearchContext as Search
        from hydrus.client.gui.panels.options.DuplicatesPanel import DuplicatesPanel
        from hydrus.client.gui.canvas import ClientGUICanvasDuplicates, ClientGUICanvasFrame, ClientGUICanvasMedia
        c = session.controller
        original = c.new_options.Duplicate()
        dialogs = []
        frame = None
        def settle():
            QT.QTest.qWait(50)
            QW.QApplication.processEvents()
        def prefs(options=None):
            options = options or c.new_options
            return [options.GetNoneableInteger(A), options.GetNoneableInteger(B), options.GetBoolean(CHECK)]
        def displayed(page):
            return [page._duplicate_background_switch_intensity_a.GetValue(),
                    page._duplicate_background_switch_intensity_b.GetValue(),
                    page._draw_transparency_checkerboard_media_canvas_duplicates.isChecked()]
        def page():
            result = DuplicatesPanel(c.gui, c.new_options)
            dialogs.append(result)
            return result
        try:
            initial = prefs()
            p = page()
            initial_displayed = displayed(p)
            labels = [w.text() for w in p.findChildren(QW.QLabel)
                      if 'intensity for ' in w.text() or 'checkerboard in the duplicate' in w.text()]
            bounds = [p._duplicate_background_switch_intensity_a._number_value.minimum(),
                      p._duplicate_background_switch_intensity_a._number_value.maximum()]
            events = []
            for values in ([0, 3, True], [None, None, False], [1, 9, True], [9, 1, False], [0, 10, True]):
                p = page()
                before = prefs()
                p._duplicate_background_switch_intensity_a.SetValue(values[0])
                p._duplicate_background_switch_intensity_b.SetValue(values[1])
                p._draw_transparency_checkerboard_media_canvas_duplicates.setChecked(values[2])
                staged = prefs()
                shown = displayed(p)
                p.UpdateOptions()
                encoded = HydrusSerialisable.CreateFromSerialisableTuple(c.new_options.GetSerialisableTuple())
                reopened = page()
                events.append(dict(input=values, before=before, staged=staged, displayed=shown,
                                   saved=prefs(), round_trip=prefs(encoded), reopened=displayed(reopened)))
            cancelled = page()
            cancelled._duplicate_background_switch_intensity_a.SetValue(None)
            cancelled._duplicate_background_switch_intensity_b.SetValue(None)
            cancelled._draw_transparency_checkerboard_media_canvas_duplicates.setChecked(False)
            cancel_before = prefs()
            dialogs.remove(cancelled)
            cancelled.deleteLater()
            cancel_after = prefs()
            manifest = json.load(open(os.path.join(HERE, 'fixtures', 'legacy_db', 'basic.manifest.json')))
            named = {f['name']: bytes.fromhex(f['hash']) for f in manifest['files']}
            results = c.Read('media_results', [named['png_alpha_00.png'], named['jpeg_00.jpg']])
            ordered = [next(m for m in results if m.GetHash() == named[name])
                       for name in ('png_alpha_00.png', 'jpeg_00.jpg')]
            factory = Factory.PotentialDuplicatePairFactoryMediaResults(Search.PotentialDuplicateMediaResultPairsAndDistances([(ordered[0], ordered[1], 0)]))
            frame = ClientGUICanvasFrame.CanvasFrame(c.gui)
            canvas = ClientGUICanvasDuplicates.CanvasFilterDuplicates(frame, factory)
            frame.SetCanvas(canvas)
            frame.showNormal()
            frame.resize(800, 600)
            settle()
            assert canvas.IsShowingAPair()
            assert canvas.IsShowingFileA()
            generator = canvas._background_colour_generator
            base = list(canvas.GetColour(CC.COLOUR_MEDIA_BACKGROUND).getRgb())[:3]
            colours = []
            bases = [[v, v, v] for v in (0, 1, 32, 52, 127, 191, 192, 254, 255)]
            bases += [[17, 94, 203], [250, 180, 10], [10, 230, 80], [23, 20, 25]]
            for rgb in bases:
                canvas.set_hmv_background(QG.QColor(*rgb))
                for intensity in (None, 0, 1, 2, 3, 4, 5, 6, 7, 8, 9):
                    c.new_options.SetNoneableInteger(A, intensity)
                    c.new_options.SetNoneableInteger(B, intensity)
                    colours.append(dict(base=rgb, intensity=intensity, colour=list(generator.GetColour().getRgb())[:3]))
            canvas.set_hmv_background(QG.QColor(*base))
            static = ClientGUICanvasMedia.StaticImage(canvas, CC.CANVAS_MEDIA_VIEWER_DUPLICATES, generator)
            static._media = ordered[0]
            canvas_events = []
            for a, b, checker, green in ((None, None, False, False), (1, 9, False, False), (9, 1, True, False), (3, 6, True, True)):
                c.new_options.SetNoneableInteger(A, a)
                c.new_options.SetNoneableInteger(B, b)
                c.new_options.SetBoolean(CHECK, checker)
                c.new_options.SetBoolean('draw_transparency_checkerboard_as_greenscreen', green)
                for wanted_a in (True, False):
                    if canvas.IsShowingFileA() != wanted_a:
                        canvas.SwitchMedia(canvas.GetCanvasKey())
                        settle()
                    static._media = canvas.GetMedia()
                    image = QG.QImage(64, 64, QG.QImage.Format.Format_RGB32)
                    painter = QG.QPainter(image)
                    static._DrawBackground(painter)
                    painter.end()
                    canvas_events.append(dict(a=a, b=b, checker=checker, green=green, file_a=canvas.IsShowingFileA(),
                                              transparent=canvas.GetMedia().GetFileInfoManager().has_transparency,
                                              colour=list(generator.GetColour().getRgb())[:3],
                                              pixels={f'{x},{y}':list(image.pixelColor(x,y).getRgb())[:3]
                                                      for x,y in ((0,0),(16,0),(0,16),(16,16),(32,0))}))
            frame.grab().save(OUT.replace('.json', '.png'))
            frame.hide()
            return dict(initial=initial, initial_displayed=initial_displayed, labels=labels, bounds=bounds,
                        options=events, cancel_before=cancel_before, cancel_after=cancel_after,
                        base=base, colours=colours, canvas=canvas_events)
        finally:
            if frame is not None:
                frame.hide()
                frame.deleteLater()
            for p in dialogs:
                p.deleteLater()
            c.new_options = original
    return session.controller.CallBlockingToQt(session.controller.gui, work)


def main():
    import hydrus_driver
    import record_api
    if len(sys.argv) > 1:
        output = sys.argv[2]
        result = hydrus_driver.run_client(record_api.unpack_fixture('basic'), record)
        with open(output, 'w') as stream:
            json.dump(result, stream)
        return
    with tempfile.TemporaryDirectory() as directory:
        output = os.path.join(directory, 'result.json')
        hydrus_driver.run_in_subprocess(os.path.abspath(__file__), '--child', output)
        result = json.load(open(output))
    with open(OUT, 'w') as stream:
        json.dump(result, stream, indent=2)
        stream.write('\n')
    print('wrote', OUT)


if __name__ == '__main__':
    main()

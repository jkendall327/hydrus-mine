#!/usr/bin/env python3
"""Record the reference's rating service star appearance: its SVG choices and
what a star drawn from an SVG looks like.

The real `EditServiceStarRatingsSubPanel` is built over a stub preview that
notes what it is told, with the installed SVGs, with a user's own added to the
database directory's `static/star_shapes` (one sharing a bundled name), and
with none found at all. The real `DrawShape` paints chosen SVGs onto
transparent images at several sizes in a given border and fill (the PNGs are
the pixels; Qt's rasteriser is not resvg's, so a test compares coverage).

Usage: QT_QPA_PLATFORM=offscreen python oracle/record_star_svgs.py
"""
import json
import os
import shutil
import sys
import tempfile

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)

SIZES = [12, 16, 24, 32]
DRAWN = ['smile', 'star', 'spiral', 'my star']
BORDER = '#112233'
FILL = '#dd8800'
CUSTOM_SVG = (
    '<svg xmlns="http://www.w3.org/2000/svg" width="64" height="64" viewBox="0 0 64 64">'
    '<path d="M8 8h48v48h-48z M20 20v24h24v-24z" fill="#000000" fill-rule="evenodd"/></svg>\n'
)


def record(session, out_dir):
    c = session.controller

    def work():
        from qtpy import QtCore as QC, QtGui as QG, QtWidgets as QW
        from hydrus.core import HydrusStaticDir
        from hydrus.client import ClientConstants as CC
        from hydrus.client.gui.services import ClientGUIClientsideServices as S
        from hydrus.client.gui.widgets import ClientGUIPainterShapes as P
        from hydrus.client.metadata import ClientRatings

        class Preview(QC.QObject):
            SetExampleData = QC.Signal(str, object)

            def __init__(self):
                super().__init__()
                self.told = []
                self.SetExampleData.connect(lambda key, value: self.told.append([key, value]))

        def describe(panel, preview):
            svg = panel._rating_svg
            return dict(
                radio_visible=panel._shape_or_svg.isVisibleTo(panel),
                radio_choices=[panel._shape_or_svg.GetValue()] if False else None,
                shape_enabled=panel._shape.isEnabled(),
                svg_enabled=panel._rating_svg.isEnabled(),
                svg_visible=panel._rating_svg.isVisibleTo(panel),
                svg_choices=[[svg.itemText(i), svg.itemData(i)] for i in range(svg.count())],
                shapes=[[panel._shape.itemText(i), panel._shape.itemData(i)] for i in range(panel._shape.count())],
                value=panel.GetValue(),
                told=list(preview.told),
            )

        out = {}
        user_dir = os.path.join(c.GetDBDir(), 'static', 'star_shapes')
        window = QW.QWidget()
        try:
            def build(dictionary):
                preview = Preview()
                panel = S.EditServiceStarRatingsSubPanel(window, dictionary, preview)
                panel.show()
                QW.QApplication.processEvents()
                return panel, preview

            # the installed SVGs only
            panel, preview = build({'shape': 3, 'rating_svg': None})
            out['installed'] = describe(panel, preview)
            radio = panel._shape_or_svg
            radio.SetValue('rating_svg')
            radio.radioBoxChanged.emit()
            out['installed_svg_selected'] = describe(panel, preview)
            panel._rating_svg.SetValue('smile')
            out['installed_smile_chosen'] = describe(panel, preview)
            radio.SetValue('shape')
            radio.radioBoxChanged.emit()
            out['installed_shape_again'] = describe(panel, preview)
            panel.deleteLater()

            # opening on a service that already draws an SVG
            panel, preview = build({'shape': None, 'rating_svg': 'spiral'})
            out['opened_on_svg'] = describe(panel, preview)
            panel.deleteLater()
            # ... and one whose SVG is not there (the first choice is shown)
            panel, preview = build({'shape': None, 'rating_svg': 'gone'})
            out['opened_on_missing_svg'] = describe(panel, preview)
            panel.deleteLater()

            # the user's own, one of them named as an installed one is
            os.makedirs(user_dir, exist_ok=True)
            with open(os.path.join(user_dir, 'my star.svg'), 'w') as f:
                f.write(CUSTOM_SVG)
            with open(os.path.join(user_dir, 'Smile.SVG'), 'w') as f:
                f.write(CUSTOM_SVG)
            with open(os.path.join(user_dir, 'smile.svg'), 'w') as f:
                f.write(CUSTOM_SVG)
            with open(os.path.join(user_dir, 'readme.md'), 'w') as f:
                f.write('not an svg')
            panel, preview = build({'shape': 3, 'rating_svg': None})
            out['with_user_svgs'] = describe(panel, preview)
            panel.deleteLater()
            out['user_svg_paths'] = {
                name: os.path.relpath(path, c.GetDBDir()) if path.startswith(c.GetDBDir()) else 'install:' + os.path.basename(path)
                for name, path in CC.global_icons().user_svg_paths.items()
            }

            # the SVGs, as drawn
            ratio = {}
            for name in DRAWN:
                for size in SIZES:
                    image = QG.QImage(size, size, QG.QImage.Format.Format_ARGB32)
                    image.fill(QC.Qt.GlobalColor.transparent)
                    painter = QG.QPainter(image)
                    painter.setRenderHint(QG.QPainter.RenderHint.Antialiasing, True)
                    painter.setPen(QG.QPen(QG.QColor(BORDER)))
                    painter.setBrush(QG.QBrush(QG.QColor(FILL)))
                    P.DrawShape(painter, ClientRatings.StarType(None, name), 0, 0, width=size, height=size)
                    painter.end()
                    file_name = 'star_svg_%s_%d.png' % (name.replace(' ', '_'), size)
                    image.save(os.path.join(out_dir, file_name))
                    ratio[file_name] = [name, size]
            # one that is not there is the default star
            for size in SIZES:
                image = QG.QImage(size, size, QG.QImage.Format.Format_ARGB32)
                image.fill(QC.Qt.GlobalColor.transparent)
                painter = QG.QPainter(image)
                painter.setRenderHint(QG.QPainter.RenderHint.Antialiasing, True)
                painter.setPen(QG.QPen(QG.QColor(BORDER)))
                painter.setBrush(QG.QBrush(QG.QColor(FILL)))
                P.DrawShape(painter, ClientRatings.StarType(None, 'gone'), 0, 0, width=size, height=size)
                painter.end()
                file_name = 'star_svg_gone_%d.png' % size
                image.save(os.path.join(out_dir, file_name))
                ratio[file_name] = ['gone', size]
            out['drawn'] = ratio
            out['border'] = BORDER
            out['fill'] = FILL
            out['sizes'] = SIZES

            # no SVGs found at all
            real = HydrusStaticDir.ListStaticDirFilePaths
            HydrusStaticDir.ListStaticDirFilePaths = lambda *args, **kwargs: []
            try:
                panel, preview = build({'shape': 3, 'rating_svg': None})
                out['none_found'] = describe(panel, preview)
                panel.deleteLater()
            finally:
                HydrusStaticDir.ListStaticDirFilePaths = real
                CC.global_icons().RefreshUserIcons()
            shutil.rmtree(user_dir)
        finally:
            window.deleteLater()
        return out

    return c.CallBlockingToQt(c.gui, work)


def main():
    import hydrus_driver
    if len(sys.argv) > 1 and sys.argv[1] == '--child':
        output = sys.argv[2]
        out_dir = os.path.join(HERE, 'fixtures')
        import record_api
        result = hydrus_driver.run_client(record_api.unpack_fixture('basic'), lambda session: record(session, out_dir))
        with open(output, 'w') as f:
            json.dump(result, f, ensure_ascii=False, indent=2)
        return
    with tempfile.TemporaryDirectory() as work:
        output = os.path.join(work, 'result.json')
        hydrus_driver.run_in_subprocess(os.path.abspath(__file__), '--child', output)
        with open(output) as f:
            result = json.load(f)
        with open(os.path.join(HERE, 'fixtures/star_svgs.json'), 'w') as f:
            json.dump(result, f, ensure_ascii=False, indent=2)
            f.write('\n')


if __name__ == '__main__':
    main()

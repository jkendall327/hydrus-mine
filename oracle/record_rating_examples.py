#!/usr/bin/env python3
"""Record the reference's ratings options page examples: the service whose
style they show, and the sizes typed beside them.

A private basic-fixture client builds the real `RatingsPanel`. For each choice
of the "Select rating service for styling numerical stars" box (and back
again), the numerical example's drawing inputs are recorded as `DrawNumerical`
takes them (`GetStars`: the shape, and for each run of stars their count and
pen and brush colours) for an unrated example and for several ratings set, with
the fraction text, the stars' count and pad and the choice saved in the
options. Then each of the page's icon size and inc/dec height boxes is given a
range of values (below its minimum, fractional, at and above its maximum) and
its editing finished; the box's value, minimum, maximum and decimals and the
example's size afterwards are recorded.

Usage: QT_QPA_PLATFORM=offscreen ~/pyenv/bin/python oracle/record_rating_examples.py
"""
import json
import os
import sys
import tempfile

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)

OUT = os.path.join(HERE, 'fixtures', 'rating_examples.json')

VALUES = [0.0, 0.5, 1.0, 1.9, 5.0, 11.99, 12.0, 17.5, 30.9, 64.0, 127.9, 128.0, 150.0, 255.0, 300.0, 1000.0]


def colour(c):
    return [c.red(), c.green(), c.blue(), c.alpha()]


def record(session):
    controller = session.controller
    gui = controller.gui

    def work():
        from qtpy import QtWidgets as QW
        from hydrus.core import HydrusConstants as HC
        from hydrus.client import ClientConstants as CC
        from hydrus.client.gui import ClientGUIRatings as R
        from hydrus.client.gui.panels.options.RatingsPanel import RatingsPanel
        from hydrus.client.metadata import ClientRatings

        options = controller.new_options
        panel = RatingsPanel(gui, options)
        panel.show()
        for _ in range(4):
            QW.QApplication.processEvents()
        dropdown = panel._service_template_dropdown
        choices = [dropdown.itemText(i) for i in range(dropdown.count())]
        out = {'choices': choices, 'initial': dropdown.currentText()}

        def stars_of(state, rating):
            key = panel._example_star_service.GetServiceKey()
            (star_type, stars) = R.GetStars(key, state, rating)
            service = controller.services_manager.GetService(key)
            return {
                'shape': int(star_type.GetShape()),
                'groups': [{'count': count, 'pen': colour(pen), 'brush': colour(brush)} for (count, pen, brush) in stars],
                'fraction': R.GetNumericalFractionText(state, stars),
                'num_stars': service.GetNumStars(),
                'pad': service.GetCustomPad(),
                'size': [panel._media_viewer_star_example.sizeHint().width(), panel._media_viewer_star_example.sizeHint().height()],
            }

        styles = []
        order = list(range(len(choices))) + list(reversed(range(len(choices))))
        for index in order:
            dropdown.setCurrentIndex(index)
            for _ in range(3):
                QW.QApplication.processEvents()
            styles.append({
                'choice': dropdown.currentText(),
                'saved': controller.services_manager.GetName(options.GetKey('options_ratings_panel_template_service_key')),
                'null': stars_of(ClientRatings.NULL, 0.0),
                'rated': [{'rating': r, **stars_of(ClientRatings.SET, r)} for r in (0.0, 0.2, 0.4, 0.6, 0.8, 1.0)],
            })
        out['styles'] = styles

        # the page opened again with each service saved as the style
        reopened = []
        for (name, key) in [(dropdown.itemText(i), dropdown.itemData(i)) for i in range(dropdown.count())] + [('none', b'\x00' * 32)]:
            options.SetKey('options_ratings_panel_template_service_key', key)
            second = RatingsPanel(gui, options)
            second.show()
            for _ in range(3):
                QW.QApplication.processEvents()
            example_key = second._example_star_service.GetServiceKey()

            def look(state, rating):
                (star_type, stars) = R.GetStars(example_key, state, rating)
                service = controller.services_manager.GetService(example_key)
                return {'shape': int(star_type.GetShape()), 'groups': [{'count': c, 'pen': colour(pe), 'brush': colour(b)} for (c, pe, b) in stars], 'num_stars': service.GetNumStars(), 'pad': service.GetCustomPad()}

            reopened.append({'saved': name, 'shown': second._service_template_dropdown.currentText(), 'null': look(ClientRatings.NULL, 0.0), 'rated': look(ClientRatings.SET, 0.6)})
            second.hide()
            second.deleteLater()
        out['reopened'] = reopened
        options.SetKey('options_ratings_panel_template_service_key', dropdown.itemData(0))

        boxes = [
            ('media viewer size', panel._media_viewer_rating_icon_size_px, panel._media_viewer_star_example),
            ('media viewer height', panel._media_viewer_rating_incdec_height_px, panel._media_viewer_incdec_example),
            ('preview size', panel._preview_window_rating_icon_size_px, panel._preview_window_star_example),
            ('preview height', panel._preview_window_rating_incdec_height_px, panel._preview_window_incdec_example),
            ('thumbnail size', panel._draw_thumbnail_rating_icon_size_px, panel._thumbnail_star_example),
            ('thumbnail height', panel._draw_thumbnail_rating_incdec_height_px, panel._thumbnail_incdec_example),
            ('dialog size', panel._dialog_rating_icon_size_px, panel._dialog_star_example),
            ('dialog height', panel._dialog_rating_incdec_height_px, panel._dialog_incdec_example),
        ]
        sizes = []
        for (name, spin, example) in boxes:
            entry = {'box': name, 'minimum': spin.minimum(), 'maximum': spin.maximum(), 'decimals': spin.decimals(), 'initial': spin.value(), 'values': []}
            for value in VALUES:
                spin.setValue(value)
                spin.editingFinished.emit()
                for _ in range(2):
                    QW.QApplication.processEvents()
                size = example._icon_size
                entry['values'].append({'typed': value, 'value': spin.value(), 'example': [size.width(), size.height()]})
            sizes.append(entry)
        out['sizes'] = sizes
        # what is saved from the boxes
        panel.UpdateOptions()
        out['saved_floats'] = {k: options.GetFloat(k) for k in ('media_viewer_rating_icon_size_px', 'media_viewer_rating_incdec_height_px', 'preview_window_rating_icon_size_px', 'preview_window_rating_incdec_height_px', 'draw_thumbnail_rating_icon_size_px', 'thumbnail_rating_incdec_height_px', 'dialog_rating_icon_size_px', 'dialog_rating_incdec_height_px')}
        panel.hide()
        panel.deleteLater()
        return out

    return controller.CallBlockingToQt(gui, work)


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
        json.dump(result, f, indent=1)
        f.write('\n')
    print('wrote', OUT)


if __name__ == '__main__':
    main()

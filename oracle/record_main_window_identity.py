#!/usr/bin/env python3
"""Real Qt raw-name Apply/Cancel and viewer tag-search activation requests.

The GUI panel and viewer hover list are real. The new-page publication is
delivered synchronously to the real main-window NewPageQuery consumer so the
fixture can distinguish the first-only batch flag and the active-window gate.
Activation wrappers delegate to Qt. All files are synthetic basic-fixture
JPEGs; temporary options and detached tag mappings are restored on shutdown.
"""
import json
import os
import sys
import tempfile
from pathlib import Path

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))


def record(session):
    def qt():
        from qtpy import QtCore as QC, QtGui as QG, QtWidgets as QW
        from hydrus.core import HydrusConstants as HC
        from hydrus.client import ClientConstants as CC, ClientLocation
        from hydrus.client.gui import ClientGUIFunctions
        from hydrus.client.gui.canvas import ClientGUICanvas, ClientGUICanvasFrame
        from hydrus.client.gui.panels.options import GUIPanel
        from hydrus.client.media import ClientMediaManagers
        from hydrus.client.search import ClientSearchPredicate

        c = session.controller
        gui = c.gui
        options = c.new_options
        key = 'activate_window_on_tag_search_page_activation'
        old_name = options.GetString('app_display_name')
        old_activate = options.GetBoolean(key)
        child = QW.QWidget(None)
        child.setWindowTitle('synthetic child title')
        child.show()
        names = []
        frames = []
        original_pub = c.pub
        original_activate = gui.activateWindow
        requests = []
        publications = []

        def activate():
            requests.append('main')
            original_activate()

        def publish(topic, *args, **kwargs):
            if topic == 'new_page_query':
                publications.append({
                    'activation': kwargs.get('activate_window', False),
                    'name': kwargs.get('page_name'),
                    'predicates': [p.GetValue() for p in kwargs['initial_predicates']],
                    'location': args[0].GetSerialisableTuple(),
                    'main_active_before': gui.isActiveWindow(),
                })
                return gui.NewPageQuery(*args, **kwargs)
            return original_pub(topic, *args, **kwargs)

        def panel():
            return GUIPanel.GUIPanel(gui)

        initial = panel()
        defaults = {'name': initial._app_display_name.text(),
                    'activate': initial._activate_window_on_tag_search_page_activation.isChecked(),
                    'enabled': initial._activate_window_on_tag_search_page_activation.isEnabled()}
        initial.deleteLater()
        try:
            for raw, typed, apply in [('', None, False), ('', None, True),
                                      ('  ', None, True), ('original', 'night library', False),
                                      ('original', 'night library', True)]:
                options.SetString('app_display_name', raw)
                p = panel()
                displayed = p._app_display_name.text()
                if typed is not None:
                    p._app_display_name.setText(typed)
                if apply:
                    p.UpdateOptions()
                    ClientGUIFunctions.UpdateAppDisplayName()
                reopened = panel()
                names.append({'raw': raw, 'typed': typed, 'apply': apply,
                              'displayed': displayed, 'saved': options.GetString('app_display_name'),
                              'reopened': reopened._app_display_name.text(),
                              'application_display_name': QW.QApplication.applicationDisplayName(),
                              'child_window_title': child.windowTitle(),
                              'child_native_title': child.windowHandle().title()})
                reopened.deleteLater()
                p.deleteLater()
            manifest = json.loads((HERE / 'fixtures/legacy_db/basic.manifest.json').read_text())
            h = next(bytes.fromhex(f['hash']) for f in manifest['files'] if f['name'] == 'jpeg_00.jpg')
            original_media = c.Read('media_results', [h])[0]
            media = original_media.Duplicate()
            tags = {'series:canonical_under_score', 'creator:synthetic person'}
            mapping = {CC.DEFAULT_LOCAL_TAG_SERVICE_KEY: {HC.CONTENT_STATUS_CURRENT: tags}}
            media.SetTagsManager(ClientMediaManagers.TagsManager(mapping, mapping))
            location = ClientLocation.LocationContext.STATICCreateSimple(CC.LOCAL_FILE_SERVICE_KEY)
            frame = ClientGUICanvasFrame.CanvasFrame(gui)
            frames.append(frame)
            canvas = ClientGUICanvas.CanvasMediaListBrowser(frame, os.urandom(32), location, [original_media], h)
            frame.SetCanvas(canvas)
            frame.showNormal()
            frame.resize(1000, 750)
            QW.QApplication.processEvents()
            hover = canvas._tags_hover
            hover.show()
            hover._tags.SetTagsByMediaResults([media])
            hover._tags._SetVirtualSize()
            QW.QApplication.processEvents()
            c.pub = publish
            gui.activateWindow = activate
            cases = []
            predicates = [ClientSearchPredicate.Predicate(ClientSearchPredicate.PREDICATE_TYPE_TAG, tag)
                          for tag in sorted(tags)]
            for enabled, active, batch in [(False, False, False), (True, False, False),
                                           (True, True, False), (True, False, True)]:
                p = panel()
                p._activate_window_on_tag_search_page_activation.setChecked(enabled)
                before = options.GetBoolean(key)
                p.UpdateOptions()
                reopened = panel()
                QW.QApplication.setActiveWindow(gui if active else frame)
                requests.clear()
                publications.clear()
                hover._tags._NewSearchPages([[p] for p in predicates] if batch else [predicates])
                cases.append({'enabled': enabled, 'active': active, 'batch': batch,
                              'before': before, 'saved': options.GetBoolean(key),
                              'reopened': reopened._activate_window_on_tag_search_page_activation.isChecked(),
                              'publications': list(publications), 'requests': list(requests)})
                p.deleteLater()
                reopened.deleteLater()
            # Deliver a real middle-button press through the hover event filter.
            options.SetBoolean(key, True)
            QW.QApplication.setActiveWindow(frame)
            requests.clear()
            publications.clear()
            e = QG.QMouseEvent(QC.QEvent.Type.MouseButtonPress, QC.QPointF(5, 5), QC.QPointF(5, 5),
                              QC.Qt.MouseButton.MiddleButton, QC.Qt.MouseButton.MiddleButton,
                              QC.Qt.KeyboardModifier.NoModifier)
            handled = hover._tags.eventFilter(hover._tags.widget(), e)
            middle = {'handled': handled, 'accepted': e.isAccepted(),
                      'publications': list(publications), 'requests': list(requests)}
            p = panel()
            p._activate_window_on_tag_search_page_activation.setChecked(False)
            p.deleteLater()
            reopened = panel()
            cancelled = {'saved': options.GetBoolean(key),
                         'reopened': reopened._activate_window_on_tag_search_page_activation.isChecked()}
            reopened.deleteLater()
            return {'defaults': defaults, 'names': names, 'activation': cases,
                    'middle_press': middle, 'cancelled_activation': cancelled,
                    'version': HC.SOFTWARE_VERSION}
        finally:
            c.pub = original_pub
            del gui.activateWindow
            options.SetString('app_display_name', old_name)
            options.SetBoolean(key, old_activate)
            ClientGUIFunctions.UpdateAppDisplayName()
            for frame in frames:
                frame.close()
            child.hide()
            child.deleteLater()
            QW.QApplication.sendPostedEvents(None, QC.QEvent.Type.DeferredDelete)
    return session.controller.CallBlockingToQt(session.controller.gui, qt)


def main():
    import hydrus_driver
    if len(sys.argv) > 1:
        import record_api
        Path(sys.argv[2]).write_text(json.dumps(hydrus_driver.run_client(record_api.unpack_fixture('basic'), record)))
        return
    with tempfile.TemporaryDirectory() as folder:
        out = Path(folder) / 'result.json'
        hydrus_driver.run_in_subprocess(str(Path(__file__).resolve()), '--child', str(out))
        result = json.loads(out.read_text())
    (HERE / 'fixtures/main_window_identity.json').write_text(json.dumps(result, indent=2) + '\n')
    print('wrote main_window_identity.json')


if __name__ == '__main__':
    main()

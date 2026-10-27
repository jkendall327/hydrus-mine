#!/usr/bin/env python3
"""Record a middle press on a search page's collect control
(`MediaCollectControl.eventFilter`).

A real page over the basic fixture's files in "my files", its collect set
to the series namespace with unmatched files left single and a tag context
of its own: a real middle press (`QTest.mousePress`) on the collect combo,
then the collect, the control's label and the page's media count; then the
same with unmatched files collected; and a left press, which only opens
the choices.

Usage: QT_QPA_PLATFORM=offscreen ~/pyenv/bin/python oracle/record_collect_middle_click.py
(writes fixtures/collect_middle_click.json)
"""

import json
import sys
import tempfile
from pathlib import Path

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))

OUT = HERE / 'fixtures' / 'collect_middle_click.json'


def record(session):
    def drive():
        from qtpy import QtCore as QC, QtWidgets as QW, QtTest as QT
        from hydrus.client import ClientConstants as CC, ClientLocation
        from hydrus.client.media import ClientMediaCollect
        from hydrus.client.search import ClientSearchTagContext
        c = session.controller
        gui = c.gui

        def settle():
            QT.QTest.qWait(50)
            QW.QApplication.processEvents()

        def context(tc):
            return dict(service=tc.service_key.hex(), include_current=tc.include_current_tags, include_pending=tc.include_pending_tags)

        def cf(collect):
            return dict(namespaces=list(collect.namespaces), ratings=[k.hex() for k in collect.rating_service_keys],
                        unmatched=collect.collect_unmatched, tag_context=context(collect.tag_context))

        manifest = json.loads((HERE / 'fixtures/legacy_db/basic.manifest.json').read_text())
        results = c.Read('media_results', [bytes.fromhex(f['hash']) for f in manifest['files']])
        results = [m for m in results if CC.LOCAL_FILE_SERVICE_KEY in m.GetLocationsManager().GetCurrent()]
        hashes = [m.GetHash() for m in results]
        location = ClientLocation.LocationContext.STATICCreateSimple(CC.LOCAL_FILE_SERVICE_KEY)
        page = gui.NewPageQuery(location, initial_hashes=hashes, page_name='collect middle click', select_page=True)
        for _ in range(40):
            settle()
            if len(page.GetMediaResultsPanel().GetFlatMedia()) == len(hashes):
                break
        panel = page.GetMediaResultsPanel()
        collect = page.GetSidebar()._media_collect_widget
        combo = collect._collect_comboctrl
        independent = ClientSearchTagContext.TagContext(include_current_tags=False, include_pending_tags=False)

        def state():
            return dict(collect=cf(collect.GetValue()), label=combo.text() if hasattr(combo, 'text') else str(collect._collect_comboctrl.GetValues()[2]),
                        description=collect._collect_comboctrl.GetValues()[2], media=len(panel._sorted_media))

        def press(button):
            QT.QTest.mousePress(combo, button, QC.Qt.KeyboardModifier.NoModifier, combo.rect().center())
            QT.QTest.mouseRelease(combo, button, QC.Qt.KeyboardModifier.NoModifier, combo.rect().center())
            settle()
            settle()

        cases = []
        for unmatched in (False, True):
            collect.SetCollect(ClientMediaCollect.MediaCollect(namespaces=['series'], collect_unmatched=unmatched, tag_context=independent.Duplicate()))
            settle()
            before = state()
            press(QC.Qt.MouseButton.MiddleButton)
            cases.append(dict(button='middle', before=before, after=state()))
        collect.SetCollect(ClientMediaCollect.MediaCollect(namespaces=['series'], collect_unmatched=False, tag_context=independent.Duplicate()))
        settle()
        before = state()
        press(QC.Qt.MouseButton.LeftButton)
        after = state()
        # (a left press opens the choices: shut them)
        QW.QApplication.processEvents()
        for popup in QW.QApplication.topLevelWidgets():
            if popup.isVisible() and popup.windowType() == QC.Qt.WindowType.Popup:
                popup.hide()
        cases.append(dict(button='left', before=before, after=after))
        gui._notebook.CloseCurrentPage(polite=False)
        settle()
        return dict(files=len(hashes), cases=cases)
    return session.controller.CallBlockingToQt(session.controller.gui, drive)


def main():
    import hydrus_driver
    import record_api
    if len(sys.argv) > 1 and sys.argv[1] == '--child':
        output = sys.argv[2]
        result = hydrus_driver.run_client(record_api.unpack_fixture('basic'), record)
        Path(output).write_text(json.dumps(result))
        return
    with tempfile.TemporaryDirectory() as work:
        out = Path(work) / 'collect.json'
        hydrus_driver.run_in_subprocess(str(Path(__file__).resolve()), '--child', str(out))
        result = json.loads(out.read_text())
    OUT.write_text(json.dumps(result, indent=1) + '\n')
    print('wrote', OUT)


if __name__ == '__main__':
    main()

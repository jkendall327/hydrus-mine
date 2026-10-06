#!/usr/bin/env python3
"""Record Database > how boned am I? on the basic client at a held time: the
files tab's table cells in grid order, its earliest-import line, the views and
duplicates tabs' lines, and whether Mr. Bones speaks, for the default search
and for a single local domain (which also counts its deleted files)."""
import json
import os
import sys
import tempfile
import time

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
NOW = 1_780_000_000


def record(session):
    from hydrus.core import HydrusTime
    HydrusTime.GetNow = lambda: NOW
    controller = session.controller
    out = {"now": NOW, "utc_offset_seconds": -time.timezone if not time.daylight else -time.altzone}

    def open_panel():
        from hydrus.client.gui import ClientGUITopLevelWindowsPanels
        from hydrus.client.gui.panels import ClientGUIScrolledPanelsReview
        frame = ClientGUITopLevelWindowsPanels.FrameThatTakesScrollablePanel(controller.gui, "review your fate")
        panel = ClientGUIScrolledPanelsReview.ReviewHowBonedAmI(frame)
        frame.SetPanel(panel)
        return frame, panel

    frame, panel = controller.CallBlockingToQt(controller.gui, open_panel)

    def read():
        from qtpy import QtWidgets as QW
        layout = panel._files_content_panel.layout()
        cells = []
        earliest = None
        if layout is not None:
            for i in range(layout.count()):
                item = layout.itemAt(i)
                inner = item.layout()
                if inner is not None and isinstance(inner, QW.QGridLayout):
                    for r in range(inner.rowCount()):
                        row = []
                        for c in range(inner.columnCount()):
                            w = inner.itemAtPosition(r, c)
                            w = w.widget() if w is not None else None
                            row.append(w.text() if isinstance(w, QW.QLabel) else "")
                        cells.append(row)
                elif item.widget() is not None and isinstance(item.widget(), QW.QLabel):
                    text = item.widget().text()
                    if text.startswith("Earliest"):
                        earliest = text
                    else:
                        cells.append([text])
        return dict(
            files=cells, earliest=earliest,
            views=[panel._media_views_st.text(), panel._preview_views_st.text()],
            duplicates=[panel._duplicates_st.text(), panel._alternates_st.text()],
            bones_text=panel._mr_bones_text.text() if not panel._mr_bones_text.isHidden() else None,
            loading=panel._loading_text.text(),
        )

    time.sleep(3)
    out["default"] = controller.CallBlockingToQt(controller.gui, read)

    def single_domain():
        from hydrus.client import ClientConstants as CC, ClientLocation
        panel._tag_autocomplete.SetLocationContext(ClientLocation.LocationContext.STATICCreateSimple(CC.LOCAL_FILE_SERVICE_KEY))

    controller.CallBlockingToQt(controller.gui, single_domain)
    time.sleep(3)
    out["my_files"] = controller.CallBlockingToQt(controller.gui, read)
    controller.CallBlockingToQt(controller.gui, frame.close)
    return out


def main():
    import hydrus_driver
    import record_api
    if len(sys.argv) > 1:
        output = sys.argv[2]
        value = hydrus_driver.run_client(record_api.unpack_fixture("basic"), record)
        with open(output, "w") as stream:
            json.dump(value, stream)
        return
    with tempfile.TemporaryDirectory() as directory:
        output = os.path.join(directory, "recording.json")
        hydrus_driver.run_in_subprocess(os.path.abspath(__file__), "--child", output)
        with open(output) as stream:
            value = json.load(stream)
    output = os.path.join(HERE, "fixtures/how_boned.json")
    with open(output, "w") as stream:
        json.dump(value, stream, indent=2)
        stream.write("\n")
    print("wrote " + output)


if __name__ == "__main__":
    main()

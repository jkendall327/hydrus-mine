#!/usr/bin/env python3
"""Record actual once-per-boot session size warnings and exact popup text.

Drives _UpdateMenuPagesCountIfDirty on real Qt menu items with scripted page
counts, avoiding millions of temporary files/seeds. Includes threshold, disabled,
closed-page exclusion, weighted seeds, repeated updates and a fresh boot latch.
"""
import json
import sys
import tempfile
from pathlib import Path
HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))


def record(session):
    from hydrus.core import HydrusData
    controller, gui = session.controller, session.controller.gui
    old_show = HydrusData.ShowText
    old_counts = gui.GetTotalPageCounts
    old_dirty = gui._pages_count_dirty
    old_shown = gui._have_shown_session_size_warning
    old_enabled = controller.new_options.GetBoolean('show_session_size_warnings')

    def drive():
        rows = []
        gui._have_shown_session_size_warning = False
        for hashes, seeds, enabled, new_boot in [(9999999,0,True,False),(10000000,0,True,False),(10000001,0,False,False),(1,500000,True,False),(20000000,0,True,False),(0,500001,True,True)]:
            messages = []
            HydrusData.ShowText = lambda message: messages.append(message)
            gui.GetTotalPageCounts = lambda: (2,hashes,seeds,5,90000000,90000000)
            if new_boot:
                gui._have_shown_session_size_warning = False
            gui._pages_count_dirty = True
            controller.new_options.SetBoolean('show_session_size_warnings', enabled)
            gui._UpdateMenuPagesCountIfDirty()
            rows.append({'hashes':hashes,'seeds':seeds,'enabled':enabled,'new_boot':new_boot,'messages':messages,'shown':gui._have_shown_session_size_warning})
        return {'default_enabled':old_enabled,'steps':rows}

    try:
        return controller.CallBlockingToQt(gui, drive)
    finally:
        HydrusData.ShowText = old_show
        gui.GetTotalPageCounts = old_counts
        gui._pages_count_dirty = old_dirty
        gui._have_shown_session_size_warning = old_shown
        controller.new_options.SetBoolean('show_session_size_warnings',old_enabled)


def main():
    import hydrus_driver
    import record_api
    if len(sys.argv)>1 and sys.argv[1]=='--child':
        destination = Path(sys.argv[2])
        result = hydrus_driver.run_client(record_api.unpack_fixture('basic'),record)
        destination.write_text(json.dumps(result))
        return
    with tempfile.TemporaryDirectory() as work:
        out=Path(work)/'warning.json'
        hydrus_driver.run_in_subprocess(str(Path(__file__).resolve()),'--child',str(out))
        result=json.loads(out.read_text())
    (HERE/'fixtures/session_warning.json').write_text(json.dumps(result,indent=2)+'\n')

if __name__=='__main__':main()

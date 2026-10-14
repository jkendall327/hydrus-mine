#!/usr/bin/env python3
"""Record the reference's rule preview pair lists (`PreviewPanel`'s two `ThumbnailPairList`s).

The real preview panel is made on the database `record_auto_resolution.py` left, with
the first suggested rule, and filled by its own search and test work. For each list
(pairs that will be actioned, pairs that will be skipped) the recording notes the
pairs in it and its selection mode, then drives it with real mouse clicks (click, ctrl-click,
shift-click) and its context menu: which rows are selected, the menu's entries, and the files
and location a chosen entry sends to a new page.
"""
import json
import os
import sys
import tempfile
HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)


def record(session):
    c = session.controller
    out = {}
    for name in ('pixel-perfect gifs vs pngs', 'pixel-perfect pairs'):
        out[name] = record_rule(c, name)
    return out


def record_rule(c, rule_name):

    def make():
        from qtpy import QtCore as QC, QtWidgets as QW
        from hydrus.client.duplicates import ClientDuplicatesAutoResolution as AR
        from hydrus.client.gui.duplicates import ClientGUIDuplicatesAutoResolutionRulePreview as PV
        rule = [r for r in c.duplicates_auto_resolution_manager.GetRules() if r.GetName() == rule_name][0]
        panel = PV.PreviewPanel(c.gui, lambda: rule)
        panel.resize(900, 1200)
        panel.show()
        panel.PageShown()
        return panel

    panel = c.CallBlockingToQt(c.gui, make)

    def state():
        return c.CallBlockingToQt(c.gui, lambda: (panel._pass_pairs_list.model().rowCount(), panel._fail_pairs_list.model().rowCount(), len(panel._fetched_pairs), len(panel._fetched_pairs_still_to_test), panel._search_results_label.text()))

    import time
    deadline = time.time() + 120
    last = None
    stable = 0
    while time.time() < deadline:
        time.sleep(0.5)
        now = state()
        stable = stable + 1 if now == last and now[2] > 0 and now[3] == 0 and now[0] + now[1] == now[2] else 0
        last = now
        if stable >= 4:
            break
    else:
        raise Exception('the preview never finished: %r' % (last,))

    def work():
        from qtpy import QtCore as QC, QtTest as QT
        from hydrus.client.gui import ClientGUICore as CGC
        from hydrus.client.gui.media import ClientGUIMediaSimpleActions as A
        out = {}
        menus = []
        shown = []
        core = CGC.core()
        core.PopupMenu = lambda parent, menu: menus.append(menu)
        A.ShowFilesInNewPage = lambda hashes, location_context, *a, **k: shown.append(dict(hashes=[h.hex() for h in hashes], location=sorted(k.hex() for k in location_context.current_service_keys)))
        for name, view in (('pass', panel._pass_pairs_list), ('fail', panel._fail_pairs_list)):
            model = view.model()
            result = dict(selection_mode=view.selectionMode().name, rows=[[a.GetHash().hex(), b.GetHash().hex()] for (a, b) in (model.GetMediaResultPair(r) for r in range(model.rowCount()))], steps=[])
            result['sizes'] = {m.GetHash().hex(): m.GetSize() for r in range(model.rowCount()) for m in model.GetMediaResultPair(r)}
            view.show()
            QC.QCoreApplication.processEvents()

            def click(row, modifier):
                index = model.index(row, 2 if model.columnCount() > 2 else 0)
                rect = view.visualRect(index)
                QT.QTest.mouseClick(view.viewport(), QC.Qt.MouseButton.LeftButton, modifier, rect.center())

            def snapshot(label):
                menus.clear()
                shown.clear()
                view.ShowMenuFromSignal(QC.QPoint(5, 5))
                entry = dict(step=label, selected=sorted(i.row() for i in view.selectionModel().selectedRows()))
                entry['menu'] = [[a.text() for a in m.actions()] for m in menus]
                if menus:
                    menus[0].actions()[0].trigger()
                entry['shown'] = list(shown)
                result['steps'].append(entry)

            n = model.rowCount()
            snapshot('nothing selected')
            click(0, QC.Qt.KeyboardModifier.NoModifier)
            snapshot('first row clicked')
            if n > 1:
                click(1, QC.Qt.KeyboardModifier.ControlModifier)
                snapshot('second row ctrl-clicked')
            if n > 2:
                click(2, QC.Qt.KeyboardModifier.ShiftModifier)
                snapshot('third row shift-clicked')
            view.clearSelection()
            out[name] = result
        return out
    result = c.CallBlockingToQt(c.gui, work)
    c.CallBlockingToQt(c.gui, panel.deleteLater)
    return result


def main():
    import hydrus_driver
    if len(sys.argv) > 1 and sys.argv[1] == '--child':
        import record_api
        output = sys.argv[2]
        value = hydrus_driver.run_client(record_api.unpack_fixture('auto_resolution'), record)
        with open(output, 'w') as f:
            json.dump(value, f)
        return
    with tempfile.TemporaryDirectory() as tmp:
        path = os.path.join(tmp, 'result.json')
        hydrus_driver.run_in_subprocess(os.path.abspath(__file__), '--child', path)
        with open(path) as f:
            value = json.load(f)
    with open(os.path.join(HERE, 'fixtures', 'preview_pair_lists.json'), 'w') as f:
        json.dump(value, f, indent=1, ensure_ascii=False)
        f.write('\n')
    print('recorded the preview pair lists')


if __name__ == '__main__':
    main()

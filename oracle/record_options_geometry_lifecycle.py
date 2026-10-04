#!/usr/bin/env python3
"""Record actual DialogManage Options geometry ordering on Apply/Cancel/X.

The live ManageOptionsPanel owns all real Options pages. Real modal-state
DialogManage buttons and close event drive NewDialog's acceptance lifecycle;
no close/save hook is replaced. The GUI page's real frame table resets itself.
"""
import json
import os
import sys
import tempfile
HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
OUT = os.path.join(HERE, 'fixtures', 'options_geometry_lifecycle.json')

def record(session):
    def work():
        from qtpy import QtWidgets as QW
        from hydrus.client.gui import ClientGUITopLevelWindowsPanels as W
        from hydrus.client.gui.panels.options.ClientGUIManageOptionsPanel import ManageOptionsPanel
        from hydrus.client.gui.panels.options.GUIPanel import GUIPanel
        controller = session.controller
        original = controller.new_options.Duplicate()
        key = 'manage_options_dialog'
        baseline = (True, True, (800, 600), (20, 20), (-1, -1), 'topleft', False, False)
        cases = []
        try:
            for action in ('cancel', 'window_close', 'apply_unchanged', 'apply_reset_self'):
                controller.new_options.SetFrameLocation(key, *baseline)
                with W.DialogManage(controller.gui, 'manage options', key) as dialog:
                    panel = ManageOptionsPanel(dialog)
                    dialog.SetPanel(panel)
                    dialog.setModal(True)
                    dialog.show()
                    dialog.resize(912, 678)
                    dialog.move(35, 45)
                    QW.QApplication.processEvents()
                    before = controller.new_options.GetFrameLocation(key)
                    if action == 'apply_reset_self':
                        gui = next(page for page in panel._listbook.GetPages() if isinstance(page, GUIPanel))
                        row = next(row for row in gui._frame_locations.GetData() if row[0] == key)
                        gui._frame_locations.SelectDatas([row], deselect_others=True)
                        gui._ResetLastSize()
                        gui._ResetLastPosition()
                    actual = {'size': [dialog.width(), dialog.height()], 'position': [dialog.frameGeometry().x(), dialog.frameGeometry().y()]}
                    if action == 'cancel':
                        dialog._cancel.click()
                    elif action == 'window_close':
                        dialog.close()
                    else:
                        dialog._apply.click()
                    cases.append({'action': action, 'before': before, 'actual_window': actual,
                                  'after': controller.new_options.GetFrameLocation(key),
                                  'result': dialog.result(), 'visible': dialog.isVisible()})
        finally:
            controller.new_options = original
        return {'frame_key': key, 'cases': cases}
    return session.controller.CallBlockingToQt(session.controller.gui, work)

def main():
    import hydrus_driver
    if len(sys.argv)>1 and sys.argv[1]=='--child':
        import record_api
        output = sys.argv[2]
        value=hydrus_driver.run_client(record_api.unpack_fixture('basic'), record)
        with open(output, 'w') as stream:
            json.dump(value,stream)
        return
    with tempfile.TemporaryDirectory() as directory:
        path=os.path.join(directory,'out.json')
        hydrus_driver.run_in_subprocess(os.path.abspath(__file__),'--child',path)
        with open(path) as stream:
            value=json.load(stream)
    with open(OUT,'w') as stream:
        json.dump(value,stream,indent=2)
        stream.write('\n')
    print('wrote',OUT)
if __name__=='__main__':
    main()

#!/usr/bin/env python3
"""Record the real GUI options frame table and owned geometry child.

Drive table selection, batch flip/reset, actual EditFrameLocations accepted and
rejected child dialogs, parent update/reopen and discarded parent drafts. Capture
optional spin-pair bounds, gravity/position choices and state. Only modal answers
are scripted; real GUIPanel/EditFrameLocationPanel and UpdateOptions run.
"""
import json
import os
import sys
import tempfile
HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)


def record(session):
    c = session.controller
    def drive():
        from qtpy import QtWidgets as QW
        from hydrus.client.gui.panels.options import GUIPanel as O
        from hydrus.client.gui.panels import ClientGUIScrolledPanelsEdit as E
        from hydrus.client.gui import ClientGUITopLevelWindowsPanels as W
        original = c.new_options.Duplicate()
        panels = []
        def panel():
            p = O.GUIPanel(c.gui); panels.append(p); return p
        def snapshot(p):
            data = sorted(p._frame_locations.GetData())
            return {'rows':data, 'cells':[p._GetPrettyFrameLocationInfo(row) for row in data], 'selected':[row[0] for row in p._frame_locations.GetData(only_selected=True)]}
        def select(p, names):
            p._frame_locations.SelectDatas([r for r in p._frame_locations.GetData() if r[0] in names], deselect_others=True)
        events = []
        edits = []
        p = panel()
        def stage(action):
            getattr(p, action)(); events.append({'action':action,'state':snapshot(p)})
        try:
            events.append({'action':'initial','state':snapshot(p)})
            select(p, ['main_gui','media_viewer'])
            for action in ('_FlipRememberSize','_FlipRememberPosition','_ResetLastSize','_ResetLastPosition'):
                stage(action)
            cancel_reopen = snapshot(panel())
            p.UpdateOptions()
            applied_reopen = snapshot(panel())
            scripted = [False, True]
            class Dialog(W.DialogEdit):
                def exec(self):
                    child = self._panel
                    before = child.GetValue()
                    child._remember_size.setChecked(True)
                    child._remember_position.setChecked(True)
                    child._last_size.SetValue((912,678))
                    child._last_position.SetValue((-45,67))
                    child._default_gravity_x.SetValue(1)
                    child._default_gravity_y.SetValue(-1)
                    child._default_position.SetValue('center')
                    child._maximised.setChecked(False)
                    child._fullscreen.setChecked(False)
                    yes = scripted.pop(0)
                    edits.append({'title':self.windowTitle(),'before':before,'edited':child.GetValue(),'accepted':yes})
                    return QW.QDialog.DialogCode.Accepted if yes else QW.QDialog.DialogCode.Rejected
            original_dialog = W.DialogEdit
            W.DialogEdit = Dialog
            try:
                select(p,['main_gui'])
                for _ in range(2):
                    p.EditFrameLocations(); edits[-1]['state'] = snapshot(p)
            finally:
                W.DialogEdit = original_dialog
            p.UpdateOptions()
            reopened = panel()
            final_reopen = snapshot(reopened)
            info = next(row for row in reopened._frame_locations.GetData() if row[0]=='main_gui')
            child = E.EditFrameLocationPanel(c.gui, info); panels.append(child)
            bounds = []
            for size, position in [(None,None),((1,2000000),(-2000000,2000000)),((701,503),(0,-17))]:
                child._last_size.SetValue(size); child._last_position.SetValue(position)
                bounds.append(child.GetValue())
            child._last_size.SetValue(None)
            child._last_size._checkbox.setChecked(False)
            restored_optional = child.GetValue()
            child.resize(720,540); child.show(); child.grab().save(os.path.join(HERE,'fixtures/frame_locations_child.png'))
            positions = []
            for pos in ('topleft','center','mouse'):
                child._default_position.SetValue(pos); positions.append(child.GetValue())
            return {'events':events,'cancel_reopen':cancel_reopen,'applied_reopen':applied_reopen,'edits':edits,'final_reopen':final_reopen,'bounds':bounds,'restored_optional':restored_optional,'positions':positions}
        finally:
            c.new_options = original
            for p in panels: p.deleteLater()
    return c.CallBlockingToQt(c.gui, drive)


def main():
    import hydrus_driver
    if len(sys.argv)>1 and sys.argv[1]=='--child':
        output=sys.argv[2]
        import record_api
        result=hydrus_driver.run_client(record_api.unpack_fixture('basic'),record)
        with open(output,'w') as f:json.dump(result,f,ensure_ascii=False,indent=2)
        return
    with tempfile.TemporaryDirectory() as work:
        output=os.path.join(work,'result.json')
        hydrus_driver.run_in_subprocess(os.path.abspath(__file__),'--child',output)
        with open(output) as f:result=json.load(f)
        with open(os.path.join(HERE,'fixtures/frame_locations.json'),'w') as f:json.dump(result,f,ensure_ascii=False,indent=2)

if __name__=='__main__':main()

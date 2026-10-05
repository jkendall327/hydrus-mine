#!/usr/bin/env python3
"""Record actual GUI rescue options and GetSafePosition decisions.

Options staging, bounds, serialization, reopening and cancellation use real Qt
panels. GetSafePosition is never replaced. Alongside the real offscreen screen,
only QApplication's screen topology is supplied for deterministic disconnected,
negative-coordinate and partial-intersection cases; those are environment seams,
not claims of native OS/taskbar coverage. No reference media/database is changed.
"""
import json, sys, tempfile
from pathlib import Path
HERE=Path(__file__).resolve().parent
sys.path.insert(0,str(HERE))

def record(session):
    def drive():
        from qtpy import QtCore as QC, QtWidgets as QW
        from hydrus.core import HydrusSerialisable
        from hydrus.client.gui import ClientGUITopLevelWindows as W
        from hydrus.client.gui.panels.options.GUIPanel import GUIPanel
        c=session.controller; original=c.new_options.Duplicate(); panels=[]
        def settings(options):
            return dict(disabled=options.GetBoolean('disable_get_safe_position_test'),
                        add_padding=options.GetBoolean('fuzzy_relocate_on_get_safe_position_test'),
                        padding=options.GetInteger('forgive_frame_gubbins_fuzzy_padding'))
        def read(p):return dict(disabled=p._disable_get_safe_position_test.isChecked(),add_padding=p._fuzzy_relocate_on_get_safe_position_test.isChecked(),padding=p._forgive_frame_gubbins_fuzzy_padding.value())
        stages=[]; cases=[]
        try:
            defaults=settings(c.new_options)
            for disabled,padded,padding in [(False,True,40),(True,False,-5),(False,False,101),(False,True,0),(False,True,10)]:
                p=GUIPanel(c.gui);panels.append(p); before=settings(c.new_options)
                p._disable_get_safe_position_test.setChecked(disabled);p._fuzzy_relocate_on_get_safe_position_test.setChecked(padded);p._forgive_frame_gubbins_fuzzy_padding.setValue(padding)
                draft=read(p);staged=settings(c.new_options);p.UpdateOptions()
                encoded=HydrusSerialisable.CreateFromSerialisableTuple(c.new_options.GetSerialisableTuple())
                reopened=GUIPanel(c.gui);panels.append(reopened)
                stages.append(dict(input=[disabled,padded,padding],before=before,draft=draft,staged=staged,saved=settings(c.new_options),round_trip=settings(encoded),reopened=read(reopened)))
            p=GUIPanel(c.gui);before=settings(c.new_options);p._disable_get_safe_position_test.setChecked(True);p._forgive_frame_gubbins_fuzzy_padding.setValue(93);cancel_draft=read(p);p.deleteLater();cancel_after=settings(c.new_options)
            def probe(label,position,size):
                point,message=W.GetSafePosition(QC.QPoint(*position),'manage_options_dialog',window_size=None if size is None else QC.QSize(*size))
                return dict(environment=label,settings=settings(c.new_options),position=position,size=size,result=None if point is None else [point.x(),point.y()],rescued=message is not None,message=message)
            actual_screens=[dict(geometry=[s.geometry().x(),s.geometry().y(),s.geometry().width(),s.geometry().height()],available=[s.availableGeometry().x(),s.availableGeometry().y(),s.availableGeometry().width(),s.availableGeometry().height()]) for s in QW.QApplication.screens()]
            for point in [[0,0],[-99999,-99999]]:cases.append(probe('actual_offscreen',point,[300,200]))
            topology=[dict(geometry=[0,0,800,600],available=[0,24,800,576]),dict(geometry=[-1000,100,800,600],available=[-1000,124,800,576])]
            class Screen:
                def __init__(self,v):self.value=v
                def geometry(self):return QC.QRect(*self.value['geometry'])
                def availableGeometry(self):return QC.QRect(*self.value['available'])
            screens=[Screen(v) for v in topology]
            old_at=QW.QApplication.screenAt;old_screens=QW.QApplication.screens
            QW.QApplication.screenAt=staticmethod(lambda point:next((s for s in screens if s.geometry().contains(point)),None));QW.QApplication.screens=staticmethod(lambda:screens)
            try:
                for disabled,padded,padding in [(False,True,40),(False,False,40),(False,True,0),(False,True,100),(True,True,40)]:
                    c.new_options.SetBoolean('disable_get_safe_position_test',disabled);c.new_options.SetBoolean('fuzzy_relocate_on_get_safe_position_test',padded);c.new_options.SetInteger('forgive_frame_gubbins_fuzzy_padding',padding)
                    for position,size in [([-30,-30],[100,100]),([-41,-41],[100,100]),([-41,100],[100,100]),([100,-41],[100,100]),([900,900],[100,100]),([-1200,200],[500,200]),([760,560],[100,100]),([-99999,-99999],None),([800,600],[0,0])]:cases.append(probe('synthetic_topology',position,size))
            finally:QW.QApplication.screenAt=old_at;QW.QApplication.screens=old_screens
            return dict(defaults=defaults,stages=stages,cancel_before=before,cancel_draft=cancel_draft,cancel_after=cancel_after,actual_screens=actual_screens,topology=topology,cases=cases)
        finally:
            c.new_options=original
            for p in panels:p.deleteLater()
    return session.controller.CallBlockingToQt(session.controller.gui,drive)

def main():
    import hydrus_driver,record_api
    if len(sys.argv)>1 and sys.argv[1]=='--child':
        Path(sys.argv[2]).write_text(json.dumps(hydrus_driver.run_client(record_api.unpack_fixture('basic'),record)));return
    with tempfile.TemporaryDirectory() as d:
        output=Path(d)/'result.json';hydrus_driver.run_in_subprocess(str(Path(__file__).resolve()),'--child',str(output));result=json.loads(output.read_text())
    (HERE/'fixtures/window_rescue.json').write_text(json.dumps(result,indent=2)+'\n')
    print('wrote window_rescue.json')
if __name__=='__main__':main()

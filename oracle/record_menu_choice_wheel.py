#!/usr/bin/env python3
"""Actual saved MenuChoiceButton wheel policy and represented sorting consumers.

The unchanged Qt handler runs every choice case; actual wheel dispatch also
runs live TagSortControl and MediaSortControl producers. GUIPanel staging,
Cancel/UpdateOptions/reopen and legacy false persistence are recorded.
The offscreen bubbling probe explicitly delivers an ignored child event to the
unchanged real parent viewport; it does not attest automatic window propagation.
"""
import json
import sys
import tempfile
from pathlib import Path
HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
KEY = 'menu_choice_buttons_can_mouse_scroll'


def record(session):
    def work():
        from qtpy import QtCore as C, QtGui as G, QtWidgets as W
        from hydrus.client import ClientConstants as CC, ClientOptions
        from hydrus.client.gui import ClientGUITagSorting
        from hydrus.client.gui.pages import ClientGUIMediaResultsPanelSortCollect as Media
        from hydrus.client.gui.panels.options.GUIPanel import GUIPanel
        from hydrus.client.gui.widgets.ClientGUIMenuButton import MenuChoiceButton
        from hydrus.client.metadata import ClientTagSorting as Tags
        controller = session.controller
        old = controller.new_options.GetBoolean(KEY)
        out = dict(default=ClientOptions.ClientOptions().GetBoolean(KEY), loaded=old,
                   label='Mouse wheel can "scroll" through menu buttons: ', options=[], cases=[], tags=[], media=[])
        def wheel(button, dx, dy, dispatch=False):
            event = G.QWheelEvent(C.QPointF(8, 8), C.QPointF(button.mapToGlobal(C.QPoint(8, 8))),
                                 C.QPoint(), C.QPoint(dx, dy), C.Qt.MouseButton.NoButton,
                                 C.Qt.KeyboardModifier.NoModifier, C.Qt.ScrollPhase.NoScrollPhase, False)
            event.setAccepted(False)
            if dispatch: W.QApplication.sendEvent(button, event)
            else: button.wheelEvent(event)
            return event.isAccepted()
        windows = []
        try:
            # GUIPanel holds the ordinary duplicate options draft.
            draft = controller.new_options.Duplicate()
            panel = GUIPanel(controller.gui)
            panel._new_options = draft
            windows.append(panel)
            out['options'].append(dict(case='opened', checked=panel._menu_choice_buttons_can_mouse_scroll.isChecked(), saved=controller.new_options.GetBoolean(KEY)))
            panel._menu_choice_buttons_can_mouse_scroll.setChecked(not old)
            out['options'].append(dict(case='edited', checked=panel._menu_choice_buttons_can_mouse_scroll.isChecked(), saved=controller.new_options.GetBoolean(KEY)))
            reopened = GUIPanel(controller.gui); windows.append(reopened)
            out['options'].append(dict(case='cancel_reopened', checked=reopened._menu_choice_buttons_can_mouse_scroll.isChecked(), saved=controller.new_options.GetBoolean(KEY)))
            panel.UpdateOptions()
            out['options'].append(dict(case='draft_applied', checked=draft.GetBoolean(KEY), saved=controller.new_options.GetBoolean(KEY)))
            controller.new_options.SetBoolean(KEY, draft.GetBoolean(KEY))
            reopened = GUIPanel(controller.gui); windows.append(reopened)
            out['options'].append(dict(case='applied_reopened', checked=reopened._menu_choice_buttons_can_mouse_scroll.isChecked(), saved=controller.new_options.GetBoolean(KEY)))
            reopened.resize(950, 850); reopened.show(); W.QApplication.processEvents()
            reopened.grab().save(str(HERE/'fixtures/menu-choice-wheel-qt-options.png'))
            controller.new_options.SetBoolean(KEY,False)
            out['legacy'] = controller.new_options.GetSerialisableTuple()
            button = MenuChoiceButton(controller.gui,[('first',10),('second',20),('third',30)]);windows.append(button)
            button.show(); W.QApplication.processEvents()
            changed = [];button.valueChanged.connect(lambda:changed.append(button.GetValue()))
            def case(name,enabled,dx,dy):
                controller.new_options.SetBoolean(KEY,enabled)
                before = button.GetValue();count=len(changed)
                accepted=wheel(button,dx,dy)
                out['cases'].append(dict(name=name,enabled=enabled,dx=dx,dy=dy,before=before,after=button.GetValue(),choices=button.GetChoiceTuples(),emissions=len(changed)-count,accepted=accepted,label=button.text()))
            for name,enabled,dx,dy in [('up-wrap',True,0,120),('down-wrap',True,0,-120),('small-up',True,0,1),('large-down',True,0,-960),('horizontal',True,120,0),('zero-delta',True,0,0),('live-disable',False,0,120),('live-enable',True,0,120)]:case(name,enabled,dx,dy)
            button.SetChoiceTuples([('only',99)]);case('single-emits',True,0,120)
            button.SetChoiceTuples([]);case('empty-accepts',True,0,-120);case('empty-disabled',False,0,-120)
            tag=ClientGUITagSorting.TagSortControl(controller.gui,Tags.TagSort.STATICGetTextASCUserGroupedDefault(),show_siblings=True)
            windows.append(tag);tag.show();W.QApplication.processEvents()
            def tag_state():
                value=tag.GetValue()
                return dict(type=value.sort_type,order=value.sort_order,siblings=value.use_siblings,group=value.group_by)
            for field,enabled,dy in [('_sort_type',True,120),('_sort_order_count',True,-120),('_sort_type',True,-120),('_sort_order_text',False,-120),('_sort_order_text',True,-120),('_group_by',True,-120),('_use_siblings',True,-120),('_sort_type',True,-120),('_sort_type',True,-120),('_sort_type',True,-120)]:
                controller.new_options.SetBoolean(KEY,enabled);before=tag_state();accepted=wheel(getattr(tag,field),0,dy,True)
                out['tags'].append(dict(field=field,enabled=enabled,dy=dy,before=before,after=tag_state(),accepted=accepted))
            media=Media.MediaSortControl(controller.gui);windows.append(media);media.show();W.QApplication.processEvents()
            for enabled,dy in [(True,120),(False,-120),(True,-120)]:
                controller.new_options.SetBoolean(KEY,enabled);before=media.GetSort().sort_order;accepted=wheel(media._sort_order_choice,0,dy,True)
                out['media'].append(dict(enabled=enabled,dy=dy,before=before,after=media.GetSort().sort_order,choices=media._sort_order_choice.GetChoiceTuples(),accepted=accepted))
            # The actual viewport consumer is probed with the disclosed offscreen adapter.
            scroll = W.QScrollArea(controller.gui); scroll.setWindowFlag(C.Qt.WindowType.Window,True); windows.append(scroll)
            inner = W.QWidget(); layout = W.QVBoxLayout(inner)
            nested = MenuChoiceButton(inner,[('first',10),('second',20)])
            layout.addWidget(nested); spacer = W.QWidget(inner);spacer.setMinimumHeight(900);layout.addWidget(spacer)
            scroll.setWidget(inner);scroll.setWidgetResizable(True);scroll.resize(300,220);scroll.show();W.QApplication.processEvents()
            out['bubbling'] = []
            for enabled in [False,True]:
                controller.new_options.SetBoolean(KEY,enabled);scroll.verticalScrollBar().setValue(0);W.QApplication.processEvents()
                before = scroll.verticalScrollBar().value()
                position = C.QPointF(nested.mapTo(scroll,C.QPoint(8,8)))
                event = G.QWheelEvent(position,C.QPointF(nested.mapToGlobal(C.QPoint(8,8))),C.QPoint(),C.QPoint(0,-120),C.Qt.MouseButton.NoButton,C.Qt.KeyboardModifier.NoModifier,C.Qt.ScrollPhase.NoScrollPhase,False)
                event.setAccepted(False)
                W.QApplication.sendEvent(scroll.windowHandle(),event);W.QApplication.processEvents()
                accepted = event.isAccepted()
                if not enabled:
                    # This serialized offscreen window transport does not deliver
                    # ignored child events to the scroll viewport automatically.
                    # Route that ignored event to the unchanged real parent consumer.
                    parent_event = G.QWheelEvent(C.QPointF(8,8),C.QPointF(scroll.viewport().mapToGlobal(C.QPoint(8,8))),C.QPoint(),C.QPoint(0,-120),C.Qt.MouseButton.NoButton,C.Qt.KeyboardModifier.NoModifier,C.Qt.ScrollPhase.NoScrollPhase,False)
                    W.QApplication.sendEvent(scroll.viewport(),parent_event)
                out['bubbling'].append(dict(enabled=enabled,before=before,after=scroll.verticalScrollBar().value(),maximum=scroll.verticalScrollBar().maximum(),accepted=accepted,value=nested.GetValue(),transport='actual widget-window delivery plus explicit ignored-event parent viewport delivery under offscreen oracle'))
            tag.resize(700,60); W.QApplication.processEvents()
            tag.grab().save(str(HERE/'fixtures/menu-choice-wheel-qt-tags.png'))
        finally:
            controller.new_options.SetBoolean(KEY,old)
            for window in windows:window.hide();window.deleteLater()
        return out
    return session.controller.CallBlockingToQt(session.controller.gui,work)


def main():
    import hydrus_driver
    if len(sys.argv)>1:
        import record_api
        Path(sys.argv[2]).write_text(json.dumps(hydrus_driver.run_client(record_api.unpack_fixture('basic'),record)))
        return
    with tempfile.TemporaryDirectory() as folder:
        result=Path(folder)/'result.json'
        hydrus_driver.run_in_subprocess(str(Path(__file__).resolve()),'--child',str(result))
        value=json.loads(result.read_text())
    (HERE/'fixtures/menu_choice_wheel.json').write_text(json.dumps(value,indent=2,ensure_ascii=False)+'\n')
    print('wrote menu_choice_wheel.json')
if __name__=='__main__':main()

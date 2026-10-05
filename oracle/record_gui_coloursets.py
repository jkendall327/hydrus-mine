#!/usr/bin/env python3
"""Record real ColoursPanel, picker acceptance/Cancel, and live colour consumers.

The existing Qt thumbnail page, autocomplete, tags list and preview canvas read
all thirteen roles. Real Help FlipDarkmode warns without override but still
switches the colourset; it leaves the application stylesheet unchanged. Colour
picker dialogs receive real QColorDialog values/acceptance, not replacement
colour business logic. Runs only against a copied basic fixture.
"""
import json
import sys
import tempfile
import time
from pathlib import Path
HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))


def record(session):
    c = session.controller
    def build():
        from hydrus.client import ClientConstants as CC, ClientLocation
        media = c.Read('media_results_from_ids', [1, 2, 3])
        return c.gui._notebook.NewPageQuery(ClientLocation.LocationContext.STATICCreateSimple(CC.LOCAL_FILE_SERVICE_KEY), page_name='synthetic colour consumer examples', initial_hashes=[m.GetHash() for m in media])
    page = c.CallBlockingToQt(c.gui, build)
    for _ in range(400):
        if c.CallBlockingToQt(c.gui, lambda: page._initialised):
            break
        time.sleep(.025)
    else:
        raise RuntimeError('real thumbnail page did not initialize')
    def drive():
        from qtpy import QtCore as QC, QtGui as QG, QtWidgets as QW
        from hydrus.client import ClientConstants as CC
        from hydrus.client.gui import ClientGUIDialogsMessage
        from hydrus.client.gui.panels.options.ColoursPanel import ColoursPanel
        from hydrus.core import HydrusSerialisable
        roles = [CC.COLOUR_THUMB_BACKGROUND, CC.COLOUR_THUMB_BACKGROUND_SELECTED, CC.COLOUR_THUMB_BACKGROUND_REMOTE, CC.COLOUR_THUMB_BACKGROUND_REMOTE_SELECTED, CC.COLOUR_THUMB_BORDER, CC.COLOUR_THUMB_BORDER_SELECTED, CC.COLOUR_THUMB_BORDER_REMOTE, CC.COLOUR_THUMB_BORDER_REMOTE_SELECTED, CC.COLOUR_THUMBGRID_BACKGROUND, CC.COLOUR_AUTOCOMPLETE_BACKGROUND, CC.COLOUR_MEDIA_BACKGROUND, CC.COLOUR_MEDIA_TEXT, CC.COLOUR_TAGS_BOX]
        original = c.new_options.Duplicate()
        old_warning = ClientGUIDialogsMessage.ShowWarning
        old_critical = ClientGUIDialogsMessage.ShowCritical
        old_clipboard = QW.QApplication.clipboard().text()
        warnings = []
        panels = []
        stylesheet = QW.QApplication.instance().styleSheet()
        def rgb(colour):
            return list(colour.getRgb())[:3]
        def values(options=None):
            options = c.new_options if options is None else options
            return dict(override=options.GetBoolean('override_stylesheet_colours'), current=options.GetString('current_colourset'), sets={name:[rgb(options.GetColour(role,name)) for role in roles] for name in ('default','darkmode')})
        def panel():
            p = ColoursPanel(c.gui); panels.append(p)
            return p
        def displayed(p):
            return dict(override=p._override_stylesheet_colours.isChecked(), current=p._current_colourset.GetValue(), enabled=p._coloursets_panel.isEnabled(), selected_tab=p._notebook.currentIndex(), sets={name:[rgb(p._gui_colours[name][role].GetColour()) for role in roles] for name in ('default','darkmode')})
        def consumers():
            owner = page.GetMediaResultsPanel(); sidebar = page.GetSidebar()
            colours = []
            for role in roles:
                if role <= CC.COLOUR_THUMBGRID_BACKGROUND:
                    colour = owner.GetColour(role)
                elif role == CC.COLOUR_AUTOCOMPLETE_BACKGROUND:
                    colour = sidebar._tag_autocomplete.GetColour(role)
                elif role in (CC.COLOUR_MEDIA_BACKGROUND, CC.COLOUR_MEDIA_TEXT):
                    colour = page._preview_canvas.GetColour(role)
                else:
                    colour = sidebar._current_selection_tags_list._GetBackgroundColour()
                colours.append(rgb(colour))
            return dict(colours=colours, stylesheet_unchanged=QW.QApplication.instance().styleSheet()==stylesheet)
        try:
            p = panel();initial = values();initial_displayed = displayed(p)
            labels = [x.text() for x in p.findChildren(QW.QLabel) if x.text() and not x.property('exclude_from_search')]
            choices = [p._current_colourset.itemText(i) for i in range(p._current_colourset.count())]
            tabs = [p._notebook.tabText(i) for i in range(p._notebook.count())]
            # Every role has a distinct colour in both sets, through real buttons.
            staged = []
            p._override_stylesheet_colours.setChecked(True);p._UpdateOverride()
            for set_index,name in enumerate(('default','darkmode')):
                for index,role in enumerate(roles):
                    colour = [17+index*11,39+set_index*81,203-index*9]
                    p._gui_colours[name][role].SetColour(QG.QColor(*colour))
                    staged.append(dict(set=name,role=role,colour=colour))
            p._current_colourset.SetValue('darkmode')
            before = values();draft = displayed(p);p.UpdateOptions()
            saved = values();encoded = HydrusSerialisable.CreateFromString(c.new_options.DumpToString())
            reopened = displayed(panel())
            cancelled = panel();cancelled._override_stylesheet_colours.setChecked(False);cancelled._UpdateOverride();cancelled._current_colourset.SetValue('default')
            cancel_displayed = displayed(cancelled);cancel_before = values();cancelled.deleteLater();panels.remove(cancelled);cancel_after = values()
            # Actual modal picker acceptance and rejection preserve original buttons.
            picker = p._gui_colours['default'][roles[0]];pickers=[]
            for accepted,colour in ((True,[19,83,197]),(False,[231,67,11])):
                before_picker = rgb(picker.GetColour())
                def answer(accepted=accepted,colour=colour):
                    dialog = QW.QApplication.activeModalWidget()
                    if not isinstance(dialog,QW.QColorDialog):raise RuntimeError('expected real QColorDialog')
                    dialog.setCurrentColor(QG.QColor(*colour))
                    pickers.append(dict(accepted=accepted,selected=rgb(dialog.currentColor()),before=before_picker,title=dialog.windowTitle(),labels=[label.text() for label in dialog.findChildren(QW.QLabel)]))
                    if accepted:
                        QW.QApplication.processEvents();dialog.grab().save(str(HERE/'fixtures/gui_colour_picker_qt.png'))
                    if accepted:dialog.accept()
                    else:dialog.reject()
                QC.QTimer.singleShot(0,answer);picker.click();pickers[-1]['after']=rgb(picker.GetColour())
            clipboard_cases=[];critical=[]
            ClientGUIDialogsMessage.ShowCritical=lambda parent,title,message,*args,**kwargs:critical.append(dict(title=title,message=message,parent_is_picker=parent is picker))
            for text in ('#FF0050',' 12:34:56 ','bad','#1234567',''):
                before_clipboard=rgb(picker.GetColour());QW.QApplication.clipboard().setText(text)
                picker._ImportHexFromClipboard()
                clipboard_cases.append(dict(text=text,before=before_clipboard,after=rgb(picker.GetColour()),critical=critical[-1] if critical and len(critical)>sum(bool(case['critical']) for case in clipboard_cases) else None))
            picker.SetColour(QG.QColor(19,83,197));p.UpdateOptions()
            from hydrus.client.gui import ClientGUICore as CGC
            popup=CGC.core().PopupMenu;menu=[]
            try:
                CGC.core().PopupMenu=lambda parent,value:menu.extend(action.text() for action in value.actions())
                picker.ShowMenuFromSignal(QC.QPoint())
            finally:CGC.core().PopupMenu=popup
            events=[]
            ClientGUIDialogsMessage.ShowWarning=lambda parent,message,*args,**kwargs:warnings.append(dict(message=message,parent_is_main=parent is c.gui))
            for override in (False,True):
                c.new_options.SetBoolean('override_stylesheet_colours',override)
                for current in ('default','darkmode'):
                    c.new_options.SetString('current_colourset',current)
                    c.gui.FlipDarkmode();QW.QApplication.processEvents()
                    events.append(dict(action='help darkmode',before=dict(override=override,current=current),saved=values(),consumers=consumers(),warning_count=len(warnings)))
            p = panel();p.resize(850,600);p.show();QW.QApplication.processEvents();p.grab().save(str(HERE/'fixtures/gui_coloursets_qt.png'));p.hide()
            return dict(role_codes=roles,initial=initial,initial_displayed=initial_displayed,labels=labels,choices=choices,tabs=tabs,before_update=before,draft=draft,saved=saved,round_trip=values(encoded),reopened=reopened,cancel_displayed=cancel_displayed,cancel_before=cancel_before,cancel_after=cancel_after,role_edits=staged,pickers=pickers,clipboard=clipboard_cases,colour_menu=menu,events=events,warnings=warnings)
        finally:
            ClientGUIDialogsMessage.ShowWarning=old_warning
            ClientGUIDialogsMessage.ShowCritical=old_critical
            QW.QApplication.clipboard().setText(old_clipboard)
            for p in panels:p.hide();p.deleteLater()
            c.new_options=original
    return c.CallBlockingToQt(c.gui,drive)


def main():
    import hydrus_driver,record_api
    if len(sys.argv)>1:
        output=sys.argv[2]
        result=hydrus_driver.run_client(record_api.unpack_fixture('basic'),record)
        Path(output).write_text(json.dumps(result));return
    with tempfile.TemporaryDirectory() as tmp:
        out=Path(tmp)/'result.json'
        hydrus_driver.run_in_subprocess(str(Path(__file__).resolve()),'--child',str(out))
        result=json.loads(out.read_text())
    destination=HERE/'fixtures/gui_coloursets.json'
    destination.write_text(json.dumps(result,indent=2,ensure_ascii=False)+'\n');print(f'wrote {destination}')
if __name__=='__main__':main()

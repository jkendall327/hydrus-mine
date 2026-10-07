#!/usr/bin/env python3
"""Record actual namespace Add blank validation and Delete question widgets.

No Quick.EnterText/GetYesNo/ShowWarning or business handler replacements.
QtTest pointer/key events operate actual buttons/QLineEdit. Timers observe and
acknowledge real modal dialogs. A copied basic fixture boots the real controller;
only private panel draft edits occur. Parent panel uses a standalone Qt window
for capture visibility, without changing its handlers or child parent relation.
"""
import sys, json, hashlib, subprocess, traceback
from pathlib import Path
REPO=Path(__file__).resolve().parents[1]
OUT=REPO/'oracle/fixtures'
sys.path[:0]=[str(REPO/'oracle'),str(REPO)]

def record(session):
    from qtpy import QtWidgets as QW, QtCore as QC, QtTest as QT
    from hydrus.core import HydrusConstants as HC
    from hydrus.client.gui.panels.options import TagPresentationPanel as Module
    from hydrus.client.gui import ClientGUITopLevelWindowsPanels as Top, QtPorting as QP
    c=session.controller
    def drive():
        original=c.new_options; old_colours=dict(HC.options['namespace_colours'])
        draft=original.Duplicate();c.new_options=draft
        results={'add_cases':[],'delete_cases':[],'errors':[],'captures':[]}
        panel=None
        def rows():
            return [dict(namespace=t.GetNamespace(),rgb=list(t.GetNamespaceAndColour()[1])) for t in panel._namespace_colours._ordered_terms]
        def capture(widget,name):
            QW.QApplication.processEvents()
            path=OUT/name
            assert widget.grab().save(str(path))
            results['captures'].append(dict(path=name,width=widget.width(),height=widget.height(),sha256=hashlib.sha256(path.read_bytes()).hexdigest()))
        def click(widget):
            assert widget.isVisible() and widget.isEnabled(),type(widget).__name__
            QT.QTest.mouseClick(widget,QC.Qt.MouseButton.LeftButton,pos=widget.rect().center())
        def safe(callback):
            def run():
                try:callback()
                except Exception:
                    results['errors'].append(traceback.format_exc())
                    modal=QW.QApplication.activeModalWidget()
                    if modal is not None:modal.reject()
            return run
        def later(callback,delay=0):QC.QTimer.singleShot(delay,safe(callback))
        def warning(event,parent,filename):
            dialog=QW.QApplication.activeModalWidget()
            assert isinstance(dialog,QW.QMessageBox),type(dialog).__name__
            event['warning']={'title':dialog.windowTitle(),'message':dialog.text(),'button_labels':[b.text() for b in dialog.buttons()],'ok_only':dialog.standardButtons()==QW.QMessageBox.StandardButton.Ok,'parent_is_expected':dialog.parentWidget() is parent,'parent_class':type(dialog.parentWidget()).__name__}
            capture(dialog,filename)
            click(dialog.button(QW.QMessageBox.StandardButton.Ok))
        def add_case(route,valid,whitespace=False):
            event={'route':route,'input':'   ' if whitespace else '', 'rows_before':rows()}
            results['add_cases'].append(event)
            def answer():
                dlg=QW.QApplication.activeModalWidget()
                assert isinstance(dlg,Top.DialogEdit),type(dlg).__name__
                entry=dlg._panel._text
                event.update(dialog_class=type(dlg).__name__,panel_class=type(dlg._panel).__name__,title=dlg.windowTitle(),message_labels=[w.text() for w in dlg.findChildren(QW.QLabel)],allow_blank=dlg._panel._allow_blank,allow_whitespace=dlg._panel._allow_whitespace,parent_is_namespace_panel=dlg.parentWidget() is panel)
                assert entry.text()==''
                capture(dlg,'namespace-add-'+route+'-qt.png')
                entry.setFocus(QC.Qt.FocusReason.OtherFocusReason)
                dlg.activateWindow();QW.QApplication.processEvents()
                event['focus_is_entry']=QW.QApplication.focusWidget() is entry
                if whitespace:
                    QT.QTest.keyClicks(entry,'   ')
                    event['entered_text']=entry.text()
                    later(lambda:warning(event,panel,'namespace-add-whitespace-warning-qt.png'))
                    click(dlg._apply)
                    return
                later(lambda:warning(event,dlg,'namespace-add-empty-'+route+'-warning-qt.png'))
                if route=='apply':click(dlg._apply)
                else:QT.QTest.keyClick(entry,QC.Qt.Key.Key_Return)
                event['retained_after_warning']={'same_active_dialog':QW.QApplication.activeModalWidget() is dlg,'dialog_visible':dlg.isVisible(),'text':entry.text()}
                assert event['retained_after_warning']==dict(same_active_dialog=True,dialog_visible=True,text='')
                capture(dlg,'namespace-add-empty-'+route+'-retained-qt.png')
                QT.QTest.keyClicks(entry,valid)
                event['valid_entered_text']=entry.text()
                click(dlg._apply)
            later(answer)
            click(panel._add_namespace_colour)
            event['rows_after']=rows()
            event['durable_namespace_colours_unchanged']=HC.options['namespace_colours']==old_colours
            if not whitespace:assert valid.lower() in [r['namespace'] for r in rows()]
            else:assert event['rows_after']==event['rows_before']
            assert not results['errors'],results['errors']
        try:
            panel=Module.TagPresentationPanel(c.gui,draft)
            panel.setWindowFlag(QC.Qt.WindowType.Window,True)
            panel.resize(1000,900);panel.show();QW.QApplication.processEvents()
            results['initial']=rows()
            add_case('apply','Qt Apply Validation')
            add_case('return','Qt Return Validation')
            add_case('whitespace',None,True)
            selected=[None,'','qt apply validation']
            panel._namespace_colours._DeselectAll()
            for namespace in selected:
                index=next(i for i,t in enumerate(panel._namespace_colours._ordered_terms) if t.GetNamespace()==namespace)
                panel._namespace_colours._Hit(False,True,index)
            for yes in [False,True]:
                event={'selected':selected,'yes':yes,'rows_before':rows()};results['delete_cases'].append(event)
                def answer_delete():
                    dlg=QW.QApplication.activeModalWidget();assert isinstance(dlg,Top.DialogCustomButtonQuestion)
                    event.update(title=dlg.windowTitle(),dialog_class=type(dlg).__name__,panel_class=type(dlg._panel).__name__,message_labels=[w.text() for w in dlg.findChildren(QW.QLabel)],buttons=[w.text() for w in dlg.findChildren(QW.QPushButton)],parent_is_namespace_list=dlg.parentWidget() is panel._namespace_colours)
                    capture(dlg,'namespace-delete-'+('yes' if yes else 'no')+'-qt.png')
                    click(dlg._panel._yes if yes else dlg._panel._no)
                later(answer_delete)
                click(panel._delete_namespace_colour)
                event['rows_after']=rows()
                assert (event['rows_after']==event['rows_before']) is (not yes)
                assert None in [r['namespace'] for r in rows()] and '' in [r['namespace'] for r in rows()]
            results['durable_namespace_colours_unchanged']=HC.options['namespace_colours']==old_colours
            assert not results['errors'],results['errors']
            return results
        finally:
            if panel is not None:panel.close();panel.deleteLater()
            c.new_options=original;HC.options['namespace_colours']=old_colours
    return c.CallBlockingToQt(c.gui,drive)

if __name__=='__main__':
    import hydrus_driver,record_api
    result=hydrus_driver.run_client(record_api.unpack_fixture('basic'),record)
    source=subprocess.check_output(['git','-C',str(REPO),'rev-parse','HEAD'],text=True).strip()
    result.update(source_commit=source,script_sha256=hashlib.sha256(Path(__file__).read_bytes()).hexdigest(),limitations=['Offscreen Qt on copied basic fixture; parent TagPresentationPanel is presented as standalone for visibility, not full Options-window pixel parity.','Actual Add/Delete dialogs and QtTest Apply/Return/acknowledgement routes are unchanged; list mixed selection uses the real list _Hit helper, not physical row-pointer geometry.','No panel UpdateOptions or persistent settings mutation; namespace RGB values from real random handler are recorded, not fixed parity output.'])
    (OUT/'namespace_entry_validation.json').write_text(json.dumps(result,indent=2,ensure_ascii=False)+'\n')
    print('RECORDED',OUT/'namespace_entry_validation.json',flush=True)

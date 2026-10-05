#!/usr/bin/env python3
"""Record real Tag Presentation namespace add/delete and OR colour controls.

Actual Options handlers drive input normalization, warnings, protected mixed
selection, exact No/Yes deletion, private draft cancellation and saved reopen.
Warnings use actual QMessageBox.warning with a Qt timer recording its visible
message, title, parent and Ok-only buttons before acknowledging it. Random RGB
generation is scripted only to make accepted add output reproducible.
The real read autocomplete list resolves the OR header colour for each saved
namespace choice. No reference files, remote services or media files change.
"""
import json, os, sys, tempfile
HERE=os.path.dirname(os.path.abspath(__file__));sys.path.insert(0,HERE)
OUT=os.path.join(HERE,'fixtures','namespace_colour_controls.json')
def record(session):
    from qtpy import QtWidgets as QW, QtCore as QC
    from hydrus.core import HydrusConstants as HC, HydrusExceptions
    from hydrus.client.gui import ClientGUIDialogsQuick as Quick,ClientGUIDialogsMessage as Message
    from hydrus.client.gui.panels.options import TagPresentationPanel as Module
    from hydrus.client.gui.search import ClientGUIACDropdown as AC
    from hydrus.client.gui.lists import ClientGUIListBoxesData as Data
    from hydrus.client import ClientConstants as CC,ClientLocation
    from hydrus.client.search import ClientSearchPredicate as P,ClientSearchFileSearchContext as F
    c=session.controller
    def drive():
        original=c.new_options;old_colours=dict(HC.options['namespace_colours'])
        old_enter=Quick.EnterText;old_warning=Message.ShowWarning;old_yes=Quick.GetYesNo;old_random=Module.random.randint
        draft=original.Duplicate();c.new_options=draft
        answer={'text':'','yes':False,'cancel':False};questions=[];warnings=[];warning_dialogs=[];rgb=iter([12,34,56]*50)
        def enter(win,message,*args,**kwargs):
            questions.append(message)
            if answer['cancel']:raise HydrusExceptions.CancelledException()
            return answer['text']
        Quick.EnterText=enter
        Quick.GetYesNo=lambda win,message,*args,**kwargs:(questions.append(message) or (QW.QDialog.DialogCode.Accepted if answer['yes'] else QW.QDialog.DialogCode.Rejected))
        def warning(win,message,*args,**kwargs):
            warnings.append(message)
            def acknowledge():
                dialog=QW.QApplication.activeModalWidget()
                assert isinstance(dialog,QW.QMessageBox)
                warning_dialogs.append(dict(title=dialog.windowTitle(),message=dialog.text(),parent_is_panel=dialog.parent() is win,button_labels=[button.text() for button in dialog.buttons()],ok_only=dialog.standardButtons()==QW.QMessageBox.StandardButton.Ok))
                dialog.accept()
            QC.QTimer.singleShot(0,acknowledge)
            old_warning(win,message,*args,**kwargs)
        Message.ShowWarning=warning
        Module.random.randint=lambda low,high:next(rgb)
        def rows(panel):
            return [dict(namespace=t.GetNamespace(),label=t.GetCopyableTexts()[0],rgb=list(t.GetNamespaceAndColour()[1])) for t in panel._namespace_colours._ordered_terms]
        def values():
            return [dict(namespace=k,rgb=list(v)) for k,v in sorted(HC.options['namespace_colours'].items(),key=lambda item:'' if item[0] is None else item[0])]
        try:
            panel=Module.TagPresentationPanel(c.gui,draft);initial=rows(panel);events=[]
            for text,cancel in [('unused',True),('',False),(':',False),(' SYSTEM: ',False),(' -Parity Artists::: ',False),('parity artists',False),('::::',False),(' Artist:Inner: ',False),('\u001c:\u001c',False),('\u001c \u001f',False),('series:\u001c',False)]:
                answer.update(text=text,cancel=cancel);questions.clear();warnings.clear();warning_dialogs.clear();panel._AddNamespaceColour()
                events.append(dict(action='add',input=text,cancel=cancel,questions=list(questions),warnings=list(warnings),warning_dialogs=list(warning_dialogs),rows=rows(panel),durable=values(),delete_enabled=panel._delete_namespace_colour.isEnabled()))
            # A default and unnamespaced row are selected along with deletable rows.
            selected=[None,'','creator','parity artists']
            panel._namespace_colours._DeselectAll()
            for namespace in selected:
                index=next(i for i,t in enumerate(panel._namespace_colours._ordered_terms) if t.GetNamespace()==namespace)
                panel._namespace_colours._Hit(False,True,index)
            for yes in [False,True]:
                answer['yes']=yes;questions.clear();panel._DeleteNamespaceColour()
                events.append(dict(action='delete',selected=selected,yes=yes,questions=list(questions),rows=rows(panel),durable=values(),delete_enabled=panel._delete_namespace_colour.isEnabled()))
            for selected in [[None,''],[]]:
                panel._namespace_colours._DeselectAll()
                for namespace in selected:
                    index=next(i for i,t in enumerate(panel._namespace_colours._ordered_terms) if t.GetNamespace()==namespace)
                    panel._namespace_colours._Hit(False,True,index)
                questions.clear();panel._DeleteNamespaceColour()
                events.append(dict(action='delete',selected=selected,clear_selection=True,yes=False,questions=list(questions),rows=rows(panel),durable=values(),delete_enabled=panel._delete_namespace_colour.isEnabled()))
            panel.deleteLater()
            cancelled=Module.TagPresentationPanel(c.gui,draft);cancelled_rows=rows(cancelled);cancelled.deleteLater()
            panel=Module.TagPresentationPanel(c.gui,draft)
            answer.update(text=' Parity Artists::: ',cancel=False);panel._AddNamespaceColour();panel.UpdateOptions()
            saved=values();panel.deleteLater()
            reopened=Module.TagPresentationPanel(c.gui,draft);reopened_rows=rows(reopened)
            predicate=P.Predicate(P.PREDICATE_TYPE_OR_CONTAINER,[P.Predicate(P.PREDICATE_TYPE_TAG,'parity artists:alpha'),P.Predicate(P.PREDICATE_TYPE_TAG,'series:beta')])
            colour_cases=[]
            # Distinguish the untouched legacy None from an explicit empty field.
            draft.SetNoneableString('or_connector_custom_namespace_colour',None)
            for namespace in ['','character','missing namespace','','parity artists']:
                reopened._or_connector_custom_namespace_colour.setText(namespace);reopened.UpdateOptions()
                ac=AC.AutoCompleteDropdownTagsRead(c.gui,b'parity namespace colours',F.FileSearchContext(location_context=ClientLocation.LocationContext.STATICCreateSimple(CC.LOCAL_FILE_SERVICE_KEY)),synchronised=False)
                actual=ac._favourites_list._GetRowsOfTextsAndColours(Data.ListBoxItemPredicate(predicate))
                colour_cases.append(dict(input=namespace,saved=draft.GetNoneableString('or_connector_custom_namespace_colour'),rows=actual));ac.deleteLater()
            selection_cases=[]
            for mode in ['plain_then_add','shift_then_add','inverse_then_add']:
                selection_panel=Module.TagPresentationPanel(c.gui,draft)
                selection_events=[]
                def selection_snapshot(action,**extra):
                    selection_events.append(dict(action=action,rows=rows(selection_panel),selected=[t.GetNamespace() for t in selection_panel._namespace_colours._ordered_terms if t in selection_panel._namespace_colours._selected_terms],delete_enabled=selection_panel._delete_namespace_colour.isEnabled(),**extra))
                selection_snapshot('initial')
                def hit(namespace,shift=False,ctrl=False):
                    index=next(i for i,t in enumerate(selection_panel._namespace_colours._ordered_terms) if t.GetNamespace()==namespace)
                    selection_panel._namespace_colours._Hit(shift,ctrl,index);selection_snapshot('hit',namespace=namespace,shift=shift,ctrl=ctrl)
                hit('creator')
                if mode != 'plain_then_add':hit('system',True)
                if mode == 'inverse_then_add':hit('creator',True,True)
                answer.update(text='aaa parity',cancel=False);selection_panel._AddNamespaceColour();selection_snapshot('add',input='aaa parity')
                if mode == 'plain_then_add':hit('system',True)
                elif mode == 'shift_then_add':hit('parity artists',True)
                else:hit('parity artists',True,True)
                answer['yes']=True;selection_panel._DeleteNamespaceColour();selection_snapshot('delete')
                hit(None,True)
                selection_panel.deleteLater()
                selection_cases.append(dict(mode=mode,events=selection_events))
            labels=[label.text() for label in reopened.findChildren(QW.QLabel) if label.text()=='Namespace for the OR top row: ']
            reopened.deleteLater()
            return dict(initial=initial,events=events,cancelled_rows=cancelled_rows,saved=saved,reopened_rows=reopened_rows,colour_cases=colour_cases,selection_cases=selection_cases,labels=labels,random_rgb=[12,34,56])
        finally:
            c.new_options=original;HC.options['namespace_colours']=old_colours
            Quick.EnterText=old_enter;Quick.GetYesNo=old_yes;Message.ShowWarning=old_warning;Module.random.randint=old_random
    return c.CallBlockingToQt(c.gui,drive)
def child(out):
    import hydrus_driver,record_api
    result=hydrus_driver.run_client(record_api.unpack_fixture('basic'),record)
    with open(out,'w') as f:json.dump(result,f)
def main():
    if len(sys.argv)>1 and sys.argv[1]=='--child':child(sys.argv[2]);return
    import hydrus_driver
    with tempfile.TemporaryDirectory() as d:
        out=os.path.join(d,'out.json');hydrus_driver.run_in_subprocess(os.path.abspath(__file__),'--child',out)
        with open(out) as f:result=json.load(f)
    with open(OUT,'w') as f:json.dump(result,f,indent=2,ensure_ascii=False);f.write('\n')
    print('wrote',OUT)
if __name__=='__main__':main()

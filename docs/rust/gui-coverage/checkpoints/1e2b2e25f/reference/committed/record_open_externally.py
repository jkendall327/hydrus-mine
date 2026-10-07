#!/usr/bin/env python3
"""Drive actual Qt Open Externally rows, registered choosers and launch routing.

Business handlers, QueueListBox order/selection, DialogEdit and Options washing
are real. Choosers return scripted registered identities; real child dialogs are
answered by Qt timers. Final callable.Call captures typed inputs instead of
starting programs or contacting authored synthetic URLs. Includes real PNG.
"""
import json
import os
import sys
import tempfile
from pathlib import Path
HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))


def record(session):
    c = session.controller
    def qt():
        from qtpy import QtWidgets as QW, QtCore as QC
        from hydrus.core import HydrusConstants as HC, HydrusExceptions as E, HydrusSerialisable as S
        from hydrus.client.executables import ClientExecutableManager as M, ClientExecutableCallables as C, ClientExecutablePipelines as P, ClientExecutableActualCall as A
        from hydrus.client.gui.panels.options import OpenExternallyPanel as O
        from hydrus.client.gui import ClientGUIDialogsQuick as Q, ClientGUIDialogsMessage as Message
        from hydrus.client.gui.executables import ClientGUIExecutableActions as Actions
        options = c.new_options
        old_manager = c.executable_manager
        old_url = options.GetLaunchURLExecutableIdsAndNames()
        old_mimes = options.GetMimesToLaunchFileExecutableIdsAndNames()
        old_buttons, old_list, old_yes, old_info = Q.SelectFromListButtons, Q.SelectFromList, Q.GetYesNo, Message.ShowInformation
        calls = []
        for key, name, pipeline in [(1, 'Zulu URL', P.EXECUTABLE_PIPELINE_TYPE_OPEN_EXTERNALLY_SINGLE_URL), (2, 'alpha URL', P.EXECUTABLE_PIPELINE_TYPE_OPEN_EXTERNALLY_SINGLE_URL), (3, 'File one 日本', P.EXECUTABLE_PIPELINE_TYPE_OPEN_EXTERNALLY_SINGLE_FILE), (4, 'File two', P.EXECUTABLE_PIPELINE_TYPE_OPEN_EXTERNALLY_SINGLE_FILE)]:
            call = C.ClientExecutableCallable(name, pipeline, A.ExecutableLocalProcessCall('synthetic-program', ['%url%' if pipeline == P.EXECUTABLE_PIPELINE_TYPE_OPEN_EXTERNALLY_SINGLE_URL else '%path%']))
            call.SetCallableKey(bytes([key]) * 32)
            calls.append(call)
        os_url = C.ClientExecutableCallable('Default OS URL Launch', P.EXECUTABLE_PIPELINE_TYPE_OPEN_EXTERNALLY_SINGLE_URL, A.ExecutableLocalProcessDefaultLaunchURL())
        os_url.SetCallableKey(bytes([5])*32)
        os_file = C.ClientExecutableCallable('Default OS File Launch', P.EXECUTABLE_PIPELINE_TYPE_OPEN_EXTERNALLY_SINGLE_FILE, A.ExecutableLocalProcessDefaultLaunchFile())
        os_file.SetCallableKey(bytes([6])*32)
        calls += [os_url, os_file]
        manager = M.ExecutableManager(); manager.SetCallables(calls); c.executable_manager = manager
        options.SetLaunchURLExecutableIdsAndNames([os_url.GetIdAndName()])
        options.SetMimesToLaunchFileExecutableIdsAndNames({HC.GENERAL_FILE: [os_file.GetIdAndName()]})
        panel = O.OpenExternallyPanel(c.gui, lambda: manager)
        chooser = []; notices = []; answer = [None]; mime_answer = [HC.IMAGE_PNG]
        def names(values): return [dict(key=x.object_id.hex(), name=x.name) for x in values]
        def choose(owner, title, tuples, **kwargs):
            chooser.append(dict(title=title, choices=[dict(label=t[0], key=t[1].object_id.hex(), tooltip=t[2]) for t in tuples], kwargs=kwargs))
            if answer[0] is None: raise E.CancelledException()
            return next(t[1] for t in tuples if t[1].object_id == bytes([answer[0]])*32)
        def choose_mime(owner, title, tuples, **kwargs):
            chooser.append(dict(title=title, choices=[dict(label=t[0], mime=t[1]) for t in tuples], kwargs=kwargs))
            if mime_answer[0] is None: raise E.CancelledException()
            return mime_answer[0]
        Q.SelectFromListButtons=choose; Q.SelectFromList=choose_mime
        Message.ShowInformation=lambda owner,text: notices.append(text)
        Q.GetYesNo=lambda owner,text,**kwargs: notices.append(text) or QW.QDialog.DialogCode.Accepted
        def urls(): return names(panel._launch_url_executable_ids_and_names.GetData())
        def rows(): return [dict(mime=row[0], display=panel._ConvertMimeToDisplayTuple(row), calls=names(row[1])) for row in panel._mime_launch_listctrl.GetData()]
        events=[]
        try:
            events.append(dict(action='initial', urls=urls(), rows=rows()))
            for value in [None,1,2]:
                answer[0]=value; panel._launch_url_executable_ids_and_names._Add()
                events.append(dict(action='url-add', answer=value, urls=urls()))
            queue=panel._launch_url_executable_ids_and_names
            queue._listbox.item(0).setSelected(True); queue._Down()
            events.append(dict(action='url-down', urls=urls()))
            answer[0]=5; queue._Edit()  # existing OS identity excluded, all choices are now used
            events.append(dict(action='url-edit-exhausted', urls=urls()))
            queue._Delete(); events.append(dict(action='url-delete',urls=urls()))
            queue._listbox.item(0).setSelected(True); answer[0]=5; queue._Edit()
            events.append(dict(action='url-edit',urls=urls()))
            nested=[]
            def edit_dialog(accept, add_keys):
                def handle():
                    dlg=QW.QApplication.activeModalWidget(); child=dlg._panel
                    assert isinstance(child,O.EditOpenFileIdsAndNamesPanel)
                    q=child._ids_and_names
                    for key in add_keys: answer[0]=key;q._Add()
                    if q._listbox.count()>1:
                        q._listbox.item(0).setSelected(True);q._Down()
                    nested.append(dict(title=dlg.windowTitle(),before=names(child.GetValue()),accept=accept))
                    if accept:dlg.accept()
                    else:dlg.reject()
                QC.QTimer.singleShot(0,handle)
            edit_dialog(False,[3]);panel._AddMimeLaunch();events.append(dict(action='mime-add-cancel',rows=rows()))
            edit_dialog(True,[3,4]);panel._AddMimeLaunch();events.append(dict(action='mime-add',rows=rows()))
            selected=next(row for row in panel._mime_launch_listctrl.GetData() if row[0]==HC.IMAGE_PNG)
            panel._mime_launch_listctrl.SelectDatas([selected],deselect_others=True)
            edit_dialog(False,[6]);panel._EditMimeLaunch();events.append(dict(action='mime-edit-cancel',rows=rows()))
            edit_dialog(True,[6]);panel._EditMimeLaunch();events.append(dict(action='mime-edit',rows=rows()))
            general=next(row for row in panel._mime_launch_listctrl.GetData() if row[0]==HC.GENERAL_FILE)
            panel._mime_launch_listctrl.SelectDatas([general],deselect_others=True)
            protected=panel._GeneralFileIsNotSelected();panel._mime_launch_listctrl.ProcessDeleteAction()
            events.append(dict(action='general-delete',can_delete=protected,rows=rows()))
            png=next(row for row in panel._mime_launch_listctrl.GetData() if row[0]==HC.IMAGE_PNG)
            panel._mime_launch_listctrl.SelectDatas([png],deselect_others=True)
            panel._mime_launch_listctrl.ProcessDeleteAction();events.append(dict(action='mime-delete',rows=rows()))
            # Washing uses stable keys, updates names, removes missing/wrong-type entries,
            # deduplicates and inserts OS defaults only for an initially empty queue.
            calls[0].SetName('renamed URL');manager.SetCallables(calls)
            missing=S.IdAndName(bytes([9])*32,'missing')
            queue.SetData([S.IdAndName(bytes([1])*32,'old label'),calls[0].GetIdAndName(),missing,calls[2].GetIdAndName()])
            panel._mime_launch_listctrl.SetData([(HC.GENERAL_FILE,[]),(HC.GENERAL_IMAGE,[calls[2].GetIdAndName()]),(HC.IMAGE_PNG,[])])
            panel.UpdateOptions()
            washed=dict(urls=names(options.GetLaunchURLExecutableIdsAndNames()),rows={str(k):names(v) for k,v in options.GetMimesToLaunchFileExecutableIdsAndNames().items()})
            reopened=O.OpenExternallyPanel(c.gui,lambda:manager)
            reopen=dict(urls=names(reopened._launch_url_executable_ids_and_names.GetData()),rows=[reopened._ConvertMimeToDisplayTuple(row) for row in reopened._mime_launch_listctrl.GetData()])
            dispatch=[]; old_call=C.ClientExecutableCallable.Call
            C.ClientExecutableCallable.Call=lambda self,inputs:dispatch.append(dict(key=self.GetCallableKey().hex(),name=self.GetName(),inputs={str(k):v for k,v in inputs.items()}))
            try:
                url='https://example.net/authored/open#漢'
                Actions.OpenExternallyURLDefault(panel,url)
                manifest=json.loads((HERE/'fixtures/legacy_db/basic.manifest.json').read_text())
                h=next(bytes.fromhex(f['hash']) for f in manifest['files'] if f['name']=='jpeg_00.jpg')
                media=c.Read('media_results',[h])[0]
                Actions.OpenExternallySingleFileDefault(panel,media)
                options.SetMimesToLaunchFileExecutableIdsAndNames({HC.GENERAL_FILE:[calls[3].GetIdAndName()],HC.GENERAL_IMAGE:[calls[2].GetIdAndName()],HC.IMAGE_JPEG:[]})
                specific_empty=names(options.GetLaunchFileExecutableIdsAndNames(HC.IMAGE_JPEG))
                Actions.OpenExternallySingleFileDefault(panel,media)
                options.SetLaunchURLExecutableIdsAndNames([missing]);Actions.OpenExternallyURLDefault(panel,url)
                options.SetLaunchURLExecutableIdsAndNames([]);Actions.OpenExternallyURLDefault(panel,url)
            finally:C.ClientExecutableCallable.Call=old_call
            # Actual nested Edit replacement, exhausted choice and captured queue Delete.
            child=O.EditOpenFileIdsAndNamesPanel(panel,HC.IMAGE_PNG,[calls[2].GetIdAndName(),calls[3].GetIdAndName()],manager)
            q=child._ids_and_names; q._listbox.item(0).setSelected(True)
            checks=[dict(action='initial',values=names(child.GetValue()))]
            for label,key in [('edit-cancel',None),('edit-os',6),('edit-restore',3)]:
                answer[0]=key;q._Edit();checks.append(dict(action=label,values=names(child.GetValue())))
            q._Down();checks.append(dict(action='down',values=names(child.GetValue())))
            Q.GetYesNo=lambda owner,text,**kwargs:notices.append(text) or QW.QDialog.DialogCode.Rejected
            q._Delete();checks.append(dict(action='delete-cancel',values=names(child.GetValue())))
            Q.GetYesNo=lambda owner,text,**kwargs:notices.append(text) or QW.QDialog.DialogCode.Accepted
            q._Delete();checks.append(dict(action='delete',values=names(child.GetValue())))
            for key in [3,6]:answer[0]=key;q._Add()
            q._listbox.item(0).setSelected(True);q._Edit();checks.append(dict(action='edit-exhausted',values=names(child.GetValue())))
            multiple=[row for row in panel._mime_launch_listctrl.GetData() if row[0] in (HC.GENERAL_FILE,HC.IMAGE_PNG)]
            panel._mime_launch_listctrl.SelectDatas(multiple,deselect_others=True);multi_general_protected=not panel._GeneralFileIsNotSelected()
            child.deleteLater()
            panel.resize(950,800);panel.show();QW.QApplication.processEvents();panel.grab().save(str(HERE/'fixtures/open_externally.png'));panel.hide()
            return dict(nested_checks=checks,multi_general_protected=multi_general_protected,calls=[dict(key=x.GetCallableKey().hex(),name=x.GetName(),pipeline=x.GetPipelineType()) for x in calls],events=events,chooser=chooser,notices=notices,nested=nested,washed=washed,reopen=reopen,dispatch=dispatch,specific_empty=specific_empty,mime_choices=[dict(mime=m,human=HC.mime_string_lookup[m],label=HC.mime_mimetype_string_lookup[m]) for m in list(HC.GENERAL_CLASSES_OF_FILETYPE)+list(HC.SEARCHABLE_MIMES)])
        finally:
            Q.SelectFromListButtons,Q.SelectFromList,Q.GetYesNo,Message.ShowInformation=old_buttons,old_list,old_yes,old_info
            c.executable_manager=old_manager;options.SetLaunchURLExecutableIdsAndNames(old_url);options.SetMimesToLaunchFileExecutableIdsAndNames(old_mimes)
            panel.deleteLater()
    return c.CallBlockingToQt(c.gui,qt)


def main():
    import hydrus_driver
    if len(sys.argv)>1:
        import record_api
        output=sys.argv[2]
        value=hydrus_driver.run_client(record_api.unpack_fixture('basic'),record)
        Path(output).write_text(json.dumps(value))
        return
    with tempfile.TemporaryDirectory() as work:
        path=Path(work)/'record.json';hydrus_driver.run_in_subprocess(str(Path(__file__).resolve()),'--child',str(path));value=json.loads(path.read_text())
    (HERE/'fixtures/open_externally.json').write_text(json.dumps(value,indent=2,ensure_ascii=False)+'\n')
    print('wrote open_externally.json and PNG')
if __name__=='__main__':main()

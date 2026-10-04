#!/usr/bin/env python3
"""Record the real Options ImportOptionsPanel lists, stacks and staged mutations.

Uses synthetic URL classes and paused, local-only drafts. Drives actual Qt list
selection and clear/reset/edit buttons; intercepts only modal answers. Records
Apply/Cancel isolation, simple-mode editor kinds, favourite rename collisions,
and the reference's single-favourite-delete unpack failure.
"""
import json
import sys
import tempfile
from pathlib import Path
HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))


def record(session):
    from qtpy import QtWidgets as QW
    from hydrus.core import HydrusConstants as HC, HydrusExceptions
    from hydrus.client.gui import ClientGUIDialogsQuick, ClientGUIDialogsMessage, ClientGUITopLevelWindowsPanels
    from hydrus.client.gui.panels.options.ImportOptionsPanel import ImportOptionsPanel
    from hydrus.client.importing.options import ImportOptionsConstants as IOC, ImportOptionsContainer, ImportOptionsManager, NoteImportOptions
    from hydrus.client.networking import ClientNetworkingURLClass
    controller, gui = session.controller, session.controller.gui
    domain = controller.network_engine.domain_manager
    original = (ClientGUIDialogsQuick.GetYesNo, ClientGUIDialogsQuick.SelectFromListButtons,
                ClientGUIDialogsMessage.ShowInformation, ClientGUITopLevelWindowsPanels.DialogEdit,
                domain.GetURLClasses, domain.GetURLClassFromKey, controller.import_options_manager)

    def drive():
        classes = [ClientNetworkingURLClass.URLClass(name, url_class_key=bytes([index])*32, url_type=kind)
                   for index, name, kind in [(1,'alpha post',HC.URL_TYPE_POST),(2,'beta watch',HC.URL_TYPE_WATCHABLE),
                                            (3,'gamma gallery',HC.URL_TYPE_GALLERY),(4,'excluded file',HC.URL_TYPE_FILE)]]
        domain.GetURLClasses = lambda: classes
        domain.GetURLClassFromKey = lambda key: next(c for c in classes if c.GetClassKey()==key)
        manager = ImportOptionsManager.ImportOptionsManager()
        ImportOptionsManager.ImportOptionsManager.STATICPopulateManagerWithDefaultDefaults(manager)
        manager.SetFavouriteImportOptionContainers({})
        for caller in [IOC.IMPORT_OPTIONS_CALLER_TYPE_LOCAL_IMPORT, IOC.IMPORT_OPTIONS_CALLER_TYPE_CLIENT_API]:
            manager.SetDefaultImportOptionsContainerForCallerType(caller,ImportOptionsContainer.ImportOptionsContainer())
        incoming = ImportOptionsContainer.ImportOptionsContainer()
        incoming.SetImportOptions(NoteImportOptions.NoteImportOptions())
        options = controller.new_options.Duplicate()
        options.SetBoolean('import_options_simple_mode',True)
        panel = ImportOptionsPanel(gui, options, manager)
        calls, answers, choices, edits, rows = [], [], [], [], []
        ClientGUIDialogsMessage.ShowInformation = lambda parent,message,*args,**kwargs: calls.append({'information':message})
        def yes_no(parent,message,*args,**kwargs):
            answer=answers.pop(0)
            calls.append({'question':message,'yes':kwargs.get('yes_label','yes'),'no':kwargs.get('no_label','no'),'answer':answer})
            return QW.QDialog.DialogCode.Accepted if answer else QW.QDialog.DialogCode.Rejected
        ClientGUIDialogsQuick.GetYesNo = yes_no
        def choose(parent,title,items,*args,**kwargs):
            answer=choices.pop(0)
            calls.append({'title':title,'choices':list(items),'answer':answer})
            if answer is None: raise HydrusExceptions.CancelledException()
            return items[answer][1]
        ClientGUIDialogsQuick.SelectFromListButtons = choose
        class Dialog(QW.QDialog):
            def __init__(self,parent,title,*args,**kwargs):
                super().__init__(parent); self.label=title
            def __enter__(self):return self
            def __exit__(self,*args):self.deleteLater()
            def SetPanel(self,child):self.child=child
            def exec(self):
                accepted,name=edits.pop(0)
                child=self.child
                calls.append({'editor':self.label,'name':child.GetName(),'accepted':accepted,'kinds':[page._import_options_type for page in child._listbook.GetPages()]})
                if accepted:
                    child.GetValue=lambda:incoming.Duplicate()
                    if name is not None:child.GetName=lambda:name
                return QW.QDialog.DialogCode.Accepted if accepted else QW.QDialog.DialogCode.Rejected
        ClientGUITopLevelWindowsPanels.DialogEdit=Dialog
        def state():
            defaults=panel._default_import_options_list.GetData()
            urls=panel._url_class_import_options_list.GetData()
            favourites=panel._favourite_import_options_list.GetData()
            return {'simple':panel._simple_mode.isChecked(),
                    'defaults':[list(panel._ConvertDefaultDataToDisplayTuple(c)) for c in defaults],
                    'caller_codes':defaults,
                    'urls':[list(panel._ConvertURLClassDataToDisplayTuple(c)) for c in urls],
                    'url_presence':[panel._import_options_manager.GetDefaultImportOptionsContainerForURLClass(c.GetClassKey()) is not None for c in urls],
                    'favourites':[list(panel._ConvertFavouriteDataToDisplayTuple(f)) for f in favourites]}
        def step(action,selection=None,answer=None,choice=None,edit=None):
            calls.clear()
            if selection is not None:
                target,selected=selection
                target.clearSelection();target.SelectDatas(selected)
            if answer is not None:answers.append(answer)
            if choice is not None:choices.append(choice)
            if edit is not None:edits.append(edit)
            selected={'defaults':[IOC.import_options_caller_type_str_lookup[c] for c in panel._default_import_options_list.GetData(only_selected=True)],
                      'urls':[c.GetName() for c in panel._url_class_import_options_list.GetData(only_selected=True)],
                      'favourites':[name for name,_ in panel._favourite_import_options_list.GetData(only_selected=True)]}
            error=None
            try:getattr(panel,action)()
            except Exception as exc:error=type(exc).__name__+': '+str(exc)
            rows.append({'action':action,'selection':selected,'calls':list(calls),'error':error,'state':state()})
        initial=state()
        for code in initial['caller_codes']:
            step('_SeeDefaultStack',(panel._default_import_options_list,[code]))
        for cls in classes[:3]:step('_SeeURLClassStack',(panel._url_class_import_options_list,[cls]))
        step('_ShowTLDR')
        step('_ClearDefault',(panel._default_import_options_list,[IOC.IMPORT_OPTIONS_CALLER_TYPE_GLOBAL]))
        step('_EditDefault',(panel._default_import_options_list,[IOC.IMPORT_OPTIONS_CALLER_TYPE_POST_URLS]),edit=(False,None))
        step('_EditDefault',edit=(True,None))
        step('_ClearDefault',answer=False)
        step('_ClearDefault',(panel._default_import_options_list,[IOC.IMPORT_OPTIONS_CALLER_TYPE_GLOBAL,IOC.IMPORT_OPTIONS_CALLER_TYPE_POST_URLS]),answer=True)
        step('_EditURLClass',(panel._url_class_import_options_list,[classes[0]]),edit=(False,None))
        step('_EditURLClass',edit=(True,None))
        step('_ClearURLClass',answer=False)
        step('_ClearURLClass',(panel._url_class_import_options_list,classes[:2]),answer=True)
        step('_AddFavourite',edit=(False,'notes'))
        step('_AddFavourite',edit=(True,'notes'))
        step('_AddFavourite',edit=(True,'notes'))
        favourite=panel._favourite_import_options_list.GetData()[0]
        step('_EditFavourite',(panel._favourite_import_options_list,[favourite]),edit=(True,'notes (1)'))
        favourite=panel._favourite_import_options_list.GetData()[0]
        step('_DeleteFavourite',(panel._favourite_import_options_list,[favourite]))
        selected=panel._favourite_import_options_list.GetData()
        step('_DeleteFavourite',(panel._favourite_import_options_list,selected),answer=False)
        step('_DeleteFavourite',answer=True)
        step('_ResetDefaultToDefault',choice=0,answer=False)
        step('_ResetDefaultToDefault',choice=0,answer=True)
        step('_ResetDefaultToDefault',choice=1,answer=True)
        # reset favourites updates the manager but reference leaves the visible list stale
        step('_ResetDefaultToDefault',choice=2,answer=True)
        before=manager.DumpToString()
        isolated=before==manager.DumpToString() and before!=panel._import_options_manager.DumpToString()
        panel._simple_mode.setChecked(False)
        panel.UpdateOptions()
        applied=controller.import_options_manager is panel._import_options_manager
        reopened=ImportOptionsPanel(gui, options, controller.import_options_manager)
        result={'initial':initial,'steps':rows,'incoming_summary':incoming.GetSummary(IOC.IMPORT_OPTIONS_CALLER_TYPE_FAVOURITES),
                'draft_isolated':isolated,'applied':applied,'reopened_simple':reopened._simple_mode.isChecked(),
                'reopened_favourites':[list(reopened._ConvertFavouriteDataToDisplayTuple(f)) for f in reopened._favourite_import_options_list.GetData()]}
        panel.deleteLater();reopened.deleteLater()
        return result
    try:return controller.CallBlockingToQt(gui,drive)
    finally:
        (ClientGUIDialogsQuick.GetYesNo,ClientGUIDialogsQuick.SelectFromListButtons,
         ClientGUIDialogsMessage.ShowInformation,ClientGUITopLevelWindowsPanels.DialogEdit,
         domain.GetURLClasses,domain.GetURLClassFromKey,controller.import_options_manager)=original


def main():
    import hydrus_driver
    import record_api
    if len(sys.argv)>1 and sys.argv[1]=='--child':
        destination=Path(sys.argv[2])
        result=hydrus_driver.run_client(record_api.unpack_fixture('basic'),record)
        destination.write_text(json.dumps(result));return
    with tempfile.TemporaryDirectory() as work:
        destination=Path(work)/'panel.json'
        hydrus_driver.run_in_subprocess(str(Path(__file__).resolve()),'--child',str(destination))
        result=json.loads(destination.read_text())
    (HERE/'fixtures/import_options_panel.json').write_text(json.dumps(result,indent=2)+'\n')

if __name__=='__main__':main()

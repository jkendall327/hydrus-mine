#!/usr/bin/env python3
"""Record live Qt Client API review/editor rows, permission enablement, key questions,
delete confirmation, duplication names, and client server setting controls."""
import json, os, sys, tempfile
HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
def record(session):
    c=session.controller
    def work():
        from qtpy import QtWidgets as QW
        from hydrus.client import ClientAPI as A
        from hydrus.client.gui import ClientGUIAPI as G, ClientGUIDialogsQuick as Q, ClientGUIDialogsMessage as M
        from hydrus.client.gui.services import ClientGUIClientsideServices as S
        from hydrus.client.gui.lists import ClientGUIListConstants as L
        from hydrus.core import HydrusConstants as HC, HydrusExceptions as E, HydrusTags
        service=c.services_manager.GetService(HC.CLIENT_API_SERVICE_KEY) if hasattr(HC,'CLIENT_API_SERVICE_KEY') else next(s for s in c.services_manager.GetServices() if s.GetServiceType()==HC.CLIENT_API_SERVICE)
        keys=[A.APIPermissions(name='searcher',access_key=bytes([1])*32,permits_everything=False,basic_permissions=[3]), A.APIPermissions(name='full',access_key=bytes([2])*32,permits_everything=True)]
        c.client_api_manager.SetPermissions(keys)
        panel=S.ReviewServiceClientAPISubPanel(c.gui, service)
        editor=G.EditAPIPermissionsPanel(c.gui,keys[0])
        out={'columns':list(L.column_list_column_name_lookup[L.COLUMN_LIST_CLIENT_API_PERMISSIONS.ID].values()),'rows':[panel._ConvertDataToDisplayTuple(k) for k in keys], 'permissions':[(i,A.basic_permission_to_str_lookup[i]) for i in A.ALLOWED_PERMISSIONS], 'status':panel._service_status.text(),'actions':[]}
        def enabled(): return {'basic':editor._basic_permissions.isEnabled(),'all':editor._check_all_permissions_button.isEnabled(),'filter':editor._search_tag_filter.isEnabled()}
        out['search_enabled']=enabled()
        editor._permits_everything.setChecked(True); editor._UpdateEnabled(); out['full_enabled']=enabled()
        editor._permits_everything.setChecked(False); editor._basic_permissions.SetValue([]); editor._UpdateEnabled(); out['none_enabled']=enabled()
        oldq,olde,oldm=Q.GetYesNo,Q.EnterText,M.ShowCritical
        Q.GetYesNo=lambda parent,text,**kw:(out['actions'].append({'question':text}),QW.QDialog.DialogCode.Rejected)[1]
        panel._Delete()
        Q.EnterText=lambda parent,text,**kw:(out['actions'].append({'question':text}), 'bad')[1]
        M.ShowCritical=lambda parent,title,text:out['actions'].append({'title':title,'message':text})
        editor._EditAccessKey()
        Q.GetYesNo,Q.EnterText,M.ShowCritical=oldq,olde,oldm
        panel._permissions_list.SelectDatas(keys)
        panel._Duplicate()
        out['duplicates']=[panel._ConvertDataToDisplayTuple(k) for k in c.client_api_manager.GetAllPermissions()]
        # The access key editor on typed answers: stripped, wrong length, not hex, and the
        # random suggestion offered; then the review panel's collision refusal on a real edit.
        key_cases = []
        offered = []
        M.ShowCritical = lambda parent, title, text: key_cases[-1].update({'title': title, 'message': text})
        def ask(parent, text, default = None, suggestions = None, **kw):
            offered.append({'default': default, 'suggestions': suggestions})
            return current_answer[0]
        Q.EnterText = ask
        current_answer = [None]
        for typed in ['  ' + 'ab' * 32 + '  ', 'z' * 64, '', 'ab' * 31, 'AB' * 32]:
            current_answer[0] = typed
            editor._access_key.setText('cd' * 32)
            key_cases.append({'typed': typed})
            editor._EditAccessKey()
            key_cases[-1]['key_after'] = editor._access_key.text()
        out['key_cases'] = key_cases
        out['key_offered'] = {'default': offered[0]['default'], 'suggestion_count': len(offered[0]['suggestions']), 'suggestion_is_64_hex': all(len(x) == 64 and bytes.fromhex(x) is not None for x in offered[0]['suggestions'])}
        Q.EnterText, M.ShowCritical = olde, oldm
        from hydrus.client.gui import ClientGUITopLevelWindowsPanels as TLW
        class ScriptedDialog(QW.QDialog):
            script = None
            def __init__(self, parent, title): super().__init__(parent)
            def __enter__(self): return self
            def __exit__(self, *a): return False
            def SetPanel(self, p): self._panel = p
            def exec(self):
                ScriptedDialog.script(self._panel)
                return QW.QDialog.DialogCode.Accepted
        collisions = []
        old_dialog = S.ClientGUITopLevelWindowsPanels.DialogEdit
        S.ClientGUITopLevelWindowsPanels.DialogEdit = ScriptedDialog
        M.ShowCritical = lambda parent, title, text: collisions.append({'title': title, 'message': text})
        try:
            def keys_now(): return sorted((k.GetName(), k.GetAccessKey().hex()) for k in c.client_api_manager.GetAllPermissions())
            out['collision'] = {'before': keys_now()}
            panel._permissions_list.SelectDatas([keys[0]], deselect_others=True)
            ScriptedDialog.script = lambda p: p._access_key.setText((bytes([2]) * 32).hex())
            panel._Edit()
            out['collision']['after_collision'] = keys_now()
            out['collision']['dialogs'] = list(collisions)
            ScriptedDialog.script = lambda p: p._access_key.setText((bytes([9]) * 32).hex())
            panel._Edit()
            out['collision']['after_new_key'] = keys_now()
            out['collision']['dialogs_after_new_key'] = list(collisions)
        finally:
            S.ClientGUITopLevelWindowsPanels.DialogEdit = old_dialog
            M.ShowCritical = oldm
        # "open client api base url": what the real handler launches or warns, by the
        # service's port and https setting
        from hydrus.client.gui.executables import ClientGUIExecutableActions as EX
        launched=[];warned=[]
        old_open,old_warn=EX.OpenExternallyURLDefault,M.ShowWarning
        EX.OpenExternallyURLDefault=lambda win,url:launched.append(url)
        M.ShowWarning=lambda win,text:warned.append(text)
        base_cases=[]
        try:
            for port,https in [(None,False),(None,True),(45869,False),(45869,True),(1,False),(65535,True)]:
                class FakeService:
                    def GetPort(self):return port
                    def UseHTTPS(self):return https
                    def GetServiceKey(self):return service.GetServiceKey()
                    def GetName(self):return service.GetName()
                panel._service=FakeService()
                del launched[:];del warned[:]
                panel._OpenBaseURL()
                base_cases.append(dict(port=port,https=https,launched=list(launched),warned=list(warned)))
        finally:
            EX.OpenExternallyURLDefault,M.ShowWarning=old_open,old_warn
        out['base_url']=base_cases
        server=S.EditServiceClientServerSubPanel(c.gui,HC.CLIENT_API_SERVICE, service.GetSerialisableDictionary())
        out['server']={'running':server._run_the_service.isChecked(),'port':server._port.value(),'port_range':[server._port.minimum(),server._port.maximum()], 'fields':server.GetValue()}
        out['server']['fields']={k:v for k,v in out['server']['fields'].items() if k!='bandwidth_rules'}
        panel.deleteLater(); editor.deleteLater(); server.deleteLater()
        return out
    return c.CallBlockingToQt(c.gui,work)
def main():
    import hydrus_driver
    if len(sys.argv)>1 and sys.argv[1]=='--child':
        import record_api
        out = sys.argv[2]
        result = hydrus_driver.run_client(record_api.unpack_fixture('basic'), record)
        with open(out, 'w') as f: json.dump(result, f)
        return
    with tempfile.TemporaryDirectory() as work:
        out = os.path.join(work, 'result.json')
        hydrus_driver.run_in_subprocess(os.path.abspath(__file__), '--child', out)
        with open(out) as f: result=json.load(f)
    with open(os.path.join(HERE,'fixtures/client_api_admin.json'),'w') as f:
        json.dump(result,f,indent=1,ensure_ascii=False)
        f.write('\n')
    print('wrote client_api_admin.json')
if __name__=='__main__': main()

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

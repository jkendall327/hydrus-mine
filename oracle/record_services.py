#!/usr/bin/env python3
"""Record real service review labels/statistics, manage rows, delete guards,
confirmation and numerical editor normalization on the running basic client.
No CommitChanges is called, so the reference store is unchanged.
"""
import json
import os
import sys
import tempfile
import types
HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)

def record(session):
    controller = session.controller
    gui = controller.gui
    def work():
        from qtpy import QtWidgets as QW
        from hydrus.core import HydrusConstants as HC
        from hydrus.client.gui.services import ClientGUIClientsideServices as S
        from hydrus.client.gui import ClientGUIDialogsMessage as M, ClientGUIDialogsQuick as Q
        panel = S.ManageClientServicesPanel(gui)
        services = panel._listctrl.GetData()
        from hydrus.client.gui.lists import ClientGUIListConstants as CGLC
        result = {'rows': [], 'actions': [], 'types': [], 'columns': list(CGLC.column_list_column_name_lookup[CGLC.COLUMN_LIST_MANAGE_SERVICES.ID].values()), 'sort': CGLC.default_column_list_sort_lookup[CGLC.COLUMN_LIST_MANAGE_SERVICES.ID]}
        for service in services:
            kind = service.GetServiceType()
            result['rows'].append({'key':service.GetServiceKey().hex(), 'row':panel._ConvertServiceToDisplayTuple(service)})
            if kind not in [t['code'] for t in result['types']]:
                result['types'].append({'code':kind, 'name':HC.service_string_lookup[kind], 'description':HC.service_description_lookup[kind]})
            review = S.ReviewServiceSubPanel(gui, service)
            row = result['rows'][-1]
            row['label'] = review._name_and_type.text()
            # Invoke the actual fetcher on a text sink without starting its
            # asynchronous constructor or racing the client's worker threads.
            old = controller.CallAfterQtSafe
            controller.CallAfterQtSafe = lambda widget, callback, *args: callback(*args)
            sink = QW.QLabel(gui)
            try:
                info = controller.Read('service_info', service.GetServiceKey())
                if kind in HC.REAL_FILE_SERVICES and HC.SERVICE_INFO_NUM_FILES in info:
                    S.ReviewServiceFileSubPanel.THREADFetchInfo(types.SimpleNamespace(_file_info_st=sink),service)
                    row['statistics'] = sink.text()
                elif kind in HC.REAL_TAG_SERVICES:
                    S.ReviewServiceTagSubPanel.THREADFetchInfo(types.SimpleNamespace(_tag_info_st=sink),service)
                    row['statistics'] = sink.text()
                elif kind in HC.RATINGS_SERVICES:
                    S.ReviewServiceRatingSubPanel.THREADFetchInfo(types.SimpleNamespace(_rating_info_st=sink),service)
                    row['statistics'] = sink.text()
            finally:
                controller.CallAfterQtSafe = old
        old_info, old_question = M.ShowInformation, Q.GetYesNo
        messages = []
        M.ShowInformation = lambda parent, text: messages.append(text)
        Q.GetYesNo = lambda parent, text, **kwargs: (messages.append(text), QW.QDialog.DialogCode.Rejected)[1]
        try:
            for kind in [HC.LOCAL_FILE_DOMAIN, HC.LOCAL_TAG, HC.LOCAL_RATING_LIKE]:
                panel._listctrl.SelectDatas([s for s in services if s.GetServiceType() == kind], deselect_others=True)
                messages.clear()
                panel._Delete()
                result['actions'].append({'type':kind, 'messages':list(messages)})
            panel._listctrl.SelectDatas([next(s for s in services if s.GetName()=='my files')], deselect_others=True)
            messages.clear()
            panel._Delete()
            result['nonempty_delete'] = list(messages)
            result['apply_ok'] = panel.UserIsOKToOK()
            panel._listctrl.SelectDatas([next(s for s in services if s.GetName()=='downloader tags')], deselect_others=True)
            Q.GetYesNo = lambda parent,text,**kwargs: QW.QDialog.DialogCode.Accepted
            panel._Delete()
            Q.GetYesNo = lambda parent,text,**kwargs: (messages.append(text),QW.QDialog.DialogCode.Rejected)[1]
            messages.clear()
            result['apply_deleted_ok'] = panel.UserIsOKToOK()
            result['apply_deletion'] = list(messages)
        finally:
            M.ShowInformation, Q.GetYesNo = old_info, old_question
        from hydrus.client import ClientServices
        service = ClientServices.GenerateService(bytes([99])*32, HC.LOCAL_RATING_NUMERICAL, 'new service')
        result['rating_defaults'] = []
        for kind in [HC.LOCAL_RATING_LIKE,HC.LOCAL_RATING_NUMERICAL,HC.LOCAL_RATING_INCDEC]:
            d = ClientServices.GenerateService(bytes([kind])*32,kind,'new service').ToTuple()[3]
            result['rating_defaults'].append({'type':kind,'colours':d['colours'],'thumbnail':d['show_in_thumbnail'],'null_thumbnail':d['show_in_thumbnail_even_when_null'],'shape':d.get('shape',None)})
        edit = S.EditClientServicePanel(gui, service)
        numerical = next(p for p in edit._panels if isinstance(p, S.EditServiceRatingsNumericalSubPanel))
        numerical._num_stars.setValue(1)
        numerical._allow_zero.setChecked(False)
        result['numerical_one_star'] = numerical.GetValue()
        result['numerical_ranges'] = {'stars':[numerical._num_stars.minimum(), numerical._num_stars.maximum()], 'padding':[numerical._custom_pad.minimum(), numerical._custom_pad.maximum()]}
        edit._service_panel._name.setText('')
        try:
            edit.GetValue()
        except Exception as e:
            result['empty_name_error'] = str(e)
        panel.deleteLater()
        return result
    return controller.CallBlockingToQt(gui, work)

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
    with open(os.path.join(HERE,'fixtures/services.json'),'w') as f:
        json.dump(result,f,indent=1,ensure_ascii=False)
        f.write('\n')
    print('wrote services.json')
if __name__=='__main__': main()

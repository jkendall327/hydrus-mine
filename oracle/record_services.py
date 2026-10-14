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
            row['service_id'] = controller.Read('service_id', service.GetServiceKey())
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
        # The add menu: the labels and tooltips of the real button's items.
        result['add_menu'] = [[HC.service_string_lookup[t], 'Add a new {}.'.format(HC.service_string_lookup[t]), t] for t in HC.ADDREMOVABLE_SERVICES]
        # Driving the real "add" path (panel._Add) with the edit dialog replaced by a
        # scripted answer: the generated key, the default name and the casefold rename.
        from hydrus.client.gui import ClientGUITopLevelWindowsPanels as TLW
        class ScriptedDialog(QW.QDialog):
            script = None
            seen = []
            def __init__(self, parent, title):
                super().__init__(parent)
                self._title = title
            def __enter__(self): return self
            def __exit__(self, *a): return False
            def SetPanel(self, p): self._panel = p
            def exec(self):
                ScriptedDialog.seen.append({'title': self._title, 'name': self._panel.GetValue().GetName(), 'type': self._panel.GetValue().GetServiceType(), 'key_len': len(self._panel.GetValue().GetServiceKey())})
                if ScriptedDialog.script is None: return QW.QDialog.DialogCode.Rejected
                ScriptedDialog.script(self._panel)
                return QW.QDialog.DialogCode.Accepted
        old_dialog = TLW.DialogEdit
        TLW.DialogEdit = ScriptedDialog
        try:
            ScriptedDialog.seen = []
            before = len(panel._listctrl.GetData())
            for kind in HC.ADDREMOVABLE_SERVICES:
                ScriptedDialog.script = None
                panel._Add(kind)
            result['add_cancelled'] = {'dialogs': list(ScriptedDialog.seen), 'rows_before': before, 'rows_after': len(panel._listctrl.GetData())}
            def name_it(p, name):
                p._service_panel._name.setText(name)
            names = {}
            for kind, typed in [(HC.LOCAL_TAG, 'MY TAGS'), (HC.LOCAL_TAG, 'my tags'), (HC.LOCAL_RATING_LIKE, 'FAVOURITES'), (HC.LOCAL_FILE_DOMAIN, 'Art')]:
                ScriptedDialog.script = lambda p, typed=typed: name_it(p, typed)
                panel._Add(kind)
            result['add_collisions'] = {'existing': sorted(s.GetName() for s in services), 'after': sorted(s.GetName() for s in panel._listctrl.GetData()), 'typed': ['MY TAGS', 'my tags', 'FAVOURITES', 'Art']}
            # the rating colours, set through the real colour pickers
            from hydrus.client.metadata import ClientRatings
            ScriptedDialog.seen = []
            def set_colours(p):
                sub = next(x for x in p._panels if isinstance(x, S.EditServiceRatingsSubPanel))
                for colour_type, (b, f) in {ClientRatings.LIKE: ((1, 2, 3), (4, 5, 6)), ClientRatings.DISLIKE: ((7, 8, 9), (10, 11, 12)), ClientRatings.NULL: ((13, 14, 15), (16, 17, 18)), ClientRatings.MIXED: ((19, 20, 21), (22, 23, 24))}.items():
                    border, fill = sub._colour_ctrls[colour_type]
                    border.SetColour(QG.QColor(*b)); fill.SetColour(QG.QColor(*f))
                sub._show_in_thumbnail.setChecked(True)
                sub._show_in_thumbnail_even_when_null.setChecked(True)
                p._service_panel._name.setText('coloured')
            from qtpy import QtGui as QG
            ScriptedDialog.script = set_colours
            panel._Add(HC.LOCAL_RATING_LIKE)
            coloured = next(s for s in panel._listctrl.GetData() if s.GetName() == 'coloured')
            d = coloured.ToTuple()[3]
            result['rating_colours_edited'] = {'type': HC.LOCAL_RATING_LIKE, 'colours': d['colours'], 'thumbnail': d['show_in_thumbnail'], 'null_thumbnail': d['show_in_thumbnail_even_when_null'], 'shape': d.get('shape')}
        finally:
            TLW.DialogEdit = old_dialog
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

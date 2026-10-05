#!/usr/bin/env python3
"""Drive the actual Options namespace grouping queue and tag-sort consumer.

Raw allow-blank Add/Edit answers, duplicate rows, empty/colon sentinels, stable
queue selection/reordering and confirmations are recorded without cleaning.
The staged list commits only through UpdateOptions; real SortTags then reads it.
"""
import json
import sys
import tempfile
from pathlib import Path
HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))

def record(session):
    def work():
        from qtpy import QtCore as QC, QtWidgets as QW
        from hydrus.core import HydrusExceptions
        from hydrus.client import ClientConstants as CC
        from hydrus.client.gui import ClientGUIDialogsQuick as D
        from hydrus.client.gui.panels.options.TagSortPanel import TagSortPanel
        from hydrus.client.metadata import ClientTagSorting as S
        c = session.controller
        original = c.new_options.GetStringList('user_namespace_group_by_sort')
        draft = c.new_options.Duplicate()
        panel = TagSortPanel(c.gui, draft)
        queue = panel._user_namespace_group_by_sort
        panel.resize(900, 700); panel.show(); QW.QApplication.processEvents()
        panel.grab().save(str(HERE / 'fixtures/tag-namespace-order-qt.png'))
        old_text, old_yesno = D.EnterText, D.GetYesNo
        calls = []
        response = [None]
        def enter(owner, message, default='', **kwargs):
            calls.append({'kind':'text','message':message,'default':default,'allow_blank':kwargs.get('allow_blank',True),'answer':response[0]})
            if response[0] is None: raise HydrusExceptions.CancelledException()
            return response[0]
        def yesno(owner, message, **kwargs):
            calls.append({'kind':'question','message':message,'answer':response[0]})
            return QW.QDialog.DialogCode.Accepted if response[0] else QW.QDialog.DialogCode.Rejected
        D.EnterText, D.GetYesNo = enter, yesno
        def state():
            return {'data':list(queue.GetData()), 'labels':[queue._listbox.item(i).text() for i in range(queue._listbox.count())],
                    'selected':[i for i in range(queue._listbox.count()) if queue._listbox.item(i).isSelected()]}
        out = {'initial':state(), 'steps':[]}
        def step(action, answer=None, selection=None):
            if selection is not None:
                queue._listbox.clearSelection()
                for i in selection: queue._listbox.item(i).setSelected(True)
            calls.clear(); response[0] = answer
            getattr(queue,action)()
            out['steps'].append({'action':action,'answer':answer,'selection':selection,'calls':list(calls),'state':state(),
                                 'saved':c.new_options.GetStringList('user_namespace_group_by_sort')})
        try:
            step('_Add',None)
            for raw in ['', ':', ' CREATOR ', 'creator', 'artist:inner', 'creator-id', 'creator', '猫']:
                step('_Add',raw)
            step('_Edit',None,[0,1])
            step('_Edit','studio',[0,1])
            step('_Edit','',[1])
            step('_Edit',':',[2])
            step('_Edit','CREATOR',[3])
            step('_Up',selection=[queue._listbox.count()-1])
            step('_Down',selection=[0,1])
            step('_Delete',False,[1,3])
            step('_Delete',True,[1,3])
            out['draft_before_apply'] = draft.GetStringList('user_namespace_group_by_sort')
            panel.UpdateOptions()
            out['applied'] = draft.GetStringList('user_namespace_group_by_sort')
            c.new_options.SetStringList('user_namespace_group_by_sort',out['applied'])
            reopened = TagSortPanel(c.gui,draft)
            out['reopened'] = list(reopened._user_namespace_group_by_sort.GetData())
            tags = ['creator:z','series:b','studio:s','character:c','meta:m','plain','artist:2','artist:10','species:y','猫:q']
            counts = {tag: i % 3 + 1 for i,tag in enumerate(tags)}
            out['tags'],out['counts'],out['sorts'] = tags,counts,[]
            for kind in [S.SORT_BY_HUMAN_TAG,S.SORT_BY_HUMAN_SUBTAG,S.SORT_BY_COUNT]:
                for order in [CC.SORT_ASC,CC.SORT_DESC]:
                    sort = S.TagSort(kind,order,True,S.GROUP_BY_NAMESPACE_USER)
                    actual = list(tags); S.SortTags(sort,actual,tag_items_to_count=counts)
                    out['sorts'].append({'type':kind,'ascending':order==CC.SORT_ASC,'tags':actual})
            reopened.deleteLater()
            # Run the real EnterText dialog after the scripted queue matrix.
            # Buttons use the actual DialogEdit handlers, including blank acceptance.
            D.EnterText = old_text
            out['real_text'] = []
            for raw, accept in [('', True), (' CREATOR ', True), ('cancelled raw', False)]:
                observed = {}
                def settle_dialog():
                    dialog = QW.QApplication.activeModalWidget()
                    assert dialog is not None
                    edit = dialog.findChild(QW.QLineEdit)
                    observed['title'] = dialog.windowTitle()
                    observed['default'] = edit.text()
                    observed['buttons'] = [button.text() for button in dialog.findChildren(QW.QPushButton)]
                    edit.setText(raw)
                    observed['typed'] = edit.text()
                    QW.QApplication.processEvents()
                    if raw == '':
                        dialog.grab().save(str(HERE / 'fixtures/tag-namespace-order-enter-text-qt.png'))
                    if accept: dialog._apply.click()
                    else: dialog._cancel.click()
                QC.QTimer.singleShot(0, settle_dialog)
                try:
                    result = panel._EditNamespaceGroupBySort('namespace')
                    observed['result'] = result
                    observed['cancelled'] = False
                except HydrusExceptions.CancelledException:
                    observed['result'] = None
                    observed['cancelled'] = True
                out['real_text'].append(observed)
        finally:
            D.EnterText,D.GetYesNo = old_text,old_yesno
            c.new_options.SetStringList('user_namespace_group_by_sort',original)
            panel.hide();panel.deleteLater()
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
    (HERE/'fixtures/tag_namespace_order.json').write_text(json.dumps(value,indent=2,ensure_ascii=False)+'\n')
    print('wrote tag_namespace_order.json')
if __name__=='__main__':main()

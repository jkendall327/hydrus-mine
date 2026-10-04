#!/usr/bin/env python3
"""Record the actual JSON exporter object-name queue and EnterText validation.

Use only a private basic fixture; observe real add/edit/reorder/delete callbacks,
with dialog publication driven deterministically. No files are exported here.
"""
import json, os, sys, tempfile
HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)

def record(session):
    c = session.controller
    def work():
        from qtpy import QtWidgets as QW, QtCore as QC
        from hydrus.client.gui.metadata import ClientGUIMetadataMigrationExporters as G
        from hydrus.client.metadata import ClientMetadataMigrationExporters as E
        from hydrus.client.gui import ClientGUIDialogsQuick as Q
        from hydrus.client.gui.panels import ClientGUIScrolledPanelsTextEntry as T
        from hydrus.core import HydrusExceptions
        owner = QW.QWidget(c.gui)
        panel = G.EditSingleFileMetadataExporterWidget(owner, E.SingleFileMetadataExporterJSON(), [E.SingleFileMetadataExporterTXT,E.SingleFileMetadataExporterJSON])
        queue = panel._nested_object_names_list
        answers, questions, states, inputs = [], [], [], []
        old_text,old_yes=Q.EnterText,Q.GetYesNo
        def enter(parent,message,default='',**kwargs):
            answer=answers.pop(0)
            inputs.append({'message':message,'default':default,'answer':answer})
            if answer is None:raise HydrusExceptions.CancelledException('Dialog cancelled.')
            real=T.EditTextPanel(parent,message,default)
            try:
                real._text.setText(answer)
                try:return real.GetValue()
                except Exception as e:
                    inputs[-1]['error']=str(e);raise
            finally:real.deleteLater()
        def yes(parent,message,**kwargs):
            answer=answers.pop(0);questions.append({'message':message,'answer':answer})
            return QW.QDialog.DialogCode.Accepted if answer else QW.QDialog.DialogCode.Rejected
        def select(indices):
            queue._listbox.clearSelection()
            for i in indices:queue._listbox.item(i).setSelected(True)
        def state(case):
            states.append({'case':case,'names':queue.GetData(),'selected':[i for i in range(queue._listbox.count()) if queue._listbox.item(i).isSelected()], 'serialised':panel.GetValue().GetSerialisableTuple()})
        Q.EnterText,Q.GetYesNo=enter,yes
        try:
            state('empty')
            for name in ['files',' tags ','files','   ','line\nbreak']:
                answers.append(name);queue._Add();state('add')
            answers.append(None);queue._Add();state('cancel_add')
            answers.append('')
            try:queue._Add()
            except HydrusExceptions.CancelledException:pass
            state('blank_add_veto')
            select([1,3]);answers.append(' renamed ');queue._Edit();state('edit_first_of_selection')
            answers.append(None);queue._Edit();state('cancel_edit')
            queue._Up();state('up_selected')
            queue._Down();state('down_selected')
            answers.append(False);queue._Delete();state('cancel_delete')
            answers.append(True);queue._Delete();state('delete_selected')
            panel.resize(640,560);panel.show();QW.QApplication.processEvents()
            panel.grab().save(os.path.join(HERE,'fixtures/sidecar_json_names_editor.png'))
            return {'states':states,'inputs':inputs,'questions':questions}
        finally:
            Q.EnterText,Q.GetYesNo=old_text,old_yes;owner.deleteLater()
    return c.CallBlockingToQt(c.gui,work)

def main():
    import hydrus_driver
    if len(sys.argv)>1 and sys.argv[1]=='--child':
        import record_api
        output=sys.argv[2]
        result=hydrus_driver.run_client(record_api.unpack_fixture('basic'),record)
        with open(output,'w') as f:json.dump(result,f)
        return
    with tempfile.TemporaryDirectory() as directory:
        output=os.path.join(directory,'result.json')
        hydrus_driver.run_in_subprocess(os.path.abspath(__file__),'--child',output)
        with open(output) as f:result=json.load(f)
    with open(os.path.join(HERE,'fixtures/sidecar_json_names.json'),'w') as f:
        json.dump(result,f,indent=1,ensure_ascii=False);f.write('\n')
    print('wrote sidecar_json_names.json')
if __name__=='__main__':main()

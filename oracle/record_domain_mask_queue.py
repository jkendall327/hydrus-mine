#!/usr/bin/env python3
"""Drive full domain-mask raw/regex queues and actual owned entry dialogs with staged CRUD."""
import json
import os
import sys
import tempfile
HERE=os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0,HERE)
ACTIONS=[
 ['add',False,None,[[' second.example ',True]]],
 ['add',False,None,[['discard.example',False]]],
 ['add',False,None,[['',True]]],
 ['add',False,None,[['   ',True]]],
 ['edit',False,[0,1],[['renamed.example',True],['discard.example',False]]],
 ['add',True,None,[[r' img\d+\.cdn\.example ',True]]],
 ['add',True,None,[['[',True]]],
 ['add',True,None,[['   ',True]]],
 ['delete',True,[1],False],
 ['delete',True,[1],True],
 ['delete',False,[0,1],False],
 ['delete',False,[0,1],True],
]


def record(session):
    c=session.controller
    def qt():
        from qtpy import QtCore as QC, QtWidgets as QW
        from hydrus.client.gui.panels import ClientGUIURLClass as C
        from hydrus.client.gui import ClientGUIDialogsQuick as Q, ClientGUITopLevelWindowsPanels as W, ClientGUIDialogsMessage as M
        from hydrus.client.networking import ClientNetworkingURLClass as U
        panel=C.EditURLDomainMaskWidget(c.gui,U.URLDomainMask(raw_domains=['mask.example']))
        panel._widget_mode.setCurrentIndex(1)
        steps=[];answers=[];opened=[];said=[]
        class Dialog(W.DialogEdit):
            def exec(self):
                value,yes=answers.pop(0)
                ctrl=self._panel._control if hasattr(self._panel,'_control') else self._panel._text
                initial=ctrl.GetValue() if hasattr(ctrl,'GetValue') else ctrl.text()
                opened.append({'title':self.windowTitle(),'initial':initial,'entered':value,'yes':yes})
                def act():
                    if hasattr(ctrl,'SetValue'):ctrl.SetValue(value)
                    else:ctrl.setText(value)
                    (self._apply if yes else self._cancel).click()
                    if yes and value=='':QC.QTimer.singleShot(0,self._cancel.click)
                QC.QTimer.singleShot(0,act)
                result=super().exec()
                opened[-1]['accepted']=result==QW.QDialog.DialogCode.Accepted
                return result
        original_dialog=W.DialogEdit;original_question=Q.GetYesNo;original_warning=M.ShowWarning
        W.DialogEdit=Dialog
        M.ShowWarning=lambda parent,text,**kw:said.append({'warning':text})
        def snapshot():
            mask=panel.GetValue()
            return {'raw_rows':panel._raw_domain_add_remove_list.GetData(),'regex_rows':panel._domain_regex_add_remove_list.GetData(),'raw_mask':mask.GetRawDomains(),'regex_mask':mask.GetDomainRegexes(),'mode_enabled':panel._widget_mode.isEnabled()}
        try:
            steps.append({'action':'initial','state':snapshot()})
            for action,regex,indices,response in ACTIONS:
                ctrl=panel._domain_regex_add_remove_list if regex else panel._raw_domain_add_remove_list
                opened.clear();said.clear()
                ctrl._listbox.clearSelection()
                if indices is not None:
                    for i in indices:ctrl._listbox.item(i).setSelected(True)
                if action=='delete':
                    def question(parent,text,**kw):
                        said.append({'question':text})
                        return QW.QDialog.DialogCode.Accepted if response else QW.QDialog.DialogCode.Rejected
                    Q.GetYesNo=question;ctrl._Delete()
                else:
                    answers[:]=response
                    (ctrl._Add if action=='add' else ctrl._Edit)()
                QW.QApplication.processEvents()
                steps.append({'action':action,'regex':regex,'indices':indices,'response':response,'dialogs':list(opened),'said':list(said),'state':snapshot()})
        finally:
            W.DialogEdit=original_dialog;Q.GetYesNo=original_question;M.ShowWarning=original_warning
        panel.deleteLater()
        return {'steps':steps}
    return c.CallBlockingToQt(c.gui,qt)


def main():
    import hydrus_driver
    if len(sys.argv)>1 and sys.argv[1]=='--child':
        import record_api
        output=sys.argv[2];value=hydrus_driver.run_client(record_api.unpack_fixture('basic'),record)
        with open(output,'w') as f:json.dump(value,f)
        return
    with tempfile.TemporaryDirectory() as tmp:
        path=os.path.join(tmp,'out.json');hydrus_driver.run_in_subprocess(os.path.abspath(__file__),'--child',path)
        with open(path) as f:value=json.load(f)
    with open(os.path.join(HERE,'fixtures','domain_mask_queue.json'),'w') as f:json.dump(value,f,indent=1);f.write('\n')
    print('recorded domain mask queues and owned raw/regex entry dialogs')


if __name__=='__main__':main()

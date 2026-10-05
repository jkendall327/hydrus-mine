#!/usr/bin/env python3
"""Actual RegexPanel staging and nested RegexInput saved-favourites copy chooser.

Drives the real favourite list Add/Edit/Delete handlers and actual RegexInput
controls, substituting only outer modal answers. Invalid fragments are advisory;
either row-input cancellation preserves the list. No import action exists here.
"""
import json
import sys
import tempfile
from pathlib import Path
HERE=Path(__file__).resolve().parent
sys.path.insert(0,str(HERE))

def record(session):
    def drive():
        from qtpy import QtWidgets as QW
        from hydrus.core import HydrusConstants as HC,HydrusExceptions
        from hydrus.client.gui import ClientGUIDialogsQuick,ClientGUIDialogsMessage,ClientGUICore
        from hydrus.client.gui.panels.options.RegexPanel import RegexPanel
        from hydrus.client.gui.panels import ClientGUIScrolledPanelsEditRegexFavourites as M
        c=session.controller; original=list(HC.options['regex_favourites'])
        initial=[('z+','last'),('a+','letters')]; HC.options['regex_favourites']=initial.copy()
        p=RegexPanel(c.gui); panel=p._regex_panel; steps=[];current={};said=[];opened=[];menus=[];copied=[]
        originals=(M.ClientGUITopLevelWindowsPanels.DialogEdit,ClientGUIDialogsQuick.EnterText,ClientGUIDialogsQuick.GetYesNo,ClientGUIDialogsMessage.ShowWarning,ClientGUICore.core().PopupMenu,c.pub)
        def pub(topic,*args,**kwargs):
            if topic=='clipboard' and args[0]=='text':copied.append(args[1]);return
            return originals[-1](topic,*args,**kwargs)
        c.pub=pub
        def popup(parent,menu):
            favourites=next(a.menu() for a in menu.actions() if a.text()=='favourites')
            rows=[]
            for a in favourites.actions():
                if a.isSeparator():rows.append({'separator':True});continue
                rows.append(dict(label=a.text(),enabled=a.isEnabled()))
                if a.text()!='manage favourites' and a.isEnabled():a.trigger()
            menus.append(dict(rows=rows,saved=list(HC.options['regex_favourites'])))
        ClientGUICore.core().PopupMenu=popup
        class Dialog(QW.QWidget):
            def __init__(self,parent,title,**kwargs):super().__init__(c.gui);self.title=title
            def __enter__(self):return self
            def __exit__(self,*args):self.deleteLater();return False
            def SetPanel(self,child):self.panel=child
            def exec(self):
                control=self.panel._control;before=control.GetValue();control.SetValue(current['phrase']);unchanged=control.GetValue()
                if current.get('menu'):control._regex_button._ShowMenu()
                opened.append(dict(title=self.title,before=before,phrase=control.GetValue(),unchanged=unchanged,valid=control._regex_text.objectName()=='HydrusValid'))
                return QW.QDialog.DialogCode.Accepted if current.get('phrase_accept',True) else QW.QDialog.DialogCode.Rejected
        M.ClientGUITopLevelWindowsPanels.DialogEdit=Dialog
        def enter(parent,question,**kwargs):
            said.append(dict(question=question,default=kwargs.get('default')))
            if current.get('description') is None:raise HydrusExceptions.CancelledException()
            return current['description']
        ClientGUIDialogsQuick.EnterText=enter
        ClientGUIDialogsQuick.GetYesNo=lambda parent,text,**kwargs:QW.QDialog.DialogCode.Accepted if current['answer'] else QW.QDialog.DialogCode.Rejected
        ClientGUIDialogsMessage.ShowWarning=lambda parent,text,**kwargs:said.append(dict(warning=text))
        def state():return dict(draft=panel.GetValue(),saved=list(HC.options['regex_favourites']))
        actions=[dict(action='add',phrase=r'\d+',description='numbers',menu=True),dict(action='add',phrase=r'\d+',description='numbers'),dict(action='add',phrase='[',description='fragment'),dict(action='add',phrase='discard',description='unused',phrase_accept=False),dict(action='add',phrase='temporary',description=None,menu=True),dict(action='edit',selected=['[','fragment'],phrase='^a+$',description='letters only'),dict(action='edit',selected=['^a+$','letters only'],phrase='discard',description=None),dict(action='delete',selected=[[r'\d+','numbers'],['z+','last']],answer=False),dict(action='delete',selected=[[r'\d+','numbers'],['z+','last']],answer=True)]
        try:
            for operation in actions:
                current.clear();current.update(operation);said.clear();opened.clear();before=state();count=len(menus);clip_count=len(copied)
                if operation['action']=='add':panel._Add()
                elif operation['action']=='edit':panel._regexes.SelectDatas([tuple(operation['selected'])],deselect_others=True);panel._Edit()
                else:panel._regexes.SelectDatas([tuple(v) for v in operation['selected']],deselect_others=True);panel._regexes.ProcessDeleteAction()
                steps.append(dict(operation=operation,before=before,after=state(),said=said.copy(),opened=opened.copy(),menus=menus[count:],copied=copied[clip_count:]))
            cancelled=state();fresh=RegexPanel(c.gui);reopened_cancel=fresh._regex_panel.GetValue();fresh.deleteLater()
            p.UpdateOptions();saved=list(HC.options['regex_favourites']);fresh=RegexPanel(c.gui);reopened=fresh._regex_panel.GetValue();fresh.deleteLater()
            # Actual EnterText defaults reject empty, preserve whitespace.
            from hydrus.client.gui.panels.ClientGUIScrolledPanelsTextEntry import EditTextPanel
            descriptions=[]
            for text in ['', ' ', '\t', 'name']:
                entry=EditTextPanel(c.gui,'Enter description.');entry._text.setText(text)
                try:descriptions.append(dict(input=text,value=entry.GetValue(),error=None))
                except HydrusExceptions.CancelledException as error:descriptions.append(dict(input=text,value=None,error=str(error)))
                entry.deleteLater()
            # A new real input sees accepted saved rows and only copies them.
            from hydrus.client.gui.widgets.ClientGUIRegex import RegexInput
            consumer=RegexInput(c.gui);consumer.SetValue('original+');count=len(copied);consumer._regex_button._ShowMenu()
            p.resize(900,600);p.show();QW.QApplication.processEvents();p.grab().save(str(HERE/'fixtures/regex_options_editor_reference.png'))
            result=dict(initial=initial,steps=steps,cancelled=cancelled,reopened_cancel=reopened_cancel,saved=saved,reopened=reopened,descriptions=descriptions,consumer=dict(menu=menus[-1],copied=copied[count:],input=consumer.GetValue()))
            consumer.deleteLater();return result
        finally:
            HC.options['regex_favourites']=original
            M.ClientGUITopLevelWindowsPanels.DialogEdit,ClientGUIDialogsQuick.EnterText,ClientGUIDialogsQuick.GetYesNo,ClientGUIDialogsMessage.ShowWarning,ClientGUICore.core().PopupMenu,c.pub=originals
            p.deleteLater()
    return session.controller.CallBlockingToQt(session.controller.gui,drive)

def main():
    import hydrus_driver,record_api
    if len(sys.argv)>1 and sys.argv[1]=='--child':Path(sys.argv[2]).write_text(json.dumps(hydrus_driver.run_client(record_api.unpack_fixture('basic'),record)));return
    with tempfile.TemporaryDirectory() as d:
        out=Path(d)/'result.json';hydrus_driver.run_in_subprocess(str(Path(__file__).resolve()),'--child',str(out));result=json.loads(out.read_text())
    (HERE/'fixtures/regex_options_editor.json').write_text(json.dumps(result,indent=2)+'\n');print('wrote regex_options_editor.json')
if __name__=='__main__':main()

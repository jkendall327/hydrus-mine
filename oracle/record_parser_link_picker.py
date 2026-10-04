#!/usr/bin/env python3
"""Drive actual owned Qt parser selector, current selection, no-op separator and clear prompts."""
import json
import os
import sys
import tempfile
HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)


def record(session):
    c = session.controller
    def qt():
        from qtpy import QtCore as QC, QtWidgets as QW
        from hydrus.client import ClientStrings as S
        from hydrus.client.gui import ClientGUIDownloaders as G, ClientGUIDialogsQuick as Q, ClientGUIDialogsMessage as M, ClientGUITopLevelWindowsPanels as W
        from hydrus.client.networking import ClientNetworkingURLClass as U
        from hydrus.client.parsing import ClientParsing as P
        from hydrus.core import HydrusConstants as HC
        cls = U.URLClass(name='direct',url_type=HC.URL_TYPE_POST,url_domain_mask=U.URLDomainMask(raw_domains=['links.example']),path_components=[(S.StringMatch(match_type=S.STRING_MATCH_FIXED,match_value='direct'),None)],parameters=[],example_url='https://links.example/direct')
        parsers = [P.PageParser(name=name,example_urls=[url]) for name,url in [('z matching','https://links.example/direct'),('a other','https://other.example/no'),('b matching','https://links.example/direct')]]
        names = {p.GetParserKey():p.GetName() for p in parsers}
        panel = G.EditURLClassLinksPanel(c.gui,c.network_engine,[cls],parsers,{cls.GetClassKey():parsers[0].GetParserKey()})
        panel._parser_list_ctrl.SelectDatas(panel._parser_list_ctrl.GetData(),deselect_others=True)
        out = {'classes':[cls.GetSerialisableTuple()],'parsers':[p.GetSerialisableTuple() for p in parsers],'steps':[]}
        current = []
        class Dialog(W.DialogEdit):
            def exec(self):
                box = self._panel._list
                row = {'title':self.windowTitle(),'choices':[box.item(i).text() for i in range(box.count())],'initial':[i for i in range(box.count()) if box.item(i).isSelected()]}
                def act():
                    box.clearSelection();box.item(current[0]).setSelected(True)
                    (self._apply if current[1] else self._cancel).click()
                QC.QTimer.singleShot(0,act)
                result = super().exec()
                row['accepted'] = result == QW.QDialog.DialogCode.Accepted
                out['steps'].append(row)
                return result
        original = W.DialogEdit
        W.DialogEdit = Dialog
        try:
            for index,yes in [(0,False),(2,True),(0,True),(3,True)]:
                current[:] = [index,yes]
                panel._EditParser()
                out['steps'][-1]['value'] = names[next(iter(panel.GetValue().values()))]
        finally: W.DialogEdit = original
        out['clear'] = []
        original_yes = Q.GetYesNo
        try:
            for yes in [False,True]:
                def answer(parent,text,**kw):
                    out['clear'].append({'question':text,'yes':yes})
                    return QW.QDialog.DialogCode.Accepted if yes else QW.QDialog.DialogCode.Rejected
                Q.GetYesNo = answer
                panel._ClearParser()
                value = panel.GetValue()
                out['clear'][-1]['value'] = names.get(value.get(cls.GetClassKey()))
                out['clear'][-1]['enabled'] = panel._LinksOnCurrentSelection()
        finally: Q.GetYesNo = original_yes
        empty = G.EditURLClassLinksPanel(c.gui,c.network_engine,[cls],[],{})
        empty._parser_list_ctrl.SelectDatas(empty._parser_list_ctrl.GetData(),deselect_others=True)
        original_warning = M.ShowWarning
        try:
            M.ShowWarning = lambda parent,text,**kw: out.update(warning=text)
            empty._EditParser()
        finally: M.ShowWarning = original_warning
        panel.deleteLater();empty.deleteLater()
        return out
    return c.CallBlockingToQt(c.gui,qt)


def main():
    import hydrus_driver
    if len(sys.argv)>1 and sys.argv[1]=='--child':
        import record_api
        output=sys.argv[2]
        value=hydrus_driver.run_client(record_api.unpack_fixture('basic'),record)
        with open(output,'w') as f:json.dump(value,f)
        return
    with tempfile.TemporaryDirectory() as tmp:
        path=os.path.join(tmp,'out.json');hydrus_driver.run_in_subprocess(os.path.abspath(__file__),'--child',path)
        with open(path) as f:value=json.load(f)
    with open(os.path.join(HERE,'fixtures','parser_link_picker.json'),'w') as f:json.dump(value,f,indent=1);f.write('\n')
    print('recorded owned parser selector and clear confirmation')


if __name__=='__main__':main()

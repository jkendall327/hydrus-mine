#!/usr/bin/env python3
"""Record real Qt subsidiary list clipboard imports/exports and reference PNG.

The wrappers include recursive children, separation/sort metadata, notes with
blank lines, converter example text and inert editor context. No HTTP requests.
"""
import json
import os
import sys
import tempfile
HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)


def record(session):
    c = session.controller
    def work():
        from qtpy import QtWidgets as QW
        from hydrus.client.gui.parsing import ClientGUIParsing as G
        from hydrus.client.parsing import ClientParsing as P
        from hydrus.client.gui import ClientGUIDialogsMessage as M, ClientGUIDialogsQuick as D
        from hydrus.client import ClientSerialisable as PNG, ClientStrings as CS
        from hydrus.core import HydrusConstants as H, HydrusSerialisable as S
        note = P.ContentParser(name='note', content_type=H.CONTENT_TYPE_NOTES,
            formula=P.ParseFormulaStatic(static_text='first\n\nnote'), additional_info='imported note')
        leaf = P.SubsidiaryPageParser(formula=P.ParseFormulaStatic(static_text='leaf'),
            page_parser=P.PageParser(name='leaf', parser_key=bytes.fromhex('33'*32), content_parsers=[note]))
        a = P.SubsidiaryPageParser(formula=P.ParseFormulaStatic(static_text='<p>first</p>'), sort_posts_by_source_time=True,
            page_parser=P.PageParser(name='alpha', parser_key=bytes.fromhex('11'*32), content_parsers=[note],
                subsidiary_page_parsers=[leaf], string_converter=CS.StringConverter(example_string='sample 日本'),
                example_parsing_context={'token':'inert retained', 'url':'https://subsidiary-exchange.example/alpha'}))
        b = P.SubsidiaryPageParser(formula=P.ParseFormulaStatic(static_text='<p>second</p>'),
            page_parser=P.PageParser(name='beta', parser_key=bytes.fromhex('22'*32), content_parsers=[note]))
        panel = G.EditPageParserPanel(c.gui, P.PageParser(name='parent', subsidiary_page_parsers=[b,a]))
        QW.QApplication.processEvents()
        listing = panel._subsidiary_page_parsers
        exchange = listing.parentWidget()
        old_pub, old_clip, old_info, old_warning = c.pub, c.GetClipboardText, M.ShowInformation, M.ShowWarning
        old_yes = D.GetYesNo
        copied, messages = [], []
        c.pub = lambda *args, **kwargs: copied.append(list(args)) if args and args[0] == 'clipboard' else None
        M.ShowInformation = lambda owner, text: messages.append(['information',text])
        M.ShowWarning = lambda owner, text: messages.append(['warning',text])
        try:
            listing.SelectDatas([a])
            exchange._ExportToClipboard()
            out = {'single':json.loads(copied[-1][2])}
            listing.SelectDatas([a,b])
            exchange._ExportToClipboard()
            out['bundle'] = json.loads(copied[-1][2])
            listing.DeleteDatas([a,b])
            c.GetClipboardText = lambda: json.dumps(out['bundle'])
            exchange._ImportFromClipboard()
            out['imported_names'] = [x.GetPageParser().GetName() for x in listing.GetData()]
            out['selected_names'] = [x.GetPageParser().GetName() for x in listing.GetData(only_selected=True)]
            out['imported'] = [x.GetSerialisableTuple() for x in listing.GetData()]
            out['parsed'] = [[content.parsed_text for content in post.parsed_contents] for post in panel.GetValue().Parse({'url':'https://subsidiary-exchange.example/parent'},'parent document')]
            # The reference duplicate button retains the complete wrapper/key.
            exchange._Duplicate()
            out['duplicated_names'] = [x.GetPageParser().GetName() for x in listing.GetData()]
            out['duplicate_selected_names'] = [x.GetPageParser().GetName() for x in listing.GetData(only_selected=True)]
            out['deletes'] = []
            for accepted in [False,True]:
                def answer(owner,text):
                    out['deletes'].append({'question':text,'accepted':accepted})
                    return QW.QDialog.DialogCode.Accepted if accepted else QW.QDialog.DialogCode.Rejected
                D.GetYesNo = answer
                listing.ProcessDeleteAction()
                out['deletes'][-1]['names'] = [x.GetPageParser().GetName() for x in listing.GetData()]
            out['messages'] = messages
            bundle = S.CreateFromSerialisableTuple(out['bundle'])
            path = os.path.join(HERE,'fixtures/subsidiary_exchange.png')
            PNG.DumpToPNG(512,bundle.DumpToNetworkBytes(),'subsidiary oracle','recursive wrappers','synthetic parser children',path)
            out['png_loaded'] = json.loads(S.CreateFromNetworkBytes(PNG.LoadFromPNG(path)).DumpToString())
            for notebook in panel.findChildren(QW.QTabWidget):
                for i in range(notebook.count()):
                    if notebook.tabText(i) == 'subsidiary page parsers': notebook.setCurrentIndex(i)
            panel.resize(1600,900); panel.show(); QW.QApplication.processEvents()
            panel.grab().save(os.path.join(HERE,'fixtures/subsidiary_exchange_editor.png'))
        finally:
            c.pub, c.GetClipboardText, M.ShowInformation, M.ShowWarning = old_pub, old_clip, old_info, old_warning
            D.GetYesNo = old_yes
            panel.deleteLater()
        return out
    return c.CallBlockingToQt(c.gui, work)


def main():
    import hydrus_driver
    if len(sys.argv)>1 and sys.argv[1]=='--child':
        import record_api
        output=sys.argv[2]
        result=hydrus_driver.run_client(record_api.unpack_fixture('basic'),record)
        with open(output,'w') as f: json.dump(result,f)
        return
    with tempfile.TemporaryDirectory() as tmp:
        output=os.path.join(tmp,'result.json')
        hydrus_driver.run_in_subprocess(os.path.abspath(__file__),'--child',output)
        with open(output) as f: result=json.load(f)
    with open(os.path.join(HERE,'fixtures/subsidiary_exchange.json'),'w') as f:
        json.dump(result,f,indent=1,ensure_ascii=False);f.write('\n')
    print('wrote subsidiary_exchange.json')
if __name__=='__main__':main()

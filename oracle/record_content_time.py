#!/usr/bin/env python3
"""Record actual timestamp content-editor choices, saved metadata and extraction."""
import json,os,sys,tempfile
HERE=os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0,HERE)

def record(session):
    def work():
        from hydrus.client.gui.parsing import ClientGUIParsing as G
        from hydrus.client.parsing import ClientParsing as P
        from hydrus.core import HydrusConstants as H, HydrusTime as HT
        from hydrus.client import ClientStrings as S
        from qtpy import QtWidgets as QW
        context={'url':'https://source-time.example/post/1'}
        now=1800000000
        cases=[]
        for info in [H.TIMESTAMP_TYPE_MODIFIED_DOMAIN,None,7,"datestring"]:
            formula=P.ParseFormulaHTML(tag_rules=[P.ParseRuleHTML(tag_name='p')],content_to_fetch=P.HTML_CONTENT_STRING)
            if info=="datestring":
                processor=S.StringProcessor()
                processor.SetProcessingSteps([S.StringConverter(conversions=[(S.STRING_CONVERSION_DATE_DECODE,("%Y-%m-%d",H.TIMEZONE_UTC,0))])])
                formula.SetStringProcessor(processor)
            parser=P.ContentParser(name='source timestamp',content_type=H.CONTENT_TYPE_TIMESTAMP,formula=formula,additional_info=0 if info=="datestring" else info)
            panel=G.EditContentParserPanel(session.controller.gui,parser,P.ParsingTestData(context,['<p>1700000000</p>']),[H.CONTENT_TYPE_TIMESTAMP])
            value=panel.GetValue()
            item={'input_type':info,'choices':[(panel._timestamp_type.itemText(i),panel._timestamp_type.itemData(i)) for i in range(panel._timestamp_type.count())],'selected':panel._timestamp_type.currentIndex(),'tuple':value.GetSerialisableTuple(),'cases':[]}
            documents=['<p>2023-11-14</p><p>2020-09-13</p>','<p>2099-01-01</p>','<p>invalid date</p>'] if info=="datestring" else ['<p>1700000000</p><p>1600000000</p>','<p>2000000000</p>','<p>0</p>','<p>-1</p>','<p>not a time</p>']
            for document in documents:
                panel._test_panel._SetExampleData(document);panel._test_panel.TestParse()
                post=value.Parse(dict(context),document)
                old_now=HT.GetNow
                HT.GetNow=lambda:now
                try:source_time=post.GetTimestamp(H.TIMESTAMP_TYPE_MODIFIED_DOMAIN)
                finally:HT.GetNow=old_now
                item['cases'].append({'document':document,'texts':[c.parsed_text for c in post.parsed_contents],'metadata':[{'name':c.parsed_content_description.name,'timestamp_type':c.parsed_content_description.timestamp_type,'description':c.parsed_content_description.GetShorthandContentSpecificInfoString()} for c in post.parsed_contents],'source_time':source_time,'preview':panel._test_panel._results.toPlainText()})
            cases.append(item)
            if info==0:
                panel.resize(900,700);panel.show();QW.QApplication.processEvents();panel.grab().save(os.path.join(HERE,'fixtures/content_time_editor.png'))
            panel.hide();panel.deleteLater()
        return {'timestamp_type':H.TIMESTAMP_TYPE_MODIFIED_DOMAIN,'now':now,'cases':cases}
    return session.controller.CallBlockingToQt(session.controller.gui,work)

def main():
    import hydrus_driver
    if len(sys.argv)>1 and sys.argv[1]=='--child':
        import record_api
        output=sys.argv[2];result=hydrus_driver.run_client(record_api.unpack_fixture('basic'),record)
        with open(output,'w') as f:json.dump(result,f)
        return
    with tempfile.TemporaryDirectory() as directory:
        output=os.path.join(directory,'result.json');hydrus_driver.run_in_subprocess(os.path.abspath(__file__),'--child',output)
        with open(output) as f:result=json.load(f)
    with open(os.path.join(HERE,'fixtures/content_time.json'),'w') as f:json.dump(result,f,indent=1,ensure_ascii=False);f.write('\n')
    print('wrote content_time.json')
if __name__=='__main__':main()

#!/usr/bin/env python3
"""Record real Qt router example tables and processor child data from generated files.
No exports are run. Paths are replaced with <examples> for portable replay.
"""
import json
import os
import sys
import tempfile
HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)


def record(session):
    def qt():
        from qtpy import QtWidgets as QW
        from hydrus.client.gui.metadata import ClientGUIMetadataMigration as G, ClientGUIMetadataMigrationTest as T
        from hydrus.client.metadata import ClientMetadataMigration as R, ClientMetadataMigrationImporters as I, ClientMetadataMigrationExporters as E
        from hydrus.client import ClientStrings as S
        from hydrus.client.parsing import ClientParsing as P
        from hydrus.core import HydrusConstants as H
        with tempfile.TemporaryDirectory() as directory:
            paths = [os.path.join(directory, name) for name in ['one.png', 'two.png', 'missing.png']]
            documents = {'one.png.txt': 'item10\nitem2\nitem2\n', 'two.png.txt': 'next\n', 'one.png.json': '["json2","json1"]', 'two.png.json': '["next json"]'}
            for name, text in documents.items():
                with open(os.path.join(directory, name), 'w') as f: f.write(text)
            def processor(steps):
                value=S.StringProcessor();value.SetProcessingSteps(steps);return value
            source_processor = processor([S.StringConverter(conversions=[(S.STRING_CONVERSION_PREPEND_TEXT, 'source:')])])
            sources = [I.SingleFileMetadataImporterTXT(string_processor=source_processor), I.SingleFileMetadataImporterJSON()]
            factory = T.MigrationTestContextFactorySidecar(paths)
            router = R.SingleFileMetadataRouter(importers=sources, exporter=E.SingleFileMetadataExporterMediaURLs())
            panel = G.EditSingleFileMetadataRouterPanel(session.controller.gui, router, [I.SingleFileMetadataImporterTXT,I.SingleFileMetadataImporterJSON], [E.SingleFileMetadataExporterMediaURLs],factory)
            output = []
            def snapshot(name):
                panel._UpdateTestPanel()
                tables=[]
                for i, source in enumerate(panel._importers_list.GetData()):
                    tables.append({'source':i,'rows':[list(panel._ConvertTestRowToDisplayTuple((source,path))) for path in factory.GetTestObjects()]})
                data=panel._GetExampleStringProcessorTestData()
                output.append({'case':name,'tuple':panel._GetValue().GetSerialisableTuple(),'tables':tables,'processor_texts':data.texts,'processor_context':data.parsing_context,'documents':dict(documents)})
            snapshot('human_sort')
            panel._string_processor_button.SetValue(S.StringProcessor())
            snapshot('no_changes')
            panel._string_processor_button.SetValue(processor([S.StringConverter(conversions=[(S.STRING_CONVERSION_APPEND_TEXT,'!')])]))
            snapshot('append')
            documents['one.png.json']='<html>not JSON</html>'
            with open(os.path.join(directory,'one.png.json'),'w') as f:f.write(documents['one.png.json'])
            snapshot('html_in_json')
            documents['one.png.json']='["json2","json1"]'
            with open(os.path.join(directory,'one.png.json'),'w') as f:f.write(documents['one.png.json'])
            panel._importers_list.Clear()
            snapshot('no_sources')
            panel._importers_list.AddDatas(sources)
            factory.SetExampleFilePaths([])
            snapshot('no_examples')
            factory.SetExampleFilePaths(paths)
            panel._UpdateTestPanel();panel.resize(1250,800);panel.show();QW.QApplication.processEvents()
            panel.grab().save(os.path.join(HERE,'fixtures','sidecar_testing.png'))
            panel.deleteLater()
            from hydrus.client.media import ClientMediaManagers as M, ClientMediaResult as MR
            media_inputs=[{'hash':'11'*32,'urls':['https://sidecars.example/item/2','https://sidecars.example/item/1'],'notes':{'note: name':'first\n\nnote'}},{'hash':'22'*32,'urls':['https://sidecars.example/next'],'notes':{}}]
            media_objects=[]
            for i,item in enumerate(media_inputs):
                times=M.TimesManager()
                media_objects.append(MR.MediaResult(M.FileInfoManager(i+1,bytes.fromhex(item['hash'])),M.TagsManager({},{}),times,M.LocationsManager(set(),set(),set(),set(),times,urls=set(item['urls'])),M.RatingsManager({}),M.NotesManager(item['notes']),M.FileViewingStatsManager(times,[])))
            factory=T.MigrationTestContextFactoryMedia(media_objects)
            sources=[I.SingleFileMetadataImporterMediaURLs(),I.SingleFileMetadataImporterMediaNotes()]
            router=R.SingleFileMetadataRouter(importers=sources,exporter=E.SingleFileMetadataExporterTXT())
            panel=G.EditSingleFileMetadataRouterPanel(session.controller.gui,router,[I.SingleFileMetadataImporterMediaURLs,I.SingleFileMetadataImporterMediaNotes],[E.SingleFileMetadataExporterTXT],factory)
            snapshot('media')
            output[-1]['media']=media_inputs
            panel.deleteLater()
            return json.loads(json.dumps(output).replace(directory,'<examples>'))
    return session.controller.CallBlockingToQt(session.controller.gui,qt)


def child(path):
    import hydrus_driver
    import record_api
    result=hydrus_driver.run_client(record_api.unpack_fixture('basic'),record)
    with open(path,'w') as f: json.dump(result,f,indent=2)


def main():
    if len(sys.argv)>1 and sys.argv[1]=='--child': child(sys.argv[2]);return
    import hydrus_driver
    with tempfile.TemporaryDirectory() as directory:
        path=os.path.join(directory,'result.json')
        hydrus_driver.run_in_subprocess(os.path.abspath(__file__),'--child',path)
        with open(path) as f: result=json.load(f)
    with open(os.path.join(HERE,'fixtures','sidecar_testing.json'),'w') as f: json.dump(result,f,indent=2);f.write('\n')
    print('recorded',len(result),'sidecar example panel states')


if __name__=='__main__':main()

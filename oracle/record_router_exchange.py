#!/usr/bin/env python3
"""Record real Qt router queue imports/exports, context vetoes and a written sidecar.

Covers all six source/destination variants, selected queue duplication, real PNG
load, old versions, processors, filename converter examples and JSON formulae.
Media is synthetic and generated documents use reserved synthetic domains.
"""
import json
import os
import sys
import tempfile
HERE=os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0,HERE)


def record(session):
    c=session.controller
    def work():
        from qtpy import QtWidgets as QW
        from hydrus.client.gui.metadata import ClientGUIMetadataMigration as G, ClientGUIMetadataMigrationTest as T
        from hydrus.client.gui import ClientGUIDialogsMessage as D
        from hydrus.client.metadata import ClientMetadataMigration as R, ClientMetadataMigrationImporters as I, ClientMetadataMigrationExporters as E
        from hydrus.client import ClientConstants as C, ClientStrings as S, ClientTime as CT, ClientSerialisable as PNG
        from hydrus.core import HydrusConstants as H, HydrusSerialisable as HS
        from hydrus.client.parsing import ClientParsing as P
        def processor(steps):
            p=S.StringProcessor();p.SetProcessingSteps(steps);return p
        tag_key=c.services_manager.GetDefaultLocalTagService().GetServiceKey()
        convert=S.StringConverter(conversions=[(S.STRING_CONVERSION_APPEND_TEXT,'_named')],example_string='filename 日本.png.json')
        txt=I.SingleFileMetadataImporterTXT(suffix='notes',separator='||||',filename_string_converter=convert)
        js=I.SingleFileMetadataImporterJSON(json_parsing_formula=P.ParseFormulaJSON(name='retained JSON formula'),remove_actual_filename_ext=True,suffix='metadata')
        imports=[R.SingleFileMetadataRouter(importers=[txt,js],exporter=destination) for destination in [
            E.SingleFileMetadataExporterMediaURLs(),E.SingleFileMetadataExporterMediaNotes(forced_name='collected'),
            E.SingleFileMetadataExporterMediaTags(service_key=tag_key),
            E.SingleFileMetadataExporterMediaTimestamps(timestamp_data_stub=CT.TimestampData(timestamp_type=H.TIMESTAMP_TYPE_MODIFIED_DOMAIN,location='router-exchange.example'))]]
        all_media=[I.SingleFileMetadataImporterMediaTags(service_key=C.COMBINED_TAG_SERVICE_KEY,tag_display_type=1),I.SingleFileMetadataImporterMediaNotes(),
            I.SingleFileMetadataImporterMediaURLs(),I.SingleFileMetadataImporterMediaTimestamps(timestamp_data_stub=CT.TimestampData(timestamp_type=H.TIMESTAMP_TYPE_LAST_VIEWED,location=C.CANVAS_PREVIEW))]
        match=S.StringMatch(match_type=S.STRING_MATCH_REGEX,match_value=r'^https://router-exchange[.]example/')
        only_urls=I.SingleFileMetadataImporterMediaURLs(string_processor=processor([match]))
        append=processor([S.StringConverter(conversions=[(S.STRING_CONVERSION_APPEND_TEXT,'!')])])
        exports=[R.SingleFileMetadataRouter(importers=all_media,exporter=E.SingleFileMetadataExporterJSON(remove_actual_filename_ext=True,suffix='metadata',nested_object_names=['results'],filename_string_converter=convert)),
            R.SingleFileMetadataRouter(importers=[only_urls],string_processor=append,exporter=E.SingleFileMetadataExporterTXT(suffix='urls',separator='||||'))]
        allowed_i=[I.SingleFileMetadataImporterTXT,I.SingleFileMetadataImporterJSON]
        allowed_e=[E.SingleFileMetadataExporterMediaTags,E.SingleFileMetadataExporterMediaNotes,E.SingleFileMetadataExporterMediaURLs,E.SingleFileMetadataExporterMediaTimestamps]
        media_i=[I.SingleFileMetadataImporterMediaTags,I.SingleFileMetadataImporterMediaNotes,I.SingleFileMetadataImporterMediaURLs,I.SingleFileMetadataImporterMediaTimestamps]
        sidecar_e=[E.SingleFileMetadataExporterTXT,E.SingleFileMetadataExporterJSON]
        controls=[G.SingleFileMetadataRoutersControl(c.gui,[],allowed_i,allowed_e,T.MigrationTestContextFactorySidecar([])),
            G.SingleFileMetadataRoutersControl(c.gui,[],media_i,sidecar_e,T.MigrationTestContextFactoryMedia([]))]
        out={'imports':[r.GetSerialisableTuple() for r in imports],'exports':[r.GetSerialisableTuple() for r in exports], 'vetoes':[],'messages':[],'queues':[]}
        old_pub,old_clip,old_warn,old_info=c.pub,c.GetClipboardText,D.ShowWarning,D.ShowInformation
        copies=[]
        c.pub=lambda *args,**kwargs: copies.append(list(args)) if args and args[0]=='clipboard' else None
        D.ShowWarning=lambda owner,text: out['messages'].append(['warning',text])
        D.ShowInformation=lambda owner,text: out['messages'].append(['information',text])
        try:
            for context,control,routers in [('import',controls[0],imports),('export',controls[1],exports)]:
                bundle=HS.SerialisableList(routers)
                c.GetClipboardText=lambda: bundle.DumpToString()
                control._ImportFromClipboard()
                control._ExportToClipboard()
                row={'context':context,'text':json.loads(copies[-1][2]),'count':len(control.GetData()),'selected':len(control.GetData(only_selected=True))}
                control._Duplicate()
                row['duplicate_count']=len(control.GetData());row['duplicate_selected']=len(control.GetData(only_selected=True));out['queues'].append(row)
                wrong=exports[0] if context=='import' else imports[0]
                try:control._CheckImportObjectCustom(wrong)
                except Exception as error:out['vetoes'].append({'context':context,'error':str(error)})
                control._ImportObject(wrong)
            # Direction-compatible destination with an incompatible source.
            wrong=R.SingleFileMetadataRouter(importers=[txt],exporter=E.SingleFileMetadataExporterTXT())
            try:controls[1]._CheckImportObjectCustom(wrong)
            except Exception as error:out['vetoes'].append({'context':'export_source','error':str(error)})
            controls[1]._ImportObject(wrong)
            bundle=HS.SerialisableList(imports+exports)
            path=os.path.join(HERE,'fixtures/router_exchange.png')
            PNG.DumpToPNG(512,bundle.DumpToNetworkBytes(),'router oracle','metadata queues','synthetic metadata',path)
            out['bundle']=json.loads(bundle.DumpToString());out['png_loaded']=json.loads(HS.CreateFromNetworkBytes(PNG.LoadFromPNG(path)).DumpToString())
            old=json.loads(exports[0].DumpToString());old[1]=2
            out['old']={'source':old,'upgraded':json.loads(HS.CreateFromSerialisableTuple(old).DumpToString())}
            from hydrus.client.media import ClientMediaManagers as M, ClientMediaResult as MR
            url='https://router-exchange.example/source'
            times=M.TimesManager()
            media=MR.MediaResult(M.FileInfoManager(1,bytes.fromhex('11'*32)),M.TagsManager({},{}),times,M.LocationsManager(set(),set(),set(),set(),times,urls={url,'https://other.example/ignored'}),M.RatingsManager({}),M.NotesManager({}),M.FileViewingStatsManager(times,[]))
            with tempfile.TemporaryDirectory() as directory:
                target=os.path.join(directory,'sample.png')
                worked=exports[1].Work(media,target)
                with open(target+'.urls.txt') as f:data=f.read()
                out['consumer']={'url':url,'worked':worked,'sidecar':data}
            controls[1].resize(1000,480);controls[1].show();QW.QApplication.processEvents();controls[1].grab().save(os.path.join(HERE,'fixtures/router_exchange_editor.png'))
        finally:
            c.pub,c.GetClipboardText,D.ShowWarning,D.ShowInformation=old_pub,old_clip,old_warn,old_info
            for control in controls:control.deleteLater()
        return out
    return c.CallBlockingToQt(c.gui,work)


def main():
    import hydrus_driver
    if len(sys.argv)>1 and sys.argv[1]=='--child':
        import record_api
        output=sys.argv[2];result=hydrus_driver.run_client(record_api.unpack_fixture('basic'),record)
        with open(output,'w') as f:json.dump(result,f)
        return
    with tempfile.TemporaryDirectory() as tmp:
        output=os.path.join(tmp,'result.json');hydrus_driver.run_in_subprocess(os.path.abspath(__file__),'--child',output)
        with open(output) as f:result=json.load(f)
    with open(os.path.join(HERE,'fixtures/router_exchange.json'),'w') as f:json.dump(result,f,indent=1,ensure_ascii=False);f.write('\n')
    print('wrote router_exchange.json')
if __name__=='__main__':main()

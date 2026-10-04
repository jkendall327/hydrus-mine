#!/usr/bin/env python3
"""Record actual Qt subscription-list import of legacy type3 versions8–10
and legacy query versions1–3.

Real legacy subscriptions carry both complete URL histories and old import
options. The list converts them, assigns fresh log names and exports current
container90 objects. A normalised current tag-options export is also accepted
by the actual list to prove the native encoder's upgraded carrier shape.
"""
import json
import os
import sys
import tempfile
HERE=os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0,HERE)
NOW=1_700_000_000


def record(session):
    c=session.controller
    def work():
        from qtpy import QtWidgets as QW
        from hydrus.core import HydrusSerialisable as S, HydrusTime as T
        from hydrus.client.gui import ClientGUISubscriptions as G, ClientGUIDialogsMessage as M
        from hydrus.client.importing import ClientImportSubscriptionLegacy as L, ClientImportSubscriptionQuery as Q, ClientImportSubscriptions as Subs
        from hydrus.client.importing.options import TagImportOptionsLegacy
        original=json.load(open(os.path.join(HERE,'fixtures/subscription_exchange.json')))['single']
        counter=[0]
        old_generate,old_now=Q.GenerateQueryLogContainerName,T.GetNow
        def fresh():
            counter[0]+=1
            return f'{counter[0]:064x}'
        Q.GenerateQueryLogContainerName=fresh;T.GetNow=lambda:NOW
        legacy=L.SubscriptionLegacy('Legacy artist',gug_key_and_name=(bytes.fromhex('11'*32),'synthetic gallery'))
        legacy._paused=True
        legacy._initial_file_limit=None
        legacy._periodic_file_limit=17
        legacy._no_work_until=NOW+600
        legacy._no_work_until_reason='old reason discarded by real conversion'
        legacy._show_a_popup_while_working=False
        legacy._publish_files_to_popup_button=False
        legacy._publish_files_to_page=True
        legacy._publish_label_override='legacy label'
        legacy._merge_query_publish_events=False
        query=L.SubscriptionQueryLegacy('legacy blue')
        query._display_name='legacy display'
        query._last_check_time=NOW-3600
        query._next_check_time=NOW+5
        history=original[2][1][2][0][1][3]
        query._gallery_seed_log=S.CreateFromSerialisableTuple(history[0])
        query._file_seed_cache=S.CreateFromSerialisableTuple(history[1])
        legacy._queries=[query]
        source=json.loads(legacy.DumpToString())
        old_pub,old_clip,old_image=c.pub,c.GetClipboardText,c.ClipboardHasImage
        old_info,old_warning=M.ShowInformation,M.ShowWarning
        copied=[];messages=[]
        c.pub=lambda *args,**kw:copied.append(list(args)) if args and args[0]=='clipboard' else None
        c.ClipboardHasImage=lambda:False
        M.ShowInformation=lambda owner,text:messages.append(text)
        M.ShowWarning=lambda owner,text:messages.append(text)
        cases=[]
        try:
            for version,query_version in [(8,3),(9,3),(10,3),(10,2),(10,1)]:
                incoming=json.loads(json.dumps(source));incoming[2]=version
                legacy_query=incoming[3][1][0]
                legacy_query[1]=query_version
                if query_version<=2:
                    legacy_query[2].pop(9);legacy_query[2].pop(1)
                if query_version==1:legacy_query[2].pop(6)
                if version==9: incoming[3].pop(13)
                if version==8:
                    incoming[3].pop(13);incoming[3].pop(10)
                converted,converted_logs=L.ConvertLegacySubscriptionToNew(S.CreateFromSerialisableTuple(incoming))
                direct=Subs.SubscriptionContainer();direct.subscription=converted;direct.query_log_containers=S.SerialisableList(converted_logs)
                direct_normalised=json.loads(direct.DumpToString())
                for header in direct_normalised[2][0][3][1]:
                    old=S.CreateFromSerialisableTuple(header[2][12])
                    header[2][12]=old.GetTagImportOptions().GetSerialisableTuple()
                panel=G.EditSubscriptionsPanel(c.gui,[])
                try:
                    c.GetClipboardText=lambda:json.dumps(incoming)
                    panel._subscriptions_panel._ImportFromClipboard()
                    listing=panel._subscriptions
                    assert len(listing.GetData())==1
                    listing.SelectDatas(listing.GetData());panel._subscriptions_panel._ExportToClipboard()
                    upgraded=json.loads(copied[-1][2])
                    # The legacy converter stores old tag options in a current
                    # header. Upgrade that inert tag object exactly through its
                    # real migration API, then import/export the native shape.
                    normalised=json.loads(json.dumps(upgraded))
                    for header in normalised[2][0][3][1]:
                        old=S.CreateFromSerialisableTuple(header[2][12])
                        assert isinstance(old,TagImportOptionsLegacy.TagImportOptionsLegacy)
                        header[2][12]=old.GetTagImportOptions().GetSerialisableTuple()
                    newpanel=G.EditSubscriptionsPanel(c.gui,[])
                    try:
                        c.GetClipboardText=lambda:json.dumps(normalised)
                        newpanel._subscriptions_panel._ImportFromClipboard()
                        assert newpanel._subscriptions.GetData()[0].GetName()=='Legacy artist'
                    finally: newpanel.deleteLater()
                    cases.append({'version':version,'query_version':query_version,'source':incoming,'upgraded':upgraded,'normalised':normalised,'direct_normalised':direct_normalised,'native_shape_accepted':True})
                finally: panel.deleteLater()
            return {'now':NOW,'cases':cases,'messages':messages}
        finally:
            c.pub,c.GetClipboardText,c.ClipboardHasImage=old_pub,old_clip,old_image
            M.ShowInformation,M.ShowWarning=old_info,old_warning
            Q.GenerateQueryLogContainerName,T.GetNow=old_generate,old_now
    return c.CallBlockingToQt(c.gui,work)


def main():
    import hydrus_driver
    if len(sys.argv)>1 and sys.argv[1]=='--child':
        output=sys.argv[2]
        import record_api
        result=hydrus_driver.run_client(record_api.unpack_fixture('basic'),record)
        with open(output,'w') as f: json.dump(result,f,ensure_ascii=False,indent=2)
        return
    with tempfile.TemporaryDirectory() as work:
        output=os.path.join(work,'result.json')
        hydrus_driver.run_in_subprocess(os.path.abspath(__file__),'--child',output)
        with open(output) as f: result=json.load(f)
        with open(os.path.join(HERE,'fixtures/subscription_legacy_exchange.json'),'w') as f:json.dump(result,f,ensure_ascii=False,indent=2)

if __name__=='__main__':main()

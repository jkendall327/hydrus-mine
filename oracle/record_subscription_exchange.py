#!/usr/bin/env python3
"""Record actual Qt subscription list clipboard exchange and full history PNG.

Paused synthetic subscriptions include file/gallery histories, headers and cached
velocity/example-seed metadata. Imports record casefold name collisions, fresh log
identities, missing-log confirmation, invalid clipboard and wrong/future types.
"""
import json
import os
import sys
import tempfile
HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
NOW = 1_700_000_000


def record(session):
    c = session.controller
    def work():
        from qtpy import QtWidgets as QW
        from hydrus.core import HydrusTime as T, HydrusSerialisable as S
        from hydrus.client.gui import ClientGUISubscriptions as G, ClientGUIDialogsMessage as M, ClientGUIDialogsQuick as D, ClientGUIDialogsFiles as FD
        from hydrus.client.importing import ClientImportSubscriptions as Subs, ClientImportSubscriptionQuery as Query, ClientImportFileSeeds as Files, ClientImportGallerySeeds as Galleries
        from hydrus.client import ClientSerialisable as PNG
        from hydrus.client.gui.widgets import ClientGUIMenuButton as MB
        old_now, old_generate = T.GetNow, Query.GenerateQueryLogContainerName
        counter = [0]
        def fresh():
            counter[0] += 1
            return f'{counter[0]:064x}'
        T.GetNow = lambda: NOW
        Query.GenerateQueryLogContainerName = fresh
        sub = Subs.Subscription('Artist', gug_key_and_name=(bytes.fromhex('11'*32), 'synthetic gallery'))
        sub.SetPaused(True)
        header = Query.SubscriptionQueryHeader()
        header.SetQueryText('blue_eyes')
        header.SetDisplayName('blue artist')
        header.SetLastCheckTime(NOW - 3600)
        log = Query.SubscriptionQueryLogContainer(header.GetQueryLogContainerName())
        seed = Files.FileSeed(Files.FILE_SEED_TYPE_URL, 'https://subscription-exchange.example/file/1')
        # Populate exact current tuple fields rather than relying on mutable setters.
        raw = list(seed.GetSerialisableTuple()); info = list(raw[2])
        info[3:8] = [NOW-7200, NOW-3600, NOW-8000, 7, 'ignored\nrecorded reason']
        info[8:17] = ['https://subscription-exchange.example/gallery', {'X-Test':'history'}, ['external:blue'], [77,1,[['22'*32,['service:blue']]]], ['https://subscription-exchange.example/post/1'], ['https://subscription-exchange.example/source/1'], ['artist:blue'], [['note','first\n\nsecond']], [['sha256','33'*32]]]
        raw[2] = info
        seed = S.CreateFromSerialisableTuple(raw)
        cache = Files.FileSeedCache(); cache.AddFileSeeds([seed]); log.SetFileSeedCache(cache)
        gallery = Galleries.GallerySeed('https://subscription-exchange.example/gallery/1')
        raw = list(gallery.GetSerialisableTuple()); info = list(raw[2])
        info[1:] = [False, ['gallery:blue'], [77,1,[['22'*32,['service:gallery']]]], NOW-9000, NOW-7000, 4, 'gallery failure', 'https://subscription-exchange.example/referral', {'X-Test':'gallery'}]
        raw[2] = info
        log.GetGallerySeedLog().AddGallerySeeds([S.CreateFromSerialisableTuple(raw)])
        header.SyncToQueryLogContainer(sub.GetCheckerOptions(), log)
        sub.SetQueryHeaders([header])
        panel = G.EditSubscriptionsPanel(c.gui, [sub])
        listing = panel._subscriptions; exchange = panel._subscriptions_panel
        panel._names_to_edited_query_log_containers[log.GetName()] = log
        current = listing.GetData()[0]
        old_pub, old_clip, old_image = c.pub, c.GetClipboardText, c.ClipboardHasImage
        old_info, old_warning, old_critical = M.ShowInformation, M.ShowWarning, M.ShowCritical
        old_yes, old_parse = D.GetYesNo, D.PresentClipboardParseError
        copied, messages, questions = [], [], []
        c.pub = lambda *args, **kw: copied.append(list(args)) if args and args[0]=='clipboard' else None
        c.ClipboardHasImage = lambda: False
        M.ShowInformation = lambda owner,text: messages.append(['information',text])
        M.ShowWarning = lambda owner,text: messages.append(['warning',text])
        M.ShowCritical = lambda owner,title,text: messages.append(['critical',title,text])
        D.PresentClipboardParseError = lambda owner,text,description,error: messages.append(['parse',description])
        accepted = [False]
        def answer(owner,text,title='Are you sure?',yes_label='yes',no_label='no',**kw):
            questions.append({'message':text,'title':title,'yes':yes_label,'no':no_label,'accepted':accepted[0]})
            return QW.QDialog.DialogCode.Accepted if accepted[0] else QW.QDialog.DialogCode.Rejected
        D.GetYesNo = answer
        try:
            listing.SelectDatas([current]); exchange._ExportToClipboard()
            single = json.loads(copied[-1][2]); out = {'single':single}
            out['menus']={button.text():[item.GetTitle() for item in button._menu_template_items] for button in exchange.findChildren(MB.MenuButton) if button.text() in ('export','import')}
            # Actual import through clipboard, preserving all histories but changing identities.
            c.GetClipboardText = lambda: json.dumps(single)
            exchange._ImportFromClipboard()
            out['after_import'] = [s.GetName() for s in listing.GetData()]
            out['selected_after_import'] = [s.GetName() for s in listing.GetData(only_selected=True)]
            listing.SelectDatas(listing.GetData()); exchange._ExportToClipboard()
            out['bundle'] = json.loads(copied[-1][2])
            out['imported_log_names'] = [h.GetQueryLogContainerName() for s in listing.GetData() for h in s.GetQueryHeaders()]
            # Drive the actual JSON file menu actions on a separate empty list.
            with tempfile.TemporaryDirectory() as temp:
                json_path=os.path.join(temp,'subscriptions.json')
                old_dialog=FD.FileDialog
                dialogs=[]
                picker_accept=[True]
                class FileDialog:
                    def __init__(self,owner,title,**kwargs):
                        dialogs.append({'title':title,'default_filename':kwargs.get('default_filename'),'wildcard':kwargs.get('wildcard')})
                    def __enter__(self): return self
                    def __exit__(self,*args): pass
                    def exec(self): return QW.QDialog.DialogCode.Accepted if picker_accept[0] else QW.QDialog.DialogCode.Rejected
                    def GetPath(self): return json_path
                    def GetPaths(self): return [json_path]
                FD.FileDialog=FileDialog
                file_panel=G.EditSubscriptionsPanel(c.gui,[])
                try:
                    exchange._ExportToJSON()
                    with open(json_path) as stream: exported_json=json.load(stream)
                    file_panel._subscriptions_panel._ImportFromJSON()
                    names=[s.GetName() for s in file_panel._subscriptions.GetData()]
                    picker_accept[0]=False
                    file_panel._subscriptions_panel._ImportFromJSON()
                    after_cancel=[s.GetName() for s in file_panel._subscriptions.GetData()]
                    picker_accept[0]=True
                    overwritten=[]
                    for confirm in [False,True]:
                        with open(json_path,'w') as stream: stream.write('previous file')
                        def confirm_overwrite(owner,message,**kwargs):
                            overwritten.append({'question':message.replace(json_path,'{path}'),'accepted':confirm})
                            return QW.QDialog.DialogCode.Accepted if confirm else QW.QDialog.DialogCode.Rejected
                        D.GetYesNo=confirm_overwrite
                        exchange._ExportToJSON()
                        with open(json_path) as stream: content=stream.read()
                        overwritten[-1]['retained_existing'] = content=='previous file'
                    D.GetYesNo=answer
                    out['json_files']={'exported':exported_json,'imported_names':names,'cancel_names':after_cancel,'dialogs':dialogs,'overwrite':overwritten}
                finally:
                    FD.FileDialog=old_dialog
                    D.GetYesNo=answer
                    file_panel.deleteLater()
            broken = json.loads(json.dumps(single)); broken[2][1] = [26,3,[]]
            counts=[]
            for ok in [False,True]:
                accepted[0]=ok; c.GetClipboardText=lambda: json.dumps(broken)
                exchange._ImportFromClipboard(); counts.append(len(listing.GetData()))
            out['missing_log_counts']=counts
            for value in ['not JSON', json.dumps([90,99,single[2]]), json.dumps(single[2][0])]:
                c.GetClipboardText=lambda value=value:value
                exchange._ImportFromClipboard()
            out['invalid_final_count']=len(listing.GetData())
            changes=[]
            for action in ('reset','retry_failed','retry_ignored'):
                incoming=json.loads(json.dumps(single))
                if action=='retry_failed':incoming[2][1][2][0][1][3][1][2][2][0][1][2][6]=4
                testpanel=G.EditSubscriptionsPanel(c.gui,[])
                try:
                    c.GetClipboardText=lambda:json.dumps(incoming)
                    testpanel._subscriptions_panel._ImportFromClipboard()
                    current=testpanel._subscriptions.GetData()[0]
                    headers=current.GetQueryHeaders()
                    testpanel._subscriptions.SelectDatas([current])
                    if action=='reset':testpanel._Reset(headers)
                    elif action=='retry_failed':testpanel._RetryFailed(headers)
                    else:testpanel._RetryIgnored(headers,None)
                    testpanel._subscriptions_panel._ExportToClipboard()
                    changes.append({'action':action,'input':incoming,'output':json.loads(copied[-1][2])})
                finally:testpanel.deleteLater()
            out['log_changes']=changes
            out['questions']=questions; out['messages']=messages
            obj=S.CreateFromSerialisableTuple(out['bundle'])
            path=os.path.join(HERE,'fixtures/subscription_exchange.png')
            PNG.DumpToPNG(512,obj.DumpToNetworkBytes(),'subscriptions','complete query histories','synthetic exchange',path)
            out['png_loaded']=json.loads(S.CreateFromNetworkBytes(PNG.LoadFromPNG(path)).DumpToString())
            panel.resize(1500,850); panel.show(); QW.QApplication.processEvents()
            panel.grab().save(os.path.join(HERE,'fixtures/subscription_exchange_editor.png'))
            return out
        finally:
            c.pub,c.GetClipboardText,c.ClipboardHasImage=old_pub,old_clip,old_image
            M.ShowInformation,M.ShowWarning,M.ShowCritical=old_info,old_warning,old_critical
            D.GetYesNo,D.PresentClipboardParseError=old_yes,old_parse
            T.GetNow,Query.GenerateQueryLogContainerName=old_now,old_generate
            panel.deleteLater()
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
        with open(os.path.join(HERE,'fixtures/subscription_exchange.json'),'w') as f: json.dump(result,f,ensure_ascii=False,indent=2)

if __name__=='__main__': main()

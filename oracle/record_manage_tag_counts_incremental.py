#!/usr/bin/env python3
"""Real Manage Tags deleted-count toggle and Incremental Tagging child workflows.

Uses actual synthetic mappings on the private basic DB and actual Qt controls,
ManageTagsPanel staging, parent cancellation/apply, and reopened DB media. Only
DialogEdit.exec is answered programmatically; its real child is driven first.
"""
import json, os, sys, tempfile
HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
OUT = os.path.join(HERE, 'fixtures', 'manage_tag_counts_incremental.json')

def record(session):
    from hydrus.core import HydrusConstants as HC
    from hydrus.client import ClientConstants as CC, ClientLocation
    from hydrus.client.media import ClientMediaSingle
    from hydrus.client.metadata import ClientContentUpdates as U
    from hydrus.client.gui.metadata.ClientGUIManageTags import ManageTagsPanel
    from hydrus.client.gui import ClientGUIDialogsQuick, ClientGUITopLevelWindowsPanels as W
    from qtpy import QtWidgets as QW
    c = session.controller
    manifest = json.load(open(os.path.join(HERE, 'fixtures/legacy_db/basic.manifest.json')))
    hashes = [bytes.fromhex(f['hash']) for f in manifest['files'][:3]]
    services = c.services_manager.GetServices((HC.LOCAL_TAG,))
    keys = {s.GetName(): s.GetServiceKey() for s in services}
    names = {value: name for name, value in keys.items()}
    local = keys['my tags']
    corpus = [
        {'service':'my tags', 'tag':'checkpoint:old', 'files':[0,1], 'deleted':True},
        {'service':'my tags', 'tag':'checkpoint:both', 'files':[0], 'deleted':True},
        {'service':'my tags', 'tag':'checkpoint:both', 'files':[1], 'deleted':False},
        {'service':'my tags', 'tag':'checkpoint:live', 'files':[0,1,2], 'deleted':False},
        {'service':next(s.GetName() for s in services if s.GetName()=='second tags'), 'tag':'checkpoint:other', 'files':[2], 'deleted':True},
        {'service':'my tags', 'tag':'page:8', 'files':[0], 'deleted':False},
        {'service':'my tags', 'tag':'page:12', 'files':[1], 'deleted':False},
    ]
    for row in corpus:
        fs = {hashes[i] for i in row['files']}
        updates = [U.ContentUpdate(HC.CONTENT_TYPE_MAPPINGS, HC.CONTENT_UPDATE_ADD, (row['tag'], fs))]
        if row['deleted']: updates.append(U.ContentUpdate(HC.CONTENT_TYPE_MAPPINGS, HC.CONTENT_UPDATE_DELETE, (row['tag'], fs)))
        c.WriteSynchronous('content_updates', U.ContentUpdatePackage.STATICCreateFromContentUpdates(keys[row['service']],updates))
    def media():
        results = {r.GetHash():r for r in c.Read('media_results',hashes)}
        return [ClientMediaSingle.MediaSingle(results[h]) for h in hashes]
    def work():
        c.new_options.SetBoolean('manage_tags_show_deleted_mappings',False)
        c.new_options.SetBoolean('yes_no_on_remove_on_manage_tags',True)
        c.new_options.SetKey('default_tag_service_tab',local)
        for key,value in [('last_incremental_tagging_namespace','page'),('last_incremental_tagging_prefix',''),('last_incremental_tagging_suffix','')]:c.new_options.SetString(key,value)
        asked=[]
        old_yes=ClientGUIDialogsQuick.GetYesNo
        ClientGUIDialogsQuick.GetYesNo=lambda win,message,**kw:asked.append(message) or QW.QDialog.DialogCode.Accepted
        old_after=c.CallAfterQtSafe
        c.CallAfterQtSafe=lambda win,fn,*args,**kw:fn(*args,**kw) if getattr(fn,'__name__','') in ('setCurrentWidget','connect') else old_after(win,fn,*args,**kw)
        def panel():return ManageTagsPanel(c.gui,ClientLocation.LocationContext.STATICCreateSimple(CC.LOCAL_FILE_SERVICE_KEY),CC.TAG_PRESENTATION_SEARCH_PAGE_MANAGE_TAGS,media())
        def page(p,key=local):return next(p._tag_services.widget(i) for i in range(p._tag_services.count()) if p._tag_services.widget(i).GetServiceKey()==key)
        def snapshot(p):
            result=[]
            for i in range(p._tag_services.count()):
                child=p._tag_services.widget(i);box=child._tags_box
                rows=[]
                for term in box._ordered_terms:
                    if term.GetTag().startswith('checkpoint:'):
                        rows.append({'tag':term.GetTag(),'label':''.join(text for text,_ in box._GetRowsOfTextsAndColours(term)[0])})
                result.append({'service':names[child.GetServiceKey()],'label':child._deleted_tags_label.text(),'visible':not child._deleted_tags_panel.isHidden(),'tooltip':child._deleted_tags_show_button.toolTip(),'rows':rows})
            return {'show_deleted':c.new_options.GetBoolean('manage_tags_show_deleted_mappings'),'services':result}
        p=panel();second=panel();deleted=[{'step':'opened','state':snapshot(p)}]
        page(p)._FlipShowDeleted()
        for owner in [p,second]:
            for child in owner._tag_services.GetPages():child._UpdateShowDeleted()
        deleted.append({'step':'show','state':snapshot(p),'other_open_owner':snapshot(second)})
        page(p).RemoveTags({'checkpoint:live'})
        deleted.append({'step':'remove','state':snapshot(p),'asked':list(asked)})
        page(p).EnterTags({'checkpoint:old'})
        deleted.append({'step':'readd','state':snapshot(p)})
        p.deleteLater();second.deleteLater();p=panel()
        deleted.append({'step':'parent_cancel_reopen','state':snapshot(p)})
        page(p).EnterTags({'checkpoint:old'});page(p).RemoveTags({'checkpoint:live'})
        for package in p._GetContentUpdatePackages():c.WriteSynchronous('content_updates',package)
        p.deleteLater();p=panel();deleted.append({'step':'parent_apply_reopen','state':snapshot(p)})
        p.deleteLater()
        p=panel();page(p).EnterTags({'checkpoint:cycle'});page(p).EnterTags({'checkpoint:cycle'})
        fresh_cycle={'state':snapshot(p),'has_changes':page(p).HasChanges()};p.deleteLater()
        # Actual child factory/callback with ordered parent media and real controls.
        scenarios=[{'name':'child_cancel','namespace':'sequence','prefix':'v','suffix':'x','start':-2,'step':-3,'reverse':True,'accept':False},
                   {'name':'child_apply_parent_cancel','namespace':'page','prefix':'','suffix':'','start':8,'step':2,'reverse':False,'accept':True},
                   {'name':'child_apply_parent_apply','namespace':'sequence','prefix':'v','suffix':'x','start':-2,'step':-3,'reverse':True,'accept':True}]
        incremental=[];old_exec=W.DialogEdit.exec
        def pairs(package):
            return [{'service':names[key],'action':cu.GetAction(),'tag':cu.GetRow()[0],'files':[hashes.index(h) for h in sorted(cu.GetHashes())]} for key,updates in package.IterateContentUpdates() for cu in updates]
        for scenario in scenarios:
            p=panel();child_states=[]
            def answer(dlg):
                editor=dlg._panel
                def state():return {'namespace':editor._namespace.text(),'prefix':editor._prefix.text(),'suffix':editor._suffix.text(),'start':editor._start.value(),'step':editor._step.value(),'reverse':editor._tag_in_reverse.isChecked(),'summary':editor._summary_st.text(),'updates':pairs(editor.GetValue())}
                child_states.append({'step':'opened','state':state()})
                for field in ('namespace','prefix','suffix'):getattr(editor,'_'+field).setText(scenario[field])
                editor._start.setValue(scenario['start']);editor._step.setValue(scenario['step']);editor._tag_in_reverse.setChecked(scenario['reverse']);editor._UpdateSummary()
                child_states.append({'step':'edited','state':state()})
                return QW.QDialog.DialogCode.Accepted if scenario['accept'] else QW.QDialog.DialogCode.Rejected
            W.DialogEdit.exec=answer
            page(p)._DoIncrementalTagging()
            staged=[item for pkg in p._GetContentUpdatePackages() for item in pairs(pkg)]
            if scenario['name'].endswith('parent_apply'):
                for pkg in p._GetContentUpdatePackages():c.WriteSynchronous('content_updates',pkg)
            p.deleteLater()
            reopened=media()
            incremental.append({'input':scenario,'child':child_states,'staged':staged,'preferences':{key:c.new_options.GetString('last_incremental_tagging_'+key) for key in ('namespace','prefix','suffix')},'reopened_tags':[sorted(m.GetTagsManager().GetCurrent(local,0)) for m in reopened]})
        from hydrus.client.gui.metadata.ClientGUIIncrementalTagging import IncrementalTaggingPanel
        from hydrus.client.media import ClientMediaManagers
        from hydrus.core import HydrusTags
        initial_cases=[]
        for subtags in [['١٢'], ['１２'], ['١2٣'], ['100','٢'], ['-4','١٢'], ['-4'], ['²','١٢'], ['0','١٢'], ['99999999'], ['𑽕𑽓'], ['𞓰𞓵']]:
            medias=media()
            raw_tags={'page:'+subtag for subtag in subtags}
            statuses={status:(raw_tags if status==HC.CONTENT_STATUS_CURRENT else set()) for status in (HC.CONTENT_STATUS_CURRENT,HC.CONTENT_STATUS_PENDING,HC.CONTENT_STATUS_DELETED,HC.CONTENT_STATUS_PETITIONED)}
            medias[0].GetMediaResult().SetTagsManager(ClientMediaManagers.TagsManager({local:statuses},{local:statuses}))
            c.new_options.SetString('last_incremental_tagging_namespace','page')
            editor=IncrementalTaggingPanel(c.gui,local,medias)
            initial_cases.append({'subtags':subtags,'sorted':HydrusTags.SortNumericTags(subtags),'initial_start':editor._start.value(),'summary':editor._summary_st.text()})
            editor._step.setValue(0)
            initial_cases[-1]['zero_step_summary']=editor._summary_st.text()
            editor.deleteLater()
        import unicodedata
        decimal_zeros=[i for i in range(0x110000) if chr(i).isdecimal() and unicodedata.decimal(chr(i))==0]
        W.DialogEdit.exec=old_exec;c.CallAfterQtSafe=old_after;ClientGUIDialogsQuick.GetYesNo=old_yes
        return {'files':[h.hex() for h in hashes],'corpus':corpus,'deleted':deleted,'incremental':incremental,'fresh_cycle':fresh_cycle,'initial_cases':initial_cases,'unicode_version':unicodedata.unidata_version,'decimal_zeros':decimal_zeros}
    return c.CallBlockingToQt(c.gui,work)

def child(out):
    import hydrus_driver, record_api
    result=hydrus_driver.run_client(record_api.unpack_fixture('basic'),record)
    with open(out,'w') as f:json.dump(result,f)

def main():
    if len(sys.argv)>1 and sys.argv[1]=='--child':child(sys.argv[2]);return
    import hydrus_driver
    with tempfile.TemporaryDirectory() as directory:
        path=os.path.join(directory,'out.json');hydrus_driver.run_in_subprocess(os.path.abspath(__file__),'--child',path)
        with open(path) as f:result=json.load(f)
    with open(OUT,'w') as f:json.dump(result,f,indent=2,ensure_ascii=False);f.write('\n')
    print('wrote',OUT)
if __name__=='__main__':main()

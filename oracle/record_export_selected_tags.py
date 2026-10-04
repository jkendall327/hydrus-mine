#!/usr/bin/env python3
"""Record actual manual-export media tag selection, sorting and clipboard menus.

The real ReviewExportFilesPanel and ListBoxTagsMedia consume basic stored media.
Capture current/pending/petitioned display counts, all-files fallback, file/tag
selection, locally changed sorting, no-op double-click and copy/search menu
payloads. Relationship lookup scheduling alone runs synchronously for exact menu
capture. Export writes and destructive client actions are never invoked.
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
        from qtpy import QtWidgets as QW, QtCore as QC
        from hydrus.client.gui import ClientGUICore as CGC, ClientGUIAsync
        from hydrus.client.gui import ClientGUITopLevelWindowsPanels as W, ClientGUIDialogsQuick as Q
        from hydrus.client.gui.exporting import ClientGUIExport as E
        from hydrus.client.gui.lists import ClientGUIListBoxes as L
        from hydrus.client.media import ClientMediaSingle as M
        from hydrus.client.metadata import ClientTagSorting as S
        from hydrus.client import ClientConstants as CC
        from hydrus.core import HydrusConstants as HC
        from hydrus.client.metadata import ClientContentUpdates as U
        results = c.Read('media_results_from_ids',[1,8,3])
        repository = next(service for service in c.services_manager.GetServices((HC.LOCAL_TAG,)) if service.GetName() == 'my tags')
        hashes = {results[0].GetHash()}
        updates = [U.ContentUpdate(HC.CONTENT_TYPE_MAPPINGS, HC.CONTENT_UPDATE_PEND, ('parity:pending example', hashes)),
                   U.ContentUpdate(HC.CONTENT_TYPE_MAPPINGS, HC.CONTENT_UPDATE_ADD, ('parity:petitioned example', hashes)),
                   U.ContentUpdate(HC.CONTENT_TYPE_MAPPINGS, HC.CONTENT_UPDATE_PETITION, ('parity:petitioned example', hashes), reason='synthetic export preview')]
        c.WriteSynchronous('content_updates',U.ContentUpdatePackage.STATICCreateFromContentUpdates(repository.GetServiceKey(),updates))
        for _ in range(12):
            if not c.WriteSynchronous('sync_tag_display_maintenance',repository.GetServiceKey(),0.5): break
        media=[M.MediaSingle(m) for m in c.Read('media_results_from_ids',[1,8,3])]
        frame=W.FrameThatTakesScrollablePanel(c.gui,'export files')
        c.options['export_path']=tempfile.gettempdir()
        panel=E.ReviewExportFilesPanel(frame,media)
        tags=panel._tags_box._tags_box
        copied, launched, menus=[],[],[]
        old_popup,old_pub,old_job,old_yes=CGC.core().PopupMenu,c.pub,ClientGUIAsync.AsyncQtJob,Q.GetYesNo
        def popup(owner,menu):menus.append(menu)
        def pub(topic,*args,**kwargs):
            if topic=='clipboard':copied.append(args[1])
            elif topic in ['new_page_query','new_page_duplicates']:
                launched.append({'topic':topic,'location':args[0].GetSerialisableTuple(),'predicates':[p.GetSerialisableTuple() for p in kwargs['initial_predicates']],'page_name':kwargs['page_name'],'activate_window':kwargs['activate_window']})
            else:old_pub(topic,*args,**kwargs)
        class ImmediateJob:
            def __init__(self,owner,callback,publish):self.callback,self.publish=callback,publish
            def start(self):self.publish(self.callback())
        CGC.core().PopupMenu,c.pub,ClientGUIAsync.AsyncQtJob=popup,pub,ImmediateJob
        Q.GetYesNo=lambda *args,**kwargs: QW.QDialog.DialogCode.Accepted
        def state(name):
            return {'case':name,'files':[m.GetHashId() for m in panel._paths.GetData(only_selected=True)],
                'sort':[tags._tag_sort.sort_type,tags._tag_sort.sort_order,tags._tag_sort.group_by],
                'rows':[{'tag':t.GetTag(),'text':t.GetCopyableTexts(with_counts=True)[0],
                    'rendered':''.join(x[0] for x in t.GetRowsOfPresentationTextsWithNamespaces(True,False,'','',False,False)[0]),
                    'selected':t in tags._selected_terms} for t in tags._ordered_terms]}
        def select(names):tags._selected_terms={t for t in tags._ordered_terms if t.GetTag() in names}
        def tree(menu):
            return [{'label':a.text(),'separator':a.isSeparator(),'children':tree(a.menu()) if a.menu() is not None else None} for a in menu.actions()]
        def find(menu,path):
            for action in menu.actions():
                if action.text()==path[0]:return find(action.menu(),path[1:]) if len(path)>1 else action
            raise RuntimeError(path)
        out={'seed_service':repository.GetName(),'seed_hash_id':results[0].GetHashId(),'files':[m.GetHashId() for m in media],'states':[],'copy':[],'menus':[]}
        try:
            out['states'].append(state('all_files'))
            panel._paths.SelectDatas([media[0]],deselect_others=True);panel._RefreshTags()
            out['states'].append(state('one_file'))
            panel._paths.SelectDatas(media[:2],deselect_others=True);panel._RefreshTags()
            out['states'].append(state('two_files'))
            selected=[t.GetTag() for t in tags._ordered_terms if ':' in t.GetTag()][:2]
            select(selected);out['selected']=selected
            out['states'].append(state('tags_selected'))
            tags._CopySelectedTexts();out['keyboard_copy']=copied[-1]
            count=len(copied);out['double_click_activated']=tags._Activate(False,False);out['double_click_copied']=len(copied)!=count
            for name,sort in [('tag_desc',S.TagSort(sort_type=S.SORT_BY_HUMAN_TAG,sort_order=CC.SORT_DESC,group_by=S.GROUP_BY_NOTHING)),
                              ('subtag_asc',S.TagSort(sort_type=S.SORT_BY_HUMAN_SUBTAG,sort_order=CC.SORT_ASC,group_by=S.GROUP_BY_NOTHING)),
                              ('count_desc',S.TagSort(sort_type=S.SORT_BY_COUNT,sort_order=CC.SORT_DESC,group_by=S.GROUP_BY_NOTHING)),
                              ('count_asc_grouped',S.TagSort(sort_type=S.SORT_BY_COUNT,sort_order=CC.SORT_ASC,group_by=S.GROUP_BY_NAMESPACE_AZ))]:
                panel._tags_box._tag_sort.SetValue(sort);panel._tags_box.EventSort();out['states'].append(state(name))
            for selection in [[],selected[:1],selected]:
                select(selection);tags.ShowMenuFromSignal(QC.QPoint(0,0));menu=menus.pop()
                row={'selected':selection,'tree':tree(menu),'copy':[]}
                for action in find(menu,['copy']).menu().actions():
                    if action.isSeparator():continue
                    action.trigger();row['copy'].append({'label':action.text(),'payload':copied[-1]})
                if selection:
                    opens=find(menu,['open']).menu()
                    for action in opens.actions():
                        if not action.isSeparator():action.trigger()
                out['menus'].append(row)
            out['launches']=launched
            select(selected)
            panel._paths.SelectDatas([media[0]],deselect_others=True);panel._RefreshTags()
            out['states'].append(state('selection_retained_after_media_change'))
            panel._paths.SelectDatas([media[1]],deselect_others=True);panel._DeletePaths();panel._RefreshTags()
            out['states'].append(state('remove_selected_file_fallback'))
            frame.resize(1100,760);frame.SetPanel(panel);frame.show();QW.QApplication.processEvents()
            frame.grab().save(os.path.join(HERE,'fixtures/export_selected_tags_editor.png'))
        finally:
            CGC.core().PopupMenu,c.pub,ClientGUIAsync.AsyncQtJob,Q.GetYesNo=old_popup,old_pub,old_job,old_yes
            frame.close()
        return out
    return c.CallBlockingToQt(c.gui,work)


def main():
    import hydrus_driver
    if len(sys.argv)>1 and sys.argv[1]=='--child':
        import record_api
        output=sys.argv[2]
        result=hydrus_driver.run_client(record_api.unpack_fixture('basic'),record)
        with open(output,'w') as f:json.dump(result,f)
        return
    with tempfile.TemporaryDirectory() as directory:
        output=os.path.join(directory,'result.json')
        hydrus_driver.run_in_subprocess(os.path.abspath(__file__),'--child',output)
        with open(output) as f:result=json.load(f)
    with open(os.path.join(HERE,'fixtures/export_selected_tags.json'),'w') as f:
        json.dump(result,f,indent=1,ensure_ascii=False);f.write('\n')
    print('wrote export_selected_tags.json')
if __name__=='__main__':main()

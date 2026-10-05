#!/usr/bin/env python3
"""Actual Qt namespace/weight questions and real DB weighted related ranking.

Modal question answers are scripted. Related-panel workers/publishers are
forwarded synchronously to their real implementations. Weight controls, table validation,
Options update and the real related-tags DB reader/filter/sort are unchanged.
Synthetic tags/mappings are written only to the private unpacked basic fixture.
"""
import json, sys, tempfile
from pathlib import Path
HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))

def record(session):
    from hydrus.client import ClientConstants as CC
    from hydrus.client.gui import ClientGUIDialogsQuick as Q, ClientGUIDialogsMessage as M
    from hydrus.client.gui import ClientGUITopLevelWindowsPanels as W
    from hydrus.client.gui.panels.options.TagSuggestionsPanel import TagSuggestionsPanel
    from hydrus.client.metadata import ClientContentUpdates as U, ClientTags
    from hydrus.client.search import ClientSearchTagContext, ClientSearchPredicate
    from hydrus.core import HydrusConstants as HC, HydrusSerialisable
    from qtpy import QtWidgets as QW
    c = session.controller
    service = next(s for s in c.services_manager.GetServices((HC.LOCAL_TAG,)) if s.GetName() == 'second tags')
    manifest = json.loads((HERE / 'fixtures/legacy_db/basic.manifest.json').read_text())
    hashes = [bytes.fromhex(f['hash']) for f in manifest['files'][:6]]
    corpus = [('source:seed', [0,1,2,3]), ('context:seed', [2,3,4,5]),
              ('alpha:first', [0,1,2]), ('beta:second', [3,4,5]),
              ('alpha:third', [4,5]), ('zero:hidden', [0,1])]
    c.WriteSynchronous('content_updates', U.ContentUpdatePackage.STATICCreateFromContentUpdates(service.GetServiceKey(), [
        U.ContentUpdate(HC.CONTENT_TYPE_MAPPINGS, HC.CONTENT_UPDATE_ADD, (tag, {hashes[i] for i in files})) for tag,files in corpus]))
    def drive():
        draft = c.new_options.Duplicate()
        panel = TagSuggestionsPanel(c.gui, draft)
        initial = draft.GetRelatedTagsTagSliceWeights()
        events, prompts, warnings = [], [], []
        answers, weights = [], []
        old_enter, old_exec, old_warning = Q.EnterText, W.DialogEdit.exec, M.ShowWarning
        def enter(parent, message, **kwargs):
            prompts.append(dict(kind='namespace', message=message)); return answers.pop(0)
        def modal(dialog):
            control = dialog._panel._control
            answer, accept = weights.pop(0)
            prompts.append(dict(kind='weight', title=dialog.windowTitle(), initial=control.value(), minimum=control.minimum(), maximum=control.maximum(), answer=answer, accept=accept))
            control.setValue(answer)
            return QW.QDialog.DialogCode.Accepted if accept else QW.QDialog.DialogCode.Rejected
        Q.EnterText, W.DialogEdit.exec = enter, modal
        M.ShowWarning = lambda parent, message: warnings.append(message)
        def snapshot(action):
            events.append(dict(action=action, search=panel._search_tag_slices_weights.GetData(), result=panel._result_tag_slices_weights.GetData(), prompts=list(prompts), warnings=list(warnings)))
        try:
            snapshot('initial')
            for namespace in ['', ':', 'creator']:
                answers.append(namespace); panel._AddSearchTagSliceWeight(); snapshot('reject ' + namespace)
            answers.append('probe'); weights.append((0, False)); panel._AddSearchTagSliceWeight(); snapshot('cancel weight')
            answers.append('probe'); weights.append((10000, True)); panel._AddSearchTagSliceWeight(); snapshot('add search')
            panel._search_tag_slices_weights.SelectDatas([('probe:',10000)], deselect_others=True)
            weights.append((0,True)); panel._EditSearchTagSliceWeight(); snapshot('edit search')
            panel._search_tag_slices_weights.SelectDatas([('',10)], deselect_others=True)
            protected = panel._CanDeleteSearchTagSliceWeight()
            panel._search_tag_slices_weights.SelectDatas([('probe:',0)], deselect_others=True)
            deletable = panel._CanDeleteSearchTagSliceWeight()
            panel._search_tag_slices_weights.DeleteSelected(); snapshot('delete search')
            answers.append('probe:'); weights.append((333,True)); panel._AddResultTagSliceWeight(); snapshot('add result')
            panel._result_tag_slices_weights.SelectDatas([('probe:',333)], deselect_others=True)
            weights.append((777,False)); panel._EditResultTagSliceWeight(); snapshot('cancel result edit')
            before = draft.GetRelatedTagsTagSliceWeights(); panel.UpdateOptions()
            saved = draft.GetRelatedTagsTagSliceWeights()
            reopened = HydrusSerialisable.CreateFromSerialisableTuple(draft.GetSerialisableTuple()).GetRelatedTagsTagSliceWeights()
        finally:
            Q.EnterText, W.DialogEdit.exec, M.ShowWarning = old_enter, old_exec, old_warning
            panel.deleteLater()
        ranking = []
        for name, search_weights, result_weights, searches in [
            ('balanced', [['',0],[':',0],['source:',100],['context:',100]], [['',0],[':',0],['alpha:',100],['beta:',100],['zero:',0]], ['source:seed','context:seed']),
            ('search bias', [['',0],[':',0],['source:',300],['context:',100]], [['',0],[':',0],['alpha:',100],['beta:',100],['zero:',0]], ['source:seed','context:seed']),
            ('result bias', [['',0],[':',0],['source:',100],['context:',100]], [['',0],[':',0],['alpha:',50],['beta:',400],['zero:',0]], ['source:seed','context:seed']),
            ('skip search', [['',0],[':',0],['source:',0],['context:',100]], [['',0],[':',0],['alpha:',100],['beta:',100],['zero:',0]], ['source:seed','context:seed']),
            ('zero all', [['',0],[':',0],['source:',0]], [['',0],[':',100]], ['source:seed'])]:
            def mapping(rows): return {slice[:-1] if slice not in ('',':') else slice: weight/100 for slice,weight in rows}
            result = c.Read('related_tags', CC.COMBINED_LOCAL_FILE_DOMAINS_SERVICE_KEY,
                ClientSearchTagContext.TagContext(service_key=service.GetServiceKey(), display_service_key=service.GetServiceKey()), searches,
                tag_display_type=ClientTags.TAG_DISPLAY_STORAGE, max_time_to_take=5.0,
                concurrence_threshold=.06, search_tag_slices_weight_dict=mapping(search_weights), result_tag_slices_weight_dict=mapping(result_weights))
            ordered = ClientSearchPredicate.SortPredicates(result[3])
            ranking.append(dict(name=name, search_weights=search_weights, result_weights=result_weights, searches=searches,
                                searched=result[0], total=result[1], skipped=result[2], rows=[dict(tag=p.GetValue(), score=p.GetCount().GetMinCount()) for p in ordered]))
        # Real RelatedTagsPanel filtering and add-only activation. Forward only
        # its named query/publisher synchronously, preserving DB and UI handlers.
        from hydrus.client.gui import ClientGUITagSuggestions as S
        from hydrus.client.media import ClientMediaSingle
        medias = [ClientMediaSingle.MediaSingle(m) for m in c.Read('media_results', hashes=hashes)]
        old_thread, old_after = c.CallToThread, c.CallAfterQtSafe
        c.CallToThread = lambda f,*a,**kw: f(*a,**kw) if getattr(f,'__name__','')=='do_it' else old_thread(f,*a,**kw)
        c.CallAfterQtSafe = lambda w,f,*a,**kw: f(*a,**kw) if getattr(f,'__name__','')=='qt_code' else old_after(w,f,*a,**kw)
        activations=[]
        consumer=S.RelatedTagsPanel(c.gui,service.GetServiceKey(),lambda tags,**kw:activations.append(dict(tags=sorted(tags),only_add=kw.get('only_add'))))
        try:
            consumer._tag_display_type.SetOnOff(False)
            c.new_options.SetRelatedTagsTagSliceWeights(ranking[0]['search_weights'],ranking[0]['result_weights'])
            consumer.SetMedia(medias);consumer._FetchRelatedTagsNew(5.0)
            initial_list=[t.GetTag() for t in consumer._related_tags._ordered_terms]
            consumer._related_tags.TakeFocusForUser();consumer._related_tags._Activate(False,False)
            after_activate=[t.GetTag() for t in consumer._related_tags._ordered_terms]
            c.new_options.SetRelatedTagsTagSliceWeights(ranking[2]['search_weights'],ranking[2]['result_weights'])
            consumer._FetchRelatedTagsNew(5.0)
            changed_list=[t.GetTag() for t in consumer._related_tags._ordered_terms]
            # All captured files already carrying beta removes that result from
            # the actual panel's filtered list, independently of ranking.
            c.WriteSynchronous('content_updates', U.ContentUpdatePackage.STATICCreateFromContentUpdates(service.GetServiceKey(), [U.ContentUpdate(HC.CONTENT_TYPE_MAPPINGS,HC.CONTENT_UPDATE_ADD,('beta:second',set(hashes)))]))
            refreshed=[ClientMediaSingle.MediaSingle(m) for m in c.Read('media_results',hashes=hashes)]
            consumer.SetMedia(refreshed);consumer._FetchRelatedTagsNew(5.0)
            filtered_list=[t.GetTag() for t in consumer._related_tags._ordered_terms]
        finally:
            consumer.hide();consumer.deleteLater();c.CallToThread,c.CallAfterQtSafe=old_thread,old_after
        panel_consumer=dict(initial=initial_list,activations=activations,after_activate=after_activate,changed=changed_list,present_on_all_filtered=filtered_list)
        return dict(initial=initial, events=events, protected_deletable=protected, normal_deletable=deletable, before=before, saved=saved, reopened=reopened,
                    service=service.GetName(), files=[h.hex() for h in hashes], corpus=corpus, ranking=ranking, consumer=panel_consumer)
    return c.CallBlockingToQt(c.gui, drive)

def main():
    import hydrus_driver
    if len(sys.argv)>1 and sys.argv[1]=='--child':
        import record_api
        Path(sys.argv[2]).write_text(json.dumps(hydrus_driver.run_client(record_api.unpack_fixture('basic'),record))); return
    with tempfile.TemporaryDirectory() as temp:
        out=Path(temp)/'result.json'
        hydrus_driver.run_in_subprocess(str(Path(__file__).resolve()),'--child',str(out))
        result=json.loads(out.read_text())
    (HERE/'fixtures/related_tag_weights.json').write_text(json.dumps(result,indent=2,ensure_ascii=False)+'\n')
    print('wrote related_tag_weights.json')
if __name__=='__main__': main()

#!/usr/bin/env python3
"""Drive real write-autocomplete fetch, highlighted choices and paste questions.

Synthetic parity tags are seeded through real mapping/sibling/parent updates.
WriteFetch runs the real database query and predicate insertion pipeline; its
results are installed in the real Qt list and every rendered row is recorded.
"""
import json, os, sys, tempfile
HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
OUT = os.path.join(HERE, 'fixtures', 'write_tag_autocomplete.json')
def record(session):
    from qtpy import QtWidgets as W
    from hydrus.client import ClientConstants as CC, ClientLocation, ClientThreading
    from hydrus.client.gui.search import ClientGUIACDropdown as A
    from hydrus.client.gui import ClientGUIDialogsQuick as Q
    from hydrus.client.metadata import ClientContentUpdates as U
    from hydrus.client.search import ClientSearchAutocomplete as SA, ClientSearchFileSearchContext as SC
    from hydrus.core import HydrusConstants as HC
    c = session.controller
    qt = lambda f: c.CallBlockingToQt(c.gui, f)
    local = qt(lambda: next(s.GetServiceKey() for s in c.services_manager.GetServices((HC.LOCAL_TAG,)) if s.GetName() == 'my tags'))
    manifest = json.load(open(os.path.join(HERE, 'fixtures', 'legacy_db', 'basic.manifest.json')))
    hashes = [bytes.fromhex(f['hash']) for f in manifest['files'][:4]]
    corpus = [('parity:amber',hashes[:3]),('parity:amber old',hashes[:1]),('parity:amethyst',hashes[1:2]),('parity:amaranth',hashes[2:3])]
    updates = [U.ContentUpdate(HC.CONTENT_TYPE_MAPPINGS,HC.CONTENT_UPDATE_ADD,(tag,set(hs))) for tag,hs in corpus]
    updates += [U.ContentUpdate(HC.CONTENT_TYPE_TAG_SIBLINGS,HC.CONTENT_UPDATE_ADD,('parity:amber old','parity:amber')),U.ContentUpdate(HC.CONTENT_TYPE_TAG_PARENTS,HC.CONTENT_UPDATE_ADD,('parity:amber','parity:colour')),U.ContentUpdate(HC.CONTENT_TYPE_TAG_PARENTS,HC.CONTENT_UPDATE_ADD,('parity:colour','parity:root'))]
    c.WriteSynchronous('content_updates', U.ContentUpdatePackage.STATICCreateFromContentUpdates(local,updates))
    for _ in range(12):
        if not c.WriteSynchronous('sync_tag_display_maintenance',local,0.5): break
    choices=[]; pasted=[]; asked=[]; clipboard={'text':''}; answer={'yes':False}
    old_clipboard=c.GetClipboardText;c.GetClipboardText=lambda:clipboard['text']
    Q.GetYesNo=lambda parent,message,**kw:(asked.append({'message':message,'title':kw.get('title')}),W.QDialog.DialogCode.Accepted if answer['yes'] else W.QDialog.DialogCode.Rejected)[1]
    old_thread=c.CallToThread
    c.CallToThread=lambda func,*args,**kw:None if func == A.WriteFetch else old_thread(func,*args,**kw)
    location=ClientLocation.LocationContext.STATICCreateSimple(CC.LOCAL_FILE_SERVICE_KEY)
    ac=qt(lambda:A.AutoCompleteDropdownTagsWrite(c.gui,lambda tags:choices.append(sorted(tags)),location,local,show_paste_button=True))
    qt(lambda:ac.tagsPasted.connect(lambda tags:pasted.append(sorted(tags))))
    def controls():
        from hydrus.client.gui.panels.options.TagEditingPanel import TagEditingPanel
        panel=TagEditingPanel(c.gui,c.new_options)
        fields=['_ac_select_first_with_count','_skip_yesno_on_write_autocomplete_multiline_paste','_show_parent_decorators_on_storage_autocomplete_taglists','_expand_parents_on_storage_autocomplete_taglists','_show_sibling_decorators_on_storage_autocomplete_taglists']
        values=[getattr(panel,name).isChecked() for name in fields]
        height=panel._ac_write_list_height_num_chars
        result={'values':values,'height':height.value(),'minimum':height.minimum(),'maximum':height.maximum()}
        panel.deleteLater()
        return result
    option_controls=qt(controls)
    queries=[]
    def fetch(text,first=False,expand=True,parents=True,siblings=True,service=None):
        def prepare():
            c.new_options.SetBoolean('ac_select_first_with_count',first)
            ac._search_results_list.SetExtraParentRowsAllowed(expand)
            ac._search_results_list.SetParentDecoratorsAllowed(parents)
            ac._search_results_list.SetSiblingDecoratorsAllowed(siblings)
            ac._text_ctrl.setText(text)
            if service is not None:
                ctx=ac._tag_context_button.GetValue();ctx.service_key=service;ac._tag_context_button.SetValue(ctx)
            ac.CancelCurrentResultsFetchJob()
            return ac._GetParsedAutocompleteText(),SC.FileSearchContext(location_context=ac._location_context_button.GetValue(),tag_context=ac._tag_context_button.GetValue())
        parsed,context=qt(prepare)
        captured={}
        def prefetch(*args):pass
        def results(job,parsed,cache,matches):captured['matches']=matches
        old_after=c.CallAfterQtSafe
        c.CallAfterQtSafe=lambda target,func,*args,**kw:func(*args,**kw) if target == ac and func in (prefetch,results) else old_after(target,func,*args,**kw)
        try:A.WriteFetch(ac,ClientThreading.JobStatus(),prefetch,results,parsed,context,SA.PredicateResultsCacheInit())
        finally:c.CallAfterQtSafe=old_after
        matches=captured['matches']
        def show():
            box=ac._search_results_list;box.SetPredicates([]);box.SetPredicates(matches)
            rows=[]
            for term in box._ordered_terms:
                texts=term.GetRowsOfPresentationTextsWithNamespaces(True,siblings,' → ',None,parents,expand)
                rows.append({'tag':term.GetPredicate().GetValue(),'count':term.GetPredicate().GetCount().HasNonZeroCount(),'rows':[''.join(t[0] for t in row) for row in texts]})
            selected=sorted(t.GetPredicate().GetValue() for t in box._selected_terms)
            return {'text':text,'first_with_count':first,'expanded':expand,'parents':parents,'siblings':siblings,'service':c.services_manager.GetName(context.GetTagContext().service_key),'suggestions':rows,'selected':selected}
        queries.append(qt(show))
    fetch('parity:amb');fetch('parity:amb',True);fetch('parity:am');fetch('parity:amber old',False,False);fetch('parity:amber old',False,False,False,False)
    fetch('parity:amb',False,False,True,True,CC.COMBINED_TAG_SERVICE_KEY)
    paste_events=[]
    for text,skip,yes,button in [(' Parity:Amber \nparity:new\nparity:new\n\n',False,False,False),(' Parity:Amber \nparity:new\nparity:new\n\n',False,True,False),('parity:skip a\nparity:skip b',True,False,False),('parity:button a\nparity:button b',False,False,True),('parity:single',False,False,False)]:
        clipboard['text']=text;answer['yes']=yes;qt(lambda:c.new_options.SetBoolean('skip_yesno_on_write_autocomplete_multiline_paste',skip))
        consumed=qt(lambda:ac._Paste() if button else ac._TryToProcessAPasteEvent())
        paste_events.append({'text':text,'skip':skip,'answer':yes,'button':button,'consumed':consumed,'asked':list(asked),'pasted':list(pasted)})
        asked.clear();pasted.clear()
    qt(ac.deleteLater);c.CallToThread=old_thread;c.GetClipboardText=old_clipboard
    return {'controls':option_controls,'corpus':[{'tag':tag,'hashes':[h.hex() for h in hs]} for tag,hs in corpus],'queries':queries,'paste':paste_events}
def child(out):
    import hydrus_driver,record_api
    result=hydrus_driver.run_client(record_api.unpack_fixture('basic'),record)
    with open(out,'w') as f:json.dump(result,f)
def main():
    if len(sys.argv)>1 and sys.argv[1]=='--child':child(sys.argv[2]);return
    import hydrus_driver
    with tempfile.TemporaryDirectory() as d:
        p=os.path.join(d,'out.json');hydrus_driver.run_in_subprocess(os.path.abspath(__file__),'--child',p)
        with open(p) as f:result=json.load(f)
    with open(OUT,'w') as f:json.dump(result,f,indent=2,ensure_ascii=False);f.write('\n')
    print('wrote',OUT)
if __name__=='__main__':main()

#!/usr/bin/env python3
"""Record actual read-autocomplete OR broadcasts, rewind/Escape/cancel and commit.

The real widget and predicate list receive synthetic selected predicates. Only
background suggestion fetching is suppressed for deterministic snapshots; the OR
state machine, button policy, text clearing and active search are unchanged.
The dropdown is explicitly shown before its real button updater is snapshotted.
"""
import json,os,sys,tempfile
HERE=os.path.dirname(os.path.abspath(__file__));sys.path.insert(0,HERE)
OUT=os.path.join(HERE,'fixtures','read_or.json')
def record(session):
    from hydrus.client import ClientConstants as CC,ClientLocation
    from hydrus.client.gui.search import ClientGUIACDropdown as A
    from hydrus.client.search import ClientSearchFileSearchContext as F,ClientSearchTagContext as T,ClientSearchPredicate as P
    from hydrus.core import HydrusConstants as HC
    c=session.controller;qt=lambda f:c.CallBlockingToQt(c.gui,f)
    service=next(s.GetServiceKey() for s in c.services_manager.GetServices((HC.LOCAL_TAG,)) if s.GetName()=='my tags')
    from hydrus.client.metadata import ClientContentUpdates as U
    manifest=json.load(open(os.path.join(HERE,'fixtures','legacy_db','basic.manifest.json')))
    hashes=[bytes.fromhex(row['hash']) for row in manifest['files'][:4]]
    corpus=[('parity:or alpha',hashes[:2]),('parity:or beta',hashes[1:3]),('parity:or single',hashes[:3])]
    updates=[U.ContentUpdate(HC.CONTENT_TYPE_MAPPINGS,HC.CONTENT_UPDATE_ADD,(tag,set(files))) for tag,files in corpus]
    c.WriteSynchronous('content_updates',U.ContentUpdatePackage.STATICCreateFromContentUpdates(service,updates))
    old=c.CallToThread;c.CallToThread=lambda fn,*args,**kw:None if fn==A.ReadFetch else old(fn,*args,**kw)
    def replay():
        context=F.FileSearchContext(location_context=ClientLocation.LocationContext.STATICCreateSimple(CC.LOCAL_FILE_SERVICE_KEY),tag_context=T.TagContext(service_key=service))
        ac=A.AutoCompleteDropdownTagsRead(c.gui,b'parity read OR',context,synchronised=False)
        ac.show();events=[]
        def snap(action,**extra):
            ac._dropdown_window.show();ac._UpdateORButtons()
            draft=ac._under_construction_or_predicate
            events.append(dict(action=action,text=ac._text_ctrl.text(),draft=None if draft is None else [p.ToString() for p in draft.GetValue()],draft_label=None if draft is None else draft.ToString(),cancel_visible=not ac._or_cancel.isHidden(),rewind_visible=not ac._or_rewind.isHidden(),predicates=sorted(p.ToString() for p in ac.GetFileSearchContext().GetPredicates()),**extra))
        def broadcast(tag,shift):
            ac._text_ctrl.setText('typed draft');ac._BroadcastChoices([P.Predicate(P.PREDICATE_TYPE_TAG,tag)],shift);snap('broadcast',tag=tag,shift=shift)
        snap('initial');broadcast('parity:or beta',True);broadcast('parity:or alpha',True);broadcast('parity:or beta',True);broadcast('parity:or gamma',True)
        ac._text_ctrl.setText('rewind draft');ac._RewindORConstruction();snap('rewind')
        snap('escape',handled=ac._HandleEscape());snap('escape',handled=ac._HandleEscape())
        broadcast('parity:or beta',True);ac._text_ctrl.setText('cancel draft');ac._CancelORConstruction();snap('cancel')
        broadcast('parity:or beta',True);broadcast('parity:or alpha',False)
        broadcast('parity:or single',True);ac._BroadcastChoices([ac._under_construction_or_predicate],False);snap('commit_draft')
        draft=P.Predicate(P.PREDICATE_TYPE_OR_CONTAINER,[P.Predicate(P.PREDICATE_TYPE_TAG,'parity:or alpha')])
        ac._search_results_list.SetPredicates([draft,P.Predicate(P.PREDICATE_TYPE_TAG,'parity:or beta')]);selected=[term.GetPredicate().ToString() for term in ac._search_results_list._selected_terms]
        query_count=len(c.Read('file_query_ids',ac.GetFileSearchContext()))
        ac.deleteLater();return dict(events=events,first_selected_with_draft=selected,query_count=query_count,corpus=[dict(tag=tag,hashes=[h.hex() for h in files]) for tag,files in corpus])
    try:return qt(replay)
    finally:c.CallToThread=old
def child(out):
    import hydrus_driver,record_api
    value=hydrus_driver.run_client(record_api.unpack_fixture('basic'),record)
    with open(out,'w') as f:json.dump(value,f)
def main():
    if len(sys.argv)>1 and sys.argv[1]=='--child':child(sys.argv[2]);return
    import hydrus_driver
    with tempfile.TemporaryDirectory() as d:
        path=os.path.join(d,'out.json');hydrus_driver.run_in_subprocess(os.path.abspath(__file__),'--child',path)
        with open(path) as f:value=json.load(f)
    with open(OUT,'w') as f:json.dump(value,f,indent=2);f.write('\n')
    print('wrote',OUT)
if __name__=='__main__':main()

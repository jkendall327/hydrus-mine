#!/usr/bin/env python3
"""Record real favourite-search domain/status, sort/order, collect/domain,
autocomplete choices and system-predicate child acceptance on the basic client.
Changes are made through the embedded widgets; GetValue proves saved consumers.
"""
import json, os, sys, tempfile
HERE=os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0,HERE)
OUT=os.path.join(HERE,'fixtures','favourite_search_editor.json')
def record(session):
    from hydrus.client import ClientConstants as CC, ClientLocation
    from hydrus.client.gui.search import ClientGUISearchPanels as F
    from hydrus.client.search import ClientSearchFileSearchContext as S, ClientSearchTagContext as T
    from hydrus.client.metadata import ClientTags
    from hydrus.core import HydrusConstants as HC
    c=session.controller
    qt=lambda f:c.CallBlockingToQt(c.gui,f)
    def setup():
        c.new_options.SetBoolean('advanced_mode',True)
        search=S.FileSearchContext(location_context=ClientLocation.LocationContext.STATICCreateSimple(CC.LOCAL_FILE_SERVICE_KEY))
        panel=F.EditFavouriteSearchPanel(c.gui,{},None,'recorded favourite',search,True,None,None)
        panel._include_media_sort.setChecked(True);panel._include_media_collect.setChecked(True)
        return panel
    panel=qt(setup); events=[]
    def snapshot(action):
        folder,name,search,sync,sort,collect=panel.GetValue()
        loc=search.GetLocationContext();tags=search.GetTagContext()
        service=lambda k:c.services_manager.GetName(k)
        stype,data=sort.sort_type
        if stype=='namespaces':data={'namespaces':data[0],'tag_display_type':data[1]}
        if stype=='rating':data=service(data)
        return {'action':action,'location':{'current':sorted(service(k) for k in loc.current_service_keys),'deleted':sorted(service(k) for k in loc.deleted_service_keys)},'tags':{'service':service(tags.service_key),'current':tags.include_current_tags,'pending':tags.include_pending_tags},'sort':{'type':stype,'data':data,'ascending':sort.sort_order==CC.SORT_ASC},'collect':{'namespaces':collect.namespaces,'ratings':[service(k) for k in collect.rating_service_keys],'unmatched':collect.collect_unmatched,'service':service(collect.tag_context.service_key)},'predicates':sorted(p.ToString(with_count=False) for p in search.GetPredicates())}
    def step(action,f):
        qt(f);events.append(qt(lambda:snapshot(action)))
    ac=panel._tag_autocomplete
    local=qt(lambda:next(s.GetServiceKey() for s in c.services_manager.GetServices((HC.LOCAL_TAG,)) if s.GetName()=='my tags'))
    rating=qt(lambda:c.services_manager.GetServices((HC.LOCAL_RATING_NUMERICAL,))[0].GetServiceKey())
    step('initial',lambda:None)
    step('trash',lambda:ac._location_context_button.SetValue(ClientLocation.LocationContext.STATICCreateSimple(CC.TRASH_SERVICE_KEY)))
    step('all_files',lambda:ac._location_context_button.SetValue(ClientLocation.LocationContext.STATICCreateSimple(CC.COMBINED_FILE_SERVICE_KEY)))
    step('all_tags',lambda:ac._tag_context_button.SetValue(T.TagContext(CC.COMBINED_TAG_SERVICE_KEY)))
    step('local_tags',lambda:ac._tag_context_button.SetValue(T.TagContext(local)))
    step('exclude_current',lambda:ac._include_current_tags.click())
    step('exclude_pending',lambda:ac._include_pending_tags.click())
    step('include_current',lambda:ac._include_current_tags.click())
    step('include_pending',lambda:ac._include_pending_tags.click())
    step('multiple_deleted',lambda:ac._location_context_button.SetValue(ClientLocation.LocationContext(current_service_keys={CC.LOCAL_FILE_SERVICE_KEY},deleted_service_keys={CC.LOCAL_FILE_SERVICE_KEY})))
    step('sort_width_default',lambda:panel._media_sort._SetSortTypeFromUser(('system',CC.SORT_FILES_BY_WIDTH)))
    step('sort_width_descending',lambda:panel._media_sort._sort_order_choice.SetValue(CC.SORT_DESC))
    step('sort_namespace',lambda:panel._media_sort._SetSortTypeFromUser(c.new_options.GetDefaultNamespaceSorts()[0].sort_type))
    step('sort_rating',lambda:panel._media_sort._SetSortTypeFromUser(('rating',rating)))
    def choose_collect():
        ctrl=panel._media_collect._collect_comboctrl
        indices=[i for i in range(ctrl.count()) if ctrl.itemData(i)[0]=='namespace' and ctrl.itemData(i)[1]=='creator' or ctrl.itemData(i)[0]=='rating' and ctrl.itemData(i)[1]==rating]
        ctrl.SetCheckedIndices(indices);panel._media_collect.CollectValuesChanged()
    step('collect_namespace_rating',choose_collect)
    step('unmatched_separate',lambda:panel._media_collect.SetCollectUnmatched(False))
    step('collect_local_tags',lambda:panel._media_collect.SetTagContext(T.TagContext(local)))
    step('my_files',lambda:ac._location_context_button.SetValue(ClientLocation.LocationContext.STATICCreateSimple(CC.LOCAL_FILE_SERVICE_KEY)))
    qt(lambda:ac._text_ctrl.setText('blue'))
    # Query the reference DB and publish through this embedded list's normal
    # method synchronously, so worker timing cannot empty the recording.
    context=qt(ac.GetFileSearchContext)
    results=c.Read('autocomplete_predicates',ClientTags.TAG_DISPLAY_DISPLAY_ACTUAL,context,search_text='blue*')
    qt(ac.CancelCurrentResultsFetchJob)
    qt(lambda:ac._SetResultsToList(results,ac._GetParsedAutocompleteText()))
    suggestions=qt(lambda:sorted(p.ToString(with_count=False) for p in ac._search_results_list.GetPredicates()))
    assert 'blue eyes' in suggestions, (suggestions,qt(lambda:snapshot('autocomplete_context')))
    qt(lambda:ac.EnterPredicates({p for p in ac._search_results_list.GetPredicates() if p.ToString(with_count=False)=='blue eyes'}))
    events.append(qt(lambda:snapshot('autocomplete_blue_eyes')))
    # The real shared flesh-out panel used by the autocomplete's system choices.
    from hydrus.client.gui.search import ClientGUIPredicatesSingle as P
    from hydrus.client.search import ClientSearchPredicate as Predicate
    def child():
        p=P.PanelPredicateSystemLimit(c.gui,Predicate.Predicate(Predicate.PREDICATE_TYPE_SYSTEM_LIMIT,256))
        p._limit.setValue(7)
        predicates=p.GetPredicates();ac.EnterPredicates(set(predicates))
        p.deleteLater()
    qt(child);events.append(qt(lambda:snapshot('system_limit_child')))
    # A saved collect tag domain changes actual grouping of the fixture files.
    from hydrus.client.media import ClientMediaList, ClientMediaCollect
    manifest=json.load(open(os.path.join(HERE,'fixtures','legacy_db','basic.manifest.json')))
    hashes=[bytes.fromhex(f['hash']) for f in manifest['files']]
    results=c.Read('media_results',hashes)
    current=[m for m in results if CC.LOCAL_FILE_SERVICE_KEY in m.GetLocationsManager().GetCurrent()]
    groups=[]
    for service in [CC.COMBINED_TAG_SERVICE_KEY,local,qt(lambda:next(s.GetServiceKey() for s in c.services_manager.GetServices((HC.LOCAL_TAG,)) if s.GetName()=='downloader tags'))]:
        for include_current in [True,False]:
            context=T.TagContext(service,include_current_tags=include_current,include_pending_tags=True)
            collect=ClientMediaCollect.MediaCollect(namespaces=['series'],collect_unmatched=False,tag_context=context)
            media=ClientMediaList.MediaList(CC.LOCAL_FILE_SERVICE_KEY,current)
            media.Collect(collect)
            members=sorted(sorted(m.GetHashes()) for m in media._sorted_media)
            groups.append({'service':qt(lambda:c.services_manager.GetName(service)),'current':include_current,'pending':True,'groups':[[h.hex() for h in g] for g in members]})
    qt(panel.deleteLater)
    return {'events':events,'autocomplete_suggestions':suggestions,'local_tag_service':qt(lambda:c.services_manager.GetName(local)),'rating_service':qt(lambda:c.services_manager.GetName(rating)),'collect_groups':groups}
def child(out):
    import hydrus_driver,record_api
    result=hydrus_driver.run_client(record_api.unpack_fixture('basic'),record)
    with open(out,'w') as f:json.dump(result,f)
def main():
    if len(sys.argv)>1 and sys.argv[1]=='--child':child(sys.argv[2]);return
    import hydrus_driver
    with tempfile.TemporaryDirectory() as d:
        p=os.path.join(d,'out.json');hydrus_driver.run_in_subprocess(os.path.abspath(__file__),'--child',p)
        with open(p) as f:r=json.load(f)
    with open(OUT,'w') as f:json.dump(r,f,indent=2,ensure_ascii=False);f.write('\n')
    print('wrote',OUT)
if __name__=='__main__':main()

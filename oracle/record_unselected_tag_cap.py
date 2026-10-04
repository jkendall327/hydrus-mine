#!/usr/bin/env python3
"""Record real thumbnail tag-computation limits, collections and sidebar notices.

Options staging uses the actual NoneableSpinCtrl. Real media-panel publication
and sidebar rendering cover no-limit/zero/finite caps, selected items, reversed
sorts, collected items and service-qualified capped headings.
"""
import json, os, sys, tempfile, time
HERE=os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0,HERE)
OUT=os.path.join(HERE,'fixtures','unselected_tag_cap.json')
def record(session):
    from hydrus.client import ClientConstants as CC,ClientLocation
    from hydrus.client.gui.panels.options.TagPresentationPanel import TagPresentationPanel
    from hydrus.client.media import ClientMediaCollect,ClientMediaSort,ClientMediaFileFilter,ClientMediaList
    from hydrus.client.metadata import ClientContentUpdates as U
    from hydrus.core import HydrusConstants as HC
    c=session.controller;gui=c.gui;qt=lambda f:c.CallBlockingToQt(gui,f)
    local=next(s.GetServiceKey() for s in c.services_manager.GetServices((HC.LOCAL_TAG,)) if s.GetName()=='my tags')
    manifest=json.load(open(os.path.join(HERE,'fixtures','legacy_db','basic.manifest.json')))
    hashes=[bytes.fromhex(f['hash']) for f in manifest['files'][:4]]
    corpus=[('parity:cap '+str(i),[h]) for i,h in enumerate(hashes)]
    corpus.extend([('cap_group:a',hashes[:2]),('cap_group:b',hashes[2:3])])
    updates=[U.ContentUpdate(HC.CONTENT_TYPE_MAPPINGS,HC.CONTENT_UPDATE_ADD,(tag,set(files))) for tag,files in corpus]
    c.WriteSynchronous('content_updates',U.ContentUpdatePackage.STATICCreateFromContentUpdates(local,updates))
    def options():
        key='number_of_unselected_medias_to_present_tags_for';draft=c.new_options.Duplicate();panel=TagPresentationPanel(gui,draft)
        ctrl=panel._number_of_unselected_medias_to_present_tags_for
        out={'shown':ctrl.GetValue(),'min':ctrl._number_value.minimum(),'max':ctrl._number_value.maximum(),'none_phrase':ctrl._checkbox.text()}
        ctrl.SetValue(1);out['before_apply']=draft.GetNoneableInteger(key);panel.deleteLater();out['cancelled']=draft.GetNoneableInteger(key)
        panel=TagPresentationPanel(gui,draft);panel._number_of_unselected_medias_to_present_tags_for.SetValue(1);panel.UpdateOptions();out['applied']=draft.GetNoneableInteger(key);panel.deleteLater();return out
    controls=qt(options)
    location=ClientLocation.LocationContext.STATICCreateSimple(CC.LOCAL_FILE_SERVICE_KEY)
    sort=ClientMediaSort.MediaSort(('system',CC.SORT_FILES_BY_HASH),CC.SORT_ASC)
    page=qt(lambda:gui._notebook.NewPageQuery(location,initial_hashes=hashes,initial_sort=sort,do_sort=True))
    panel=None
    for _ in range(300):
        candidate=qt(lambda:page.GetMediaResultsPanel())
        if hasattr(candidate,'_sorted_media') and len(candidate._sorted_media)==4:panel=candidate;break
        time.sleep(0.05)
    if panel is None:raise Exception('the real thumbnail page never loaded')
    sidebar=qt(page.GetSidebar)
    box=sidebar._current_selection_tags_list;wrapper=sidebar._current_selection_tags_box
    emitted=[]
    qt(lambda:panel.selectedMediaTagPresentationChanged.connect(lambda media,changed,capped:emitted.append((media,changed,capped))))
    def groups():return [[h.hex() for h in sorted(m.GetHashes())] for m in panel._sorted_media]
    def snap(limit,selection,collected,ascending,service):
        c.new_options.SetNoneableInteger('number_of_unselected_medias_to_present_tags_for',limit)
        panel.Collect(ClientMediaCollect.MediaCollect(namespaces=['cap_group'] if collected else [],collect_unmatched=False))
        panel.Sort(ClientMediaSort.MediaSort(('system',CC.SORT_FILES_BY_HASH),CC.SORT_ASC if ascending else CC.SORT_DESC))
        panel._Select(ClientMediaFileFilter.FileFilter(ClientMediaFileFilter.FILE_FILTER_NONE))
        if selection=='last':panel._HitMedia(list(panel._sorted_media)[-1],False,False)
        elif selection=='all':panel._Select(ClientMediaFileFilter.FileFilter(ClientMediaFileFilter.FILE_FILTER_ALL))
        wrapper.SetTagServiceKey(local if service=='my tags' else CC.COMBINED_TAG_SERVICE_KEY)
        panel._PublishSelectionChange(True)
        media,changed,capped=emitted[-1]
        rows=[]
        for term in box._ordered_terms:
            if term.GetTag().startswith('parity:cap'):
                rows.extend(''.join(text for text,_colour in row) for row in box._GetRowsOfTextsAndColours(term))
        return {'limit':limit,'selection':selection,'collected':collected,'ascending':ascending,'service':service,'items':groups(),'computed_files':sorted(h.hex() for m in media for h in m.GetHashes()),'capped':capped,'title':wrapper._title_st.text(),'rows':rows}
    cases=[]
    for collected in [False,True]:
        for limit,selection,ascending,service in [(None,'none',True,'all known tags'),(0,'none',True,'all known tags'),(1,'none',True,'all known tags'),(1,'none',False,'all known tags'),(2,'none',True,'my tags'),(1,'last',True,'my tags'),(0,'all',True,'all known tags'),(10,'none',True,'all known tags'),(4,'none',True,'all known tags'),(3,'none',True,'my tags')]:
            cases.append(qt(lambda limit=limit,selection=selection,collected=collected,ascending=ascending,service=service:snap(limit,selection,collected,ascending,service)))
    return {'options':controls,'files':[h.hex() for h in hashes],'corpus':[{'tag':tag,'hashes':[h.hex() for h in files]} for tag,files in corpus],'siblings':[],'parents':[],'cases':cases}
def child(out):
    import hydrus_driver,record_api
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

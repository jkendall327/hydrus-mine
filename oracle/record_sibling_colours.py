#!/usr/bin/env python3
"""Record sibling connector colour controls and real storage/write text runs.

Cross-namespace synthetic aliases expose solid custom/ideal/fallback colours and
Qt6 fading. Actual Options stages Cancel/Apply/reopen and disables the custom
namespace control while fading. Real Manage Tags and WriteFetch lists supply
text, RGB runs and CanFadeColours; representative Qt paints are saved under /tmp.
"""
import json, os, sys, tempfile
HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
OUT = os.path.join(HERE, 'fixtures', 'sibling_colours.json')
def record(session):
    from qtpy import QtWidgets as QW
    from hydrus.client import ClientConstants as CC, ClientLocation, ClientThreading
    from hydrus.client.gui.panels.options.TagPresentationPanel import TagPresentationPanel
    from hydrus.client.gui.metadata.ClientGUIManageTags import ManageTagsPanel
    from hydrus.client.gui.search import ClientGUIACDropdown as A
    from hydrus.client.media import ClientMediaSingle
    from hydrus.client.metadata import ClientContentUpdates as U
    from hydrus.client.search import ClientSearchAutocomplete as SA, ClientSearchFileSearchContext as SC
    from hydrus.core import HydrusConstants as HC
    c=session.controller; qt=lambda f:c.CallBlockingToQt(c.gui,f)
    local=next(s.GetServiceKey() for s in c.services_manager.GetServices((HC.LOCAL_TAG,)) if s.GetName()=='my tags')
    manifest=json.load(open(os.path.join(HERE,'fixtures','legacy_db','basic.manifest.json')))
    hashes=[bytes.fromhex(f['hash']) for f in manifest['files'][:3]]
    corpus=[('character:parity ideal',hashes[:2]),('creator:parity alias',hashes[:1])]
    updates=[U.ContentUpdate(HC.CONTENT_TYPE_MAPPINGS,HC.CONTENT_UPDATE_ADD,(tag,set(files))) for tag,files in corpus]
    updates.append(U.ContentUpdate(HC.CONTENT_TYPE_TAG_SIBLINGS,HC.CONTENT_UPDATE_ADD,('creator:parity alias','character:parity ideal')))
    c.WriteSynchronous('content_updates',U.ContentUpdatePackage.STATICCreateFromContentUpdates(local,updates))
    for _ in range(12):
        if not c.WriteSynchronous('sync_tag_display_maintenance',local,0.5):break
    def options():
        def value(options):return dict(fade=options.GetBoolean('fade_sibling_connector'),namespace=options.GetNoneableString('sibling_connector_custom_namespace_colour'))
        draft=c.new_options.Duplicate();initial=value(draft)
        panel=TagPresentationPanel(c.gui,draft)
        panel._fade_sibling_connector.setChecked(False);panel._SiblingColourStuffClicked()
        panel._sibling_connector_custom_namespace_colour.SetValue('character');before=value(draft);panel.deleteLater()
        cancelled=value(draft)
        panel=TagPresentationPanel(c.gui,draft);panel._fade_sibling_connector.setChecked(False);panel._SiblingColourStuffClicked()
        panel._sibling_connector_custom_namespace_colour.SetValue('character');panel.UpdateOptions();applied=value(draft);panel.deleteLater()
        reopened=TagPresentationPanel(c.gui,draft)
        result=dict(initial=initial,before_apply=before,cancelled=cancelled,applied=applied,reopened=value(draft),labels=[label.text() for label in reopened.findChildren(QW.QLabel) if label.text() in ('Fade the colour of the sibling connector string on Qt6: ','Namespace for the colour of the sibling connecting string: ')])
        reopened.deleteLater();return result
    controls=qt(options)
    media=[ClientMediaSingle.MediaSingle(result) for result in c.Read('media_results',hashes)]
    location=ClientLocation.LocationContext.STATICCreateSimple(CC.LOCAL_FILE_SERVICE_KEY)
    old_thread=c.CallToThread;old_after=c.CallAfterQtSafe
    c.CallToThread=lambda func,*args,**kw:None if func==A.WriteFetch or hasattr(getattr(func,'__self__',None),'_win') else old_thread(func,*args,**kw)
    c.CallAfterQtSafe=lambda win,func,*args,**kw:func(*args,**kw) if getattr(func,'__name__','') in ('setCurrentWidget','SelectPage','connect') else old_after(win,func,*args,**kw)
    def make():
        c.new_options.SetKey('default_tag_service_tab',local)
        panel=ManageTagsPanel(c.gui,location,CC.TAG_PRESENTATION_SEARCH_PAGE_MANAGE_TAGS,media)
        pages=panel._tag_services;pages.setCurrentWidget(next(pages.widget(i) for i in range(pages.count()) if pages.widget(i).GetServiceKey()==local))
        box=pages.currentWidget()._tags_box
        with box._async_text_info_lock:box._pending_async_text_info_terms.update(box._ordered_terms)
        updater=box._async_text_info_updater;updater._publish_callable(updater._work_callable(updater._pre_work_callable()))
        ac=A.AutoCompleteDropdownTagsWrite(c.gui,lambda tags:None,location,local,show_paste_button=True)
        ac._SetLocationContext(location)
        context=ac._tag_context_button.GetValue();context.service_key=local;ac._tag_context_button.SetValue(context)
        ac._text_ctrl.setText('creator:parity alias');ac.CancelCurrentResultsFetchJob()
        return panel,box,ac
    panel,box,ac=qt(make)
    parsed,context=qt(lambda:(ac._GetParsedAutocompleteText(),SC.FileSearchContext(location_context=ac._location_context_button.GetValue(),tag_context=ac._tag_context_button.GetValue())))
    captured={}
    def prefetch(*args):pass
    def results(job,parsed,cache,matches):captured['matches']=matches
    c.CallAfterQtSafe=lambda target,func,*args,**kw:func(*args,**kw) if target==ac and func in (prefetch,results) else old_after(target,func,*args,**kw)
    try:A.WriteFetch(ac,ClientThreading.JobStatus(),prefetch,results,parsed,context,SA.PredicateResultsCacheInit())
    finally:c.CallAfterQtSafe=old_after
    def prepare():
        write=ac._search_results_list;write.SetPredicates([]);write.SetPredicates(captured['matches']);write.setParent(None);return write
    write=qt(prepare)
    def rows(widget,kind):
        return [dict(tag=term.GetTag() if kind=='storage' else term.GetPredicate().GetValue(),rows=widget._GetRowsOfTextsAndColours(term),can_fade=term.CanFadeColours()) for term in widget._ordered_terms if (term.GetTag() if kind=='storage' else term.GetPredicate().GetValue()) in ('creator:parity alias','character:parity ideal')]
    def snapshot(fade,namespace):
        panel_options=TagPresentationPanel(c.gui,c.new_options)
        panel_options._fade_sibling_connector.setChecked(fade);panel_options._SiblingColourStuffClicked()
        panel_options._sibling_connector_custom_namespace_colour.SetValue(namespace)
        enabled=panel_options._sibling_connector_custom_namespace_colour.isEnabled()
        panel_options.UpdateOptions();panel_options.deleteLater()
        reopened=TagPresentationPanel(c.gui,c.new_options)
        reopened_text=reopened._sibling_connector_custom_namespace_colour._text.text();reopened.deleteLater()
        box.NotifyNewOptions();write.NotifyNewOptions()
        # Save actual unselected and selected Qt paint for representative fades.
        box.resize(700,200);write.resize(700,200);box.show();write.show();QW.QApplication.processEvents()
        if fade:
            assert write.grab().save('/tmp/sibling-colours-qt-unselected.png')
            write._Hit(False,False,next(i for i,t in enumerate(write._ordered_terms) if t.GetPredicate().GetValue()=='creator:parity alias'))
            QW.QApplication.processEvents();assert write.grab().save('/tmp/sibling-colours-qt-selected.png')
            write._DeselectAll()
        return dict(fade=fade,namespace=namespace,namespace_enabled=enabled,reopened_text=reopened_text,storage=rows(box,'storage'),write=rows(write,'write'))
    try:cases=[qt(lambda fade=fade,namespace=namespace:snapshot(fade,namespace)) for fade,namespace in [(True,'system'),(False,None),(False,'system'),(False,'character'),(False,'missing namespace'),(False,''),(False,' character '),(True,None)]]
    finally:
        c.CallToThread=old_thread;c.CallAfterQtSafe=old_after
        qt(lambda:(panel.deleteLater(),ac.deleteLater()))
    return {'options':controls,'files':[h.hex() for h in hashes],'corpus':[{'tag':tag,'hashes':[h.hex() for h in files]} for tag,files in corpus],'siblings':[['creator:parity alias','character:parity ideal']],'parents':[],'file_context':[k.hex() for k in context.GetLocationContext().current_service_keys],'tag_service':local.hex(),'query':'creator:parity alias','cases':cases}
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

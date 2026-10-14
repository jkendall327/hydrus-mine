#!/usr/bin/env python3
"""Record which runs of a sibling-annotated tag the reference fades.

The same fresh client, tags and real lists as `record_sibling_colours.py` (the
Manage Tags storage list and the write autocomplete's results list over an
alias and its ideal). For `fade_sibling_connector` on and off (and a custom
connector namespace), each list's `_Redraw` is run on an image with a painter
that records what the paint does: for every run of text drawn, whether its
pen is a linear gradient and between which colours, and whether the selected
row's background rectangle is one. The row is painted unselected and selected.
"""
import json, os, sys, tempfile
HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
OUT = os.path.join(HERE, 'fixtures', 'sibling_fade_paint.json')
def record(session):
    from qtpy import QtWidgets as QW
    from hydrus.client import ClientConstants as CC, ClientLocation, ClientThreading
    from hydrus.client.gui.panels.options.TagPresentationPanel import TagPresentationPanel
    from qtpy import QtGui as QG, QtCore as QC
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
    def paint_log(widget, selected):
        runs = []
        pending = {}

        def rgb(colour):
            return [colour.red(), colour.green(), colour.blue()]

        def gradient_of(brush):
            gradient = brush.gradient() if hasattr(brush, 'gradient') else None
            if gradient is None:
                return None
            return [rgb(colour) for (position, colour) in gradient.stops()]

        class Proxy:
            def __init__(self, painter):
                self._painter = painter

            def __getattr__(self, name):
                return getattr(self._painter, name)

            def setPen(self, pen, *a):
                if isinstance(pen, QG.QPen):
                    pending['pen'] = gradient_of(pen.brush())
                else:
                    pending['pen'] = None
                return self._painter.setPen(pen, *a)

            def fillRect(self, *args):
                fill = args[4] if len(args) == 5 else None
                if isinstance(fill, QG.QLinearGradient):
                    pending['fill'] = [rgb(colour) for (position, colour) in fill.stops()]
                else:
                    pending['fill'] = None
                return self._painter.fillRect(*args)

            def drawText(self, rect, text, *a):
                runs.append(dict(text=text, pen_gradient=pending.get('pen'), fill_gradient=pending.get('fill')))
                pending.clear()
                return self._painter.drawText(rect, text, *a)
        widget.resize(700, 200)
        widget.show()
        QW.QApplication.processEvents()
        widget._DeselectAll()
        if selected:
            index = next(i for i, t in enumerate(widget._ordered_terms) if (t.GetTag() if hasattr(t, 'GetTag') else t.GetPredicate().GetValue()) == 'creator:parity alias')
            widget._Hit(False, False, index)
        image = QG.QImage(700, 200, QG.QImage.Format.Format_ARGB32)
        image.fill(0)
        painter = QG.QPainter(image)
        try:
            widget._Redraw(Proxy(painter))
        finally:
            painter.end()
        widget._DeselectAll()
        return runs

    def snapshot(fade, namespace):
        panel_options = TagPresentationPanel(c.gui, c.new_options)
        panel_options._fade_sibling_connector.setChecked(fade)
        panel_options._SiblingColourStuffClicked()
        panel_options._sibling_connector_custom_namespace_colour.SetValue(namespace)
        panel_options.UpdateOptions()
        panel_options.deleteLater()
        box.NotifyNewOptions()
        write.NotifyNewOptions()
        return dict(fade=fade, namespace=namespace,
                    storage=dict(unselected=paint_log(box, False), selected=paint_log(box, True)),
                    write=dict(unselected=paint_log(write, False), selected=paint_log(write, True)))
    try:
        cases = [qt(lambda fade=fade, namespace=namespace: snapshot(fade, namespace)) for fade, namespace in [(True, 'system'), (False, None), (False, 'system'), (False, 'character'), (False, 'missing namespace'), (False, ''), (False, ' character '), (True, None)]]
    finally:
        c.CallToThread = old_thread
        c.CallAfterQtSafe = old_after
        qt(lambda: (panel.deleteLater(), ac.deleteLater()))
    return {'cases': cases}
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

#!/usr/bin/env python3
"""Actual Qt three controls, real circular CanvasMediaList order and image-prefetch passes.

Detached real ImageRendererCache runs genuine PIL initialization against the
previously authored RGB/RGBA corpus. Only renderer CallToThread is held to expose
pending/readiness and exact one-miss passes. Actual CanvasMediaList callbacks are
invoked shown and hidden; no reference source is edited or worker policy invented.
"""
import hashlib,json,sys,tempfile,types
from pathlib import Path
HERE=Path(__file__).resolve().parent
sys.path.insert(0,str(HERE))

def record(session):
    def qt():
        from qtpy import QtWidgets as QW
        from hydrus.core import HydrusConstants as HC,HydrusSerialisable
        from hydrus.client import ClientRendering,ClientLocation,ClientConstants as CC
        from hydrus.client.gui.canvas import ClientGUICanvas
        from hydrus.client.gui.panels.options.SpeedAndMemoryPanel import SpeedAndMemoryPanel
        from hydrus.client.caches import ClientCaches,ClientCachesBase as Base
        c=session.controller
        old_options=c.new_options;options=old_options.Duplicate()
        keys=['media_viewer_prefetch_num_previous','media_viewer_prefetch_num_next','image_cache_prefetch_limit_percentage']
        initial=[options.GetInteger(key) for key in keys]
        panel=SpeedAndMemoryPanel(c.gui,options);controls=[]
        for previous,forward,pct in [(0,0,10),(2,3,25),(50,50,50),(-1,99,5),(51,-5,99)]:
            panel._media_viewer_prefetch_num_previous.setValue(previous);panel._media_viewer_prefetch_num_next.setValue(forward);panel._image_cache_prefetch_limit_percentage.setValue(pct)
            panel.UpdateOptions();encoded=HydrusSerialisable.CreateFromString(options.DumpToString());reopen=SpeedAndMemoryPanel(c.gui,encoded)
            controls.append(dict(cache_bytes=options.GetInteger('image_cache_size'),duplicate_pairs=options.GetInteger('duplicate_filter_prefetch_num_pairs'),nice=old_options.GetBoolean('use_nice_resolution_strings'),input=[previous,forward,pct],saved=[encoded.GetInteger(k) for k in keys],reopened=[reopen._media_viewer_prefetch_num_previous.value(),reopen._media_viewer_prefetch_num_next.value(),reopen._image_cache_prefetch_limit_percentage.value()],caption=panel._image_cache_prefetch_limit_percentage_st.text(),warning=panel._prefetch_label_warning.text(),warning_style=panel._prefetch_label_warning.objectName()))
            reopen.deleteLater()
        for k,value in zip(keys,initial):options.SetInteger(k,value)
        panel._media_viewer_prefetch_num_next.setValue(17);cancel_saved=[options.GetInteger(k) for k in keys]
        for widget in [panel._image_cache_size,panel._media_viewer_prefetch_num_next]:
            parent=widget.parentWidget()
            while not hasattr(parent,'_expand_button'):parent=parent.parentWidget()
            parent._expand_button.click()
        panel.resize(1100,800);panel.show();QW.QApplication.processEvents();panel.grab().save(str(HERE/'fixtures/viewer_prefetch_qt.png'));panel.close()
        manifest=json.loads((HERE/'fixtures/legacy_db/basic.manifest.json').read_text());seed=c.Read('media_results',[bytes.fromhex(next(f['hash'] for f in manifest['files'] if f['name']=='jpeg_00.jpg'))])[0]
        media={};paths={};names={}
        for name,width in [('a',10),('b',10),('c',10),('d',10),('equal',10),('small',9),('large',11)]:
            path=HERE/'fixtures/image_cache'/(name+'.png');result=seed.Duplicate();info=result.GetFileInfoManager();info.hash=hashlib.sha256(path.read_bytes()).digest();info.mime=HC.IMAGE_PNG;info.width=width;info.height=1;info.num_frames=None
            media[name]=result;paths[info.hash]=str(path);names[info.hash]=name
        location=ClientLocation.LocationContext.STATICCreateSimple(CC.COMBINED_LOCAL_FILE_DOMAINS_SERVICE_KEY)
        canvases=[];neighbours=[];pending=[];clock=[0.0]
        old_thread=c.CallToThread;old_path=c.client_files_manager.GetFilePath;old_time=Base.time;old_cache=c.images_cache;old_pub=c.pub
        def thread(fn,*args,**kwargs):
            if isinstance(getattr(fn,'__self__',None),ClientRendering.ImageRenderer):pending.append(fn);return
            return old_thread(fn,*args,**kwargs)
        def path(hash,mime,*args,**kwargs):return paths[hash] if hash in paths else old_path(hash,mime,*args,**kwargs)
        def pub(topic,*args,**kwargs):
            if topic=='notify_image_finished_rendering':return
            return old_pub(topic,*args,**kwargs)
        class Owner:
            new_options=options
            def sub(self,*args):pass
        def cache(size=100,storage=50,pct=50):
            options.SetInteger('image_cache_size',size);options.SetInteger('image_cache_storage_limit_percentage',storage);options.SetInteger('image_cache_prefetch_limit_percentage',pct)
            return ClientCaches.ImageRendererCache(Owner())
        traces=[];flush=[];candidate_rules=[];hidden=[]
        try:
            c.new_options=options;c.CallToThread=thread;c.client_files_manager.GetFilePath=path;Base.time=types.SimpleNamespace(monotonic=lambda:clock[0]);c.pub=pub
            for count in [1,2,3,7]:
                results=list(media.values())[:count];canvas=ClientGUICanvas.CanvasMediaListNavigable(c.gui,b'prefetch-test',location,results);canvases.append(canvas)
                ordered=canvas._media_list.GetSortedMedia()
                for index in range(count):
                    canvas._current_media=ordered[index]
                    for previous,forward in [(0,0),(2,3),(1,0),(0,1),(50,50),(3,1)]:
                        options.SetInteger(keys[0],previous);options.SetInteger(keys[1],forward)
                        neighbours.append(dict(files=[names[m.GetHash()] for m in ordered],index=index,previous=previous,next=forward,neighbours=[names[m.GetHash()] for m in canvas._GetPrefetchNeighboursInPreferenceOrder()]))
                canvas.hide()
            def state(ca):return dict(keys=[names[h] for h in ca._data_cache.GetAllKeys()],bytes=ca._data_cache._total_estimated_memory_footprint,pending=[names[f.__self__.GetHash()] for f in pending if f.__self__.GetHash() in names])
            def scenario(label,size,storage,pct,events):
                ca=cache(size,storage,pct);pending.clear();held={};rows=[]
                for action,arg in events:
                    if action=='get':held[arg]=ca.GetImageRenderer(media[arg]);created=[]
                    elif action=='finish':
                        fn=next(f for f in pending if f.__self__ is held[arg]);pending.remove(fn);fn();created=[]
                    else:
                        before=len(pending);ca.PrefetchImageRenderers([media[n] for n in arg]);new=pending[before:];created=[names[f.__self__.GetHash()] for f in new]
                        for f in new:held[names[f.__self__.GetHash()]]=f.__self__
                    rows.append(dict(action=action,input=arg,created=created,**state(ca)))
                traces.append(dict(name=label,policy=[size,storage,pct],events=rows))
            scenario('one miss and pending blocks',120,50,50,[('prefetch',['a','b','c']),('prefetch',['a','b','c']),('finish','a'),('prefetch',['a','b','c']),('finish','b'),('prefetch',['a','b','c'])])
            scenario('total equality permits',100,50,30,[('prefetch',['a']),('finish','a'),('prefetch',['a','small'])])
            scenario('single equality schedules uncached again',100,30,50,[('prefetch',['a']),('finish','a'),('prefetch',['a']),('finish','a')])
            scenario('loaded rgba counts against budget',120,50,50,[('get','b'),('finish','b'),('prefetch',['b','a'])])
            scenario('large first blocks later small',100,30,50,[('prefetch',['large','small'])])
            unknown=media['a'].Duplicate();unknown.GetFileInfoManager().width=None;ca=cache();before=len(pending);ca.PrefetchImageRenderers([unknown,media['small']]);traces.append(dict(name='unknown first blocks',policy=[100,50,50],events=[dict(action='prefetch_unknown',input=['a','small'],created=[names[f.__self__.GetHash()] for f in pending[before:]],**state(ca))]))
            for wanted in [29,30,69,70,71]:
                pending.clear();ca=cache(100,100,50);a=ca.GetImageRenderer(media['a']);b=ca.GetImageRenderer(media['b']);fn=next(f for f in pending if f.__self__ is a);pending.remove(fn);fn()
                before=state(ca);success=ca._data_cache.TryToFlushEasySpaceForPrefetch(wanted);flush.append(dict(wanted=wanted,before=before,success=success,after=state(ca)))
            pending.clear();ca=cache(60,100,50);ca.GetImageRenderer(media['a']);before=state(ca);success=ca._data_cache.TryToFlushEasySpaceForPrefetch(30);flush.append(dict(wanted=30,before=before,success=success,after=state(ca),note='exact free space with only pending renderer refuses'))
            # These are actual Canvas callbacks, not a model visibility assumption.
            canvas=canvases[-1];canvas._current_media=canvas._media_list.GetSortedMedia()[0]
            for visible in [True,False]:
                pending.clear();c.images_cache=cache();canvas.setVisible(visible);canvas._MaintainNeighbourPrefetch();hidden.append(dict(visible=canvas.isVisible(),**state(c.images_cache)))
            from hydrus.client.gui.canvas import ClientGUICanvasMedia
            original_views=options.GetMediaViewOptions();original_view=options._GetMediaViewOptions(HC.IMAGE_PNG)
            for action in [CC.MEDIA_VIEWER_ACTION_SHOW_WITH_NATIVE,CC.MEDIA_VIEWER_ACTION_SHOW_WITH_MPV,CC.MEDIA_VIEWER_ACTION_SHOW_WITH_QTMEDIAPLAYER,CC.MEDIA_VIEWER_ACTION_DO_NOT_SHOW,CC.MEDIA_VIEWER_ACTION_SHOW_OPEN_EXTERNALLY_BUTTON]:
                view=list(original_view);view[0]=action;options.SetMediaViewOptions(dict(original_views) | {HC.IMAGE_PNG:tuple(view)});candidate_rules.append(dict(action=action,expected=ClientGUICanvasMedia.WeAreExpectingToLoadThisMediaFile(media['a'],CC.CANVAS_MEDIA_VIEWER),static=media['a'].IsStaticImage()))
            options.SetMediaViewOptions(original_views)
            legacy=options.GetSerialisableTuple()
        finally:
            for canvas in canvases:canvas._current_media=None;canvas.hide();canvas.deleteLater()
            QW.QApplication.processEvents();c.new_options=old_options;c.CallToThread=old_thread;c.client_files_manager.GetFilePath=old_path;Base.time=old_time;c.images_cache=old_cache;c.pub=old_pub
        return dict(initial=initial,controls=controls,cancel_saved=cancel_saved,neighbours=neighbours,prefetch=traces,flush=flush,hidden=hidden,candidates=candidate_rules,legacy=legacy)
    return session.controller.CallBlockingToQt(session.controller.gui,qt)

def main():
    import hydrus_driver,record_api
    if len(sys.argv)>1:
        out=Path(sys.argv[2]);value=hydrus_driver.run_client(record_api.unpack_fixture('basic'),record);out.write_text(json.dumps(value));return
    with tempfile.TemporaryDirectory() as tmp:
        out=Path(tmp)/'record.json';hydrus_driver.run_in_subprocess(str(Path(__file__).resolve()),'--child',str(out));value=json.loads(out.read_text())
    (HERE/'fixtures/viewer_prefetch.json').write_text(json.dumps(value,indent=2)+'\n');print('wrote actual Qt viewer_prefetch.json and PNG')
if __name__=='__main__':main()

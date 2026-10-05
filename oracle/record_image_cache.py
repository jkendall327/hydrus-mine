#!/usr/bin/env python3
"""Real Qt image-cache controls and ImageRendererCache accounting/admission.

Runs actual ImageRenderer._Initialise/PIL against authored RGB/RGBA PNGs. Only
that renderer's CallToThread is held, exposing the real estimate-before-decode
transition; detached DataCache uses an owned scripted monotonic clock. The
controller and copied database remain real, and every patched method is restored.
"""
import hashlib
import json
import sys
import tempfile
import types
from pathlib import Path
HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))


def record(session):
    def qt():
        from PIL import Image
        from qtpy import QtCore as QC, QtWidgets as QW
        from hydrus.core import HydrusConstants as HC, HydrusSerialisable
        from hydrus.client import ClientRendering
        from hydrus.client.gui import ClientGUIFunctions
        from hydrus.client.caches import ClientCaches, ClientCachesBase as Base
        from hydrus.client.gui.panels.options.SpeedAndMemoryPanel import SpeedAndMemoryPanel
        from hydrus.client.gui.metadata import ClientGUITime
        c = session.controller
        options = c.new_options.Duplicate()
        panel = SpeedAndMemoryPanel(c.gui, options)
        keys = ['image_cache_size', 'image_cache_timeout', 'image_cache_storage_limit_percentage']
        initial = [options.GetInteger(k) for k in keys]
        screen = ClientGUIFunctions.GetDisplaySize(panel)
        display = [screen.width(), screen.height()]
        controls = []
        for size, timeout, pct in [(1,299,10),(1024,300,25),(32768,301,50),(1024**3,600,25),(1048576*1024**4,172800,10)]:
            panel._image_cache_size.SetValue(size)
            panel._image_cache_timeout.SetValue(timeout)
            panel._image_cache_storage_limit_percentage.setValue(pct)
            panel.UpdateOptions()
            encoded = HydrusSerialisable.CreateFromString(options.DumpToString())
            reopen = SpeedAndMemoryPanel(c.gui, encoded)
            controls.append(dict(input=[size,timeout,pct],separated=list(panel._image_cache_size.GetSeparatedValue()),saved=[encoded.GetInteger(k) for k in keys],reopened=[reopen._image_cache_size.GetValue(),reopen._image_cache_timeout.GetValue(),reopen._image_cache_storage_limit_percentage.value()],estimate=panel._estimated_number_fullscreens.text(),percentage_estimate=panel._image_cache_storage_limit_percentage_st.text()))
            reopen.deleteLater()
        for key, value in zip(keys, initial): options.SetInteger(key,value)
        panel._image_cache_size.SetValue(1024)
        cancel_saved = [options.GetInteger(k) for k in keys]
        raw = options.Duplicate();raw.SetInteger(keys[0],1048577);raw.SetInteger(keys[1],299)
        raw_panel = SpeedAndMemoryPanel(c.gui,raw);raw_panel.UpdateOptions()
        delta = ClientGUITime.TimeDeltaWidget(raw_panel,min=300,days=True,hours=True,minutes=True)
        delta.SetValue(0);delta.eventFilter(delta._minutes,QC.QEvent(QC.QEvent.Type.FocusOut))
        raw_values = dict(saved=[raw.GetInteger(k) for k in keys],edited_minimum=delta.GetValue())
        legacy = raw.GetSerialisableTuple()
        parent = panel._image_cache_size.parentWidget()
        while not hasattr(parent,'_expand_button'):parent=parent.parentWidget()
        parent._expand_button.click();panel.resize(1100,650);panel.show();QW.QApplication.processEvents()
        panel.grab().save(str(HERE/'fixtures/image_cache_qt.png'));panel.close();raw_panel.deleteLater()
        manifest = json.loads((HERE/'fixtures/legacy_db/basic.manifest.json').read_text())
        original_result = c.Read('media_results',[bytes.fromhex(next(f['hash'] for f in manifest['files'] if f['name']=='jpeg_00.jpg'))])[0]
        media = {};paths={};pending=[];clock=[0.0]
        folder=HERE/'fixtures/image_cache';folder.mkdir(exist_ok=True)
        for name,width,mode in [('a',10,'RGB'),('b',10,'RGBA'),('c',10,'RGB'),('d',10,'RGB'),('equal',10,'RGB'),('small',9,'RGB'),('large',11,'RGBA')]:
            image=Image.new(mode,(width,1),(23,41,len(name)+ord(name[0])) if mode=='RGB' else (23,41,len(name)+ord(name[0]),101))
            path=folder/(name+'.png');image.save(path)
            result=original_result.Duplicate();info=result.GetFileInfoManager()
            info.hash=hashlib.sha256(path.read_bytes()).digest();info.mime=HC.IMAGE_PNG;info.width=width;info.height=1
            media[name]=result;paths[info.hash]=str(path)
        original_thread=c.CallToThread;original_path=c.client_files_manager.GetFilePath;original_time=Base.time
        def thread(fn,*args,**kwargs):
            if isinstance(getattr(fn,'__self__',None),ClientRendering.ImageRenderer):pending.append(fn);return
            return original_thread(fn,*args,**kwargs)
        def path(hash,mime,*args,**kwargs):
            return paths[hash] if hash in paths else original_path(hash,mime,*args,**kwargs)
        class Owner:
            new_options=options
            def sub(self,*args):pass
        c.CallToThread=thread;c.client_files_manager.GetFilePath=path;Base.time=types.SimpleNamespace(monotonic=lambda:clock[0])
        options.SetInteger(keys[0],100);options.SetInteger(keys[1],300);options.SetInteger(keys[2],50)
        cache=ClientCaches.ImageRendererCache(Owner());trace=[];held={};names={m.GetHash():n for n,m in media.items()}
        def state(action,**extra):
            trace.append(dict(action=action,now=clock[0],keys=[names[h] for h in cache._data_cache.GetAllKeys()],bytes=cache._data_cache._total_estimated_memory_footprint,policy=[options.GetInteger(k) for k in keys],**extra))
        def get(name):
            before=len(pending);r=cache.GetImageRenderer(media[name]);held[name]=r
            state('get '+name,created=len(pending)>before,estimate=r.GetEstimatedMemoryFootprint())
        def finish(name):
            fn=next(f for f in pending if f.__self__ is held[name]);pending.remove(fn);fn()
            state('finish '+name,loaded_bytes=held[name].GetEstimatedMemoryFootprint(),channels=held[name].GetNumPyImage().shape[-1])
        try:
            get('a');finish('a');clock[0]=1;get('b');finish('b');state('loaded accounting unchanged')
            clock[0]=2;get('b');clock[0]=3;get('c');finish('c');clock[0]=4;get('d');finish('d')
            cache._data_cache.MaintainCache();state('maintain size')
            clock[0]=303;cache._data_cache.MaintainCache();state('maintain exact timeout')
            clock[0]=303.001;cache._data_cache.MaintainCache();state('maintain past timeout')
            options.SetInteger(keys[0],120);options.SetInteger(keys[2],25);cache.NotifyNewOptions();state('set policy')
            clock[0]=304;get('equal');finish('equal');get('equal');finish('equal');get('small');finish('small')
            options.SetInteger(keys[2],10);cache.NotifyNewOptions();state('lower percentage keeps existing')
            get('small');get('large');finish('large')
            options.SetInteger(keys[0],20);cache.NotifyNewOptions();state('shrink')
            state('external renderer remains',external_bytes=held['small'].GetEstimatedMemoryFootprint())
            options.SetInteger(keys[0],100);options.SetInteger(keys[1],600);options.SetInteger(keys[2],50);cache.NotifyNewOptions();state('extend timeout')
            get('b');finish('b');clock[0]=904;cache._data_cache.MaintainCache();state('exact extended timeout')
            clock[0]=904.001;cache._data_cache.MaintainCache();state('past extended timeout')
            cache.Clear();state('clear')
            metadata_estimates=[]
            for width,height in [(None,11),(10,None),(None,None),(10,1),(0,1)]:
                result=media['a'].Duplicate();info=result.GetFileInfoManager();info.width=width;info.height=height
                renderer=ClientRendering.ImageRenderer(result)
                metadata_estimates.append(dict(resolution=[width,height],estimated_bytes=renderer.GetEstimatedMemoryFootprint()))
        finally:
            c.CallToThread=original_thread;c.client_files_manager.GetFilePath=original_path;Base.time=original_time
        return dict(display=display,initial=initial,controls=controls,cancel_saved=cancel_saved,raw=raw_values,legacy=legacy,cache=trace,metadata_estimates=metadata_estimates)
    return session.controller.CallBlockingToQt(session.controller.gui,qt)


def main():
    import hydrus_driver,record_api
    if len(sys.argv)>1:
        out=Path(sys.argv[2]);value=hydrus_driver.run_client(record_api.unpack_fixture('basic'),record);out.write_text(json.dumps(value));return
    with tempfile.TemporaryDirectory() as tmp:
        out=Path(tmp)/'record.json';hydrus_driver.run_in_subprocess(str(Path(__file__).resolve()),'--child',str(out));value=json.loads(out.read_text())
    (HERE/'fixtures/image_cache.json').write_text(json.dumps(value,indent=2)+'\n')
    print('wrote image_cache.json, authored PNGs and actual Qt controls PNG')
if __name__=='__main__':main()

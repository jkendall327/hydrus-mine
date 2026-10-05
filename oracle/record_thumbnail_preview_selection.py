#!/usr/bin/env python3
"""Actual Qt four preview-selection checkboxes and thumbnail focus publications.

Private Options drafts and actual loaded media exercise all Boolean policies,
Ctrl toggles, Shift ranges/combined modifiers, fallback clicks and keyboard moves.
Preview publication is recorded at the actual focusMediaChanged/Cleared signals;
existing preview canvas connections are disconnected only to avoid offscreen
movie playback. Thumbnail selection/focus handlers and media metadata are real.
"""
import itertools,json,sys,tempfile,time
from pathlib import Path
HERE=Path(__file__).resolve().parent
sys.path.insert(0,str(HERE))
KEYS=['focus_preview_on_ctrl_click','focus_preview_on_ctrl_click_only_static','focus_preview_on_shift_click','focus_preview_on_shift_click_only_static']
ATTRS=['_focus_preview_on_ctrl_click','_focus_preview_on_ctrl_click_only_static','_focus_preview_on_shift_click','_focus_preview_on_shift_click_only_static']

def record(session):
    from qtpy import QtWidgets as QW
    from hydrus.client import ClientConstants as CC,ClientLocation,ClientApplicationCommand as CAC
    from hydrus.client.gui.panels.options.ThumbnailsPanel import ThumbnailsPanel
    from hydrus.client.media import ClientMediaList,ClientMediaSingle
    from hydrus.core import HydrusSerialisable
    c=session.controller;gui=c.gui;original=c.new_options
    manifest=json.loads((HERE/'fixtures/legacy_db/basic.manifest.json').read_text())
    hashes=[bytes.fromhex(f['hash']) for f in manifest['files']]
    def qt(fn):return c.CallBlockingToQt(gui,fn)
    page=qt(lambda:gui._notebook.NewPageQuery(ClientLocation.LocationContext.STATICCreateSimple(CC.LOCAL_FILE_SERVICE_KEY),initial_hashes=hashes))
    for _ in range(400):
        panel=qt(page.GetMediaResultsPanel)
        if hasattr(panel,'_thumbnail_layout') and len(panel._sorted_media)==len(hashes):break
        time.sleep(.05)
    else:raise RuntimeError('thumbnail page did not load')
    def drive():
        draft=original.Duplicate();c.new_options=draft
        def values():return [draft.GetBoolean(key) for key in KEYS]
        options=ThumbnailsPanel(gui,draft)
        labels=[w.text() for w in options.findChildren(QW.QLabel) if 'focus thumbnails in the preview' in w.text() or 'Only on files with no duration' in w.text()]
        defaults=dict(values=values(),enabled=[getattr(options,a).isEnabled() for a in ATTRS],ghost_enabled=options._on_shift_click_move_ghost_focus_to_last_hit.isEnabled())
        for attr in ATTRS:getattr(options,attr).setChecked(True)
        options._UpdatePreviewCheckboxes();cancelled=values();options.deleteLater()
        controls=[]
        for flags in itertools.product([False,True],repeat=4):
            options=ThumbnailsPanel(gui,draft)
            for attr,value in zip(ATTRS,flags):getattr(options,attr).setChecked(value)
            options._UpdatePreviewCheckboxes()
            enabled=[getattr(options,a).isEnabled() for a in ATTRS]
            ghost=options._on_shift_click_move_ghost_focus_to_last_hit.isEnabled()
            options.UpdateOptions()
            reopened=ThumbnailsPanel(gui,HydrusSerialisable.CreateFromString(draft.DumpToString()))
            controls.append(dict(input=flags,enabled=enabled,ghost_enabled=ghost,saved=values(),reopened=[getattr(reopened,a).isChecked() for a in ATTRS]))
            options.deleteLater();reopened.deleteLater()
        media=list(panel._sorted_media);index={m:i for i,m in enumerate(media)}
        by_hash={m.GetHash():i for i,m in enumerate(media)}
        durations=[m.GetDurationMS() for m in media]
        static=[i for i,d in enumerate(durations) if d is None]
        timed=[i for i,d in enumerate(durations) if d is not None]
        assert len(static)>=2 and timed
        a,b=static[:2];d=timed[0]
        duration_shapes=[]
        for values in ([None],[0],[1],[None,0],[None,1]):
            results=[]
            for i,value in enumerate(values):
                result=media[static[i]].GetMediaResult().Duplicate()
                result.GetFileInfoManager().duration_ms=value
                results.append(result)
            collection=len(values)>1
            shaped=ClientMediaList.MediaCollection(ClientLocation.LocationContext.STATICCreateSimple(CC.LOCAL_FILE_SERVICE_KEY),results) if collection else ClientMediaSingle.MediaSingle(results[0])
            duration_shapes.append(dict(input=values,collection=collection,duration=shaped.GetDurationMS()))
        # Keep the actual panel publications, suppress only downstream playback.
        panel.focusMediaChanged.disconnect();panel.focusMediaCleared.disconnect()
        published=[]
        panel.focusMediaChanged.connect(lambda m:published.append(by_hash[m.GetHash()]))
        panel.focusMediaCleared.connect(lambda:published.append(None))
        def snapshot():return dict(selected=sorted(index[m] for m in panel._selected_media),focused=index.get(panel._focused_media),last_hit=index.get(panel._last_hit_media),anchor=index.get(panel._shift_select_started_with_this_media),ghost=index.get(panel._previously_focused_media_when_nothing_now),published=list(published))
        selections=[]
        for flags in itertools.product([False,True],repeat=4):
            for key,value in zip(KEYS,flags):draft.SetBoolean(key,value)
            panel._HitMedia(None,False,False);published.clear();steps=[]
            for action,i,ctrl,shift in [('plain',a,False,False),('ctrl-duration',d,True,False),('ctrl-static',b,True,False),('ctrl-remove',b,True,False),('shift-duration',d,False,True),('shift-static',b,False,True),('shift-contract',a,False,True),('ctrl-shift',d,True,True),('move',0,False,False),('shift-move',0,False,True),('reset',None,False,False),('shift-fallback',d,False,True)]:
                if action in ('move','shift-move'):
                    command=CAC.ApplicationCommand.STATICCreateSimpleCommand(CAC.SIMPLE_MOVE_THUMBNAIL_FOCUS,(CAC.MOVE_RIGHT,CAC.SELECTION_STATUS_SHIFT if shift else CAC.SELECTION_STATUS_NORMAL));panel.ProcessApplicationCommand(command)
                else:panel._HitMedia(media[i] if i is not None else None,ctrl,shift)
                steps.append(dict(action=action,index=i,ctrl=ctrl,shift=shift,after=snapshot()))
            selections.append(dict(policy=flags,steps=steps))
        return dict(keys=KEYS,labels=labels,defaults=defaults,cancelled=cancelled,controls=controls,durations=durations,duration_shapes=duration_shapes,legacy_options=json.loads(draft.DumpToString()),targets=dict(static_a=a,static_b=b,duration=d),selections=selections,limits=['Actual Qt focus publication recorded; downstream preview canvas movie playback disconnected in this private recorder process','No reference source mutation; selection sequences use loaded metadata, duration-shape cases use private duplicated real MediaResults with explicit None/0/1 durations'])
    try:return qt(drive)
    finally:c.new_options=original

def main():
    import hydrus_driver,record_api
    if len(sys.argv)>1 and sys.argv[1]=='--child':
        import signal
        signal.alarm(40)
        from hydrus.client.gui.canvas import ClientGUIMPV
        ClientGUIMPV.MPV_IS_AVAILABLE=False
        Path(sys.argv[2]).write_text(json.dumps(hydrus_driver.run_client(record_api.unpack_fixture('basic'),record)));return
    with tempfile.TemporaryDirectory() as directory:
        path=Path(directory)/'result.json';hydrus_driver.run_in_subprocess(str(Path(__file__).resolve()),'--child',str(path));data=json.loads(path.read_text())
    target=HERE/'fixtures/thumbnail_preview_selection.json';target.write_text(json.dumps(data,indent=2)+'\n');print(f'wrote {target}')
if __name__=='__main__':main()

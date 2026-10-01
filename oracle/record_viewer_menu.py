#!/usr/bin/env python3
"""Record the reference's media viewer right-click menu.

In the running client, a media viewer (`CanvasMediaListBrowser`, in its
`CanvasFrame`) is opened on the `basic` fixture's files current in "my
files", and on those in "all local files" (with the trash's);
for several files in turn (an image in the inbox, an archived one, an
animation, a video or audio file with sound, a trashed one) its own
`ShowMenuFromSignal` builds the menu, which is captured rather than shown,
and its tree recorded: each entry's text, separators as "---", submenus
with their entries, and the volume slider as `{"slider": label, "value":
value}`. With each: what the menu reads from the viewer (the zoom, the
canvas's fit zoom, whether it is at its largest zoom, whether the frame
is fullscreen), at a fixed "now", for the info lines.

Usage: QT_QPA_PLATFORM=offscreen python oracle/record_viewer_menu.py
       (writes fixtures/viewer_menu.json)
"""

import json
import os
import sys
import time

HERE = os.path.dirname( os.path.abspath( __file__ ) )

sys.path.insert( 0, HERE )

OUT = os.path.join( HERE, 'fixtures', 'viewer_menu.json' )
MANIFEST = json.load( open( os.path.join( HERE, 'fixtures', 'legacy_db', 'basic.manifest.json' ) ) )


def tree( menu ):

    from qtpy import QtWidgets as QW

    out = []

    for action in menu.actions():

        if action.isSeparator():

            out.append( '---' )

        elif action.menu() is not None:

            out.append( { 'menu' : action.text(), 'entries' : tree( action.menu() ) } )

        elif isinstance( action, QW.QWidgetAction ) and action.defaultWidget() is not None:

            widget = action.defaultWidget()

            label = widget.findChild( QW.QLabel )
            slider = widget.findChild( QW.QSlider )

            out.append( { 'slider' : label.text() if label is not None else '', 'value' : slider.value() if slider is not None else None } )

        else:

            out.append( action.text() )


    return out


def record( session ):

    from hydrus.core import HydrusTime

    controller = session.controller
    gui = controller.gui

    now = int( time.time() )

    real = ( HydrusTime.GetNow, HydrusTime.GetNowMS, HydrusTime.GetNowFloat )

    HydrusTime.GetNow = lambda: now
    HydrusTime.GetNowMS = lambda: now * 1000
    HydrusTime.GetNowFloat = lambda: float( now )

    try:

        return { 'now' : now, 'viewers' : record_viewers( controller, gui ) }

    finally:

        ( HydrusTime.GetNow, HydrusTime.GetNowMS, HydrusTime.GetNowFloat ) = real



def record_viewers( controller, gui ):

    from hydrus.core import HydrusConstants as HC
    from hydrus.client import ClientConstants as CC
    from hydrus.client import ClientLocation
    from hydrus.client.gui import ClientGUICore as CGC
    from hydrus.client.gui.canvas import ClientGUICanvas
    from hydrus.client.gui.canvas import ClientGUICanvasFrame

    def qt( f ):

        return controller.CallBlockingToQt( gui, f )


    hashes = [ bytes.fromhex( f[ 'hash' ] ) for f in MANIFEST[ 'files' ] ]
    names = { bytes.fromhex( f[ 'hash' ] ) : f[ 'name' ] for f in MANIFEST[ 'files' ] }

    media_results = controller.Read( 'media_results', hashes )

    # the menu, captured as it would be shown
    captured = []

    core = CGC.core()

    def capture( widget, menu ):

        from hydrus.client.gui import ClientGUIMenus

        ClientGUIMenus.RemoveFinalSeparator( menu )

        # (kept, to read once the share menu's other hashes fill in)
        captured.append( menu )


    core.PopupMenu = capture

    viewers = []

    for ( name, service_key, picks ) in (
        ( 'my files', CC.LOCAL_FILE_SERVICE_KEY, [ 'jpeg_00.jpg', 'jpeg_05.jpg', 'gif_animated.gif', 'audio.mp3', 'webp_animated.webp', 'png_alpha_00.png' ] ),
        ( 'all local files', CC.HYDRUS_LOCAL_FILE_STORAGE_SERVICE_KEY, [ 'jpeg_00.jpg', 'video_with_audio.mp4', 'video_silent.webm', 'tiff_image.tiff' ] ),
    ):

        location = ClientLocation.LocationContext.STATICCreateSimple( service_key )

        in_viewer = [ m for m in media_results if service_key in m.GetLocationsManager().GetCurrent() ]

        in_viewer_hashes = [ m.GetHash() for m in in_viewer ]

        picked = [ h for h in in_viewer_hashes if names[ h ] in picks ]

        holder = {}

        def open_viewer():

            frame = ClientGUICanvasFrame.CanvasFrame( gui )

            canvas = ClientGUICanvas.CanvasMediaListBrowser( frame, os.urandom( 32 ), location, in_viewer, picked[ 0 ] )

            frame.SetCanvas( canvas )

            holder[ 'frame' ] = frame
            holder[ 'canvas' ] = canvas


        qt( open_viewer )

        canvas = holder[ 'canvas' ]

        menus = []

        for h in picked:

            def show():

                media = canvas._media_list.GetMediaByHashes( { h } )[ 0 ]

                canvas.SetMedia( media )


            qt( show )

            # (the media container sets its zoom once laid out)
            time.sleep( 1.0 )

            def facts():

                container = canvas._media_container

                return {
                    'zoomable' : container.IsZoomable(),
                    'zoom' : container.GetCurrentZoom(),
                    'canvas_zoom' : container.GetCanvasZoom(),
                    'at_max_zoom' : container.IsAtMaxZoom(),
                    'fullscreen' : canvas.parentWidget().isFullScreen(),
                    'player' : container.GetCurrentMediaPlayerLabel(),
                }


            facts = qt( facts )

            captured.clear()

            qt( lambda: canvas.ShowMenuFromSignal( None ) )

            # (the share menu's md5, sha1 and sha512 fill in a moment later)
            time.sleep( 1.0 )

            menus.append( {
                'file' : names[ h ],
                'hash' : h.hex(),
                'viewer' : facts,
                'menu' : qt( lambda: tree( captured[ 0 ] ) ) if len( captured ) > 0 else None,
            } )


        qt( lambda: holder[ 'frame' ].close() )

        time.sleep( 0.5 )

        viewers.append( { 'viewer' : name, 'service_key' : service_key.hex(), 'files' : [ h.hex() for h in in_viewer_hashes ], 'menus' : menus } )


    return viewers


def main():

    import hydrus_driver
    import record_api

    db_dir = record_api.unpack_fixture( 'basic' )

    result = hydrus_driver.run_client( db_dir, record )

    with open( OUT, 'w' ) as f:

        json.dump( result, f, indent = 1, sort_keys = True, ensure_ascii = False )
        f.write( '\n' )


    print( f'wrote {OUT}' )


if __name__ == '__main__':

    main()

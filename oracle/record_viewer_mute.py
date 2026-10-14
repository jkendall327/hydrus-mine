#!/usr/bin/env python3
"""Record the reference's media viewer mute states.

A media viewer (`CanvasMediaListBrowser`) is opened on the `basic` fixture's
video with sound. For every combination of the global mute, the media
viewer's own mute, the option that the viewer uses its own volume, and the
viewer's forced mute (none, forced mute, forced unmute: "force mute just
here" and its siblings, `SetPerPlayerMuteState`), the real
`AddAudioVolumeMenu` builds the volume submenu (its tree recorded: entries,
separators as "---", the volume slider as `{"slider": label, "value": v}`),
and the viewer's media container says whether it plays muted
(`_GetCurrentMuteState`, the value it hands to the player's `SetMute`). The
forced states are also reached by their menu entries, by text, in the
captured submenu.

Usage: QT_QPA_PLATFORM=offscreen python oracle/record_viewer_mute.py
       (writes fixtures/viewer_mute.json)
"""

import itertools
import json
import os
import sys
import time

HERE = os.path.dirname( os.path.abspath( __file__ ) )

sys.path.insert( 0, HERE )

OUT = os.path.join( HERE, 'fixtures', 'viewer_mute.json' )
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

        elif action.isCheckable():

            out.append( { 'check' : action.text(), 'checked' : action.isChecked() } )

        else:

            out.append( action.text() )

    return out


def record( session ):

    controller = session.controller
    gui = controller.gui

    from hydrus.client import ClientConstants as CC
    from hydrus.client import ClientLocation
    from hydrus.client.gui import ClientGUIMenus
    from hydrus.client.gui.canvas import ClientGUICanvas
    from hydrus.client.gui.canvas import ClientGUICanvasFrame

    def qt( f ):

        return controller.CallBlockingToQt( gui, f )


    names = { bytes.fromhex( f[ 'hash' ] ) : f[ 'name' ] for f in MANIFEST[ 'files' ] }

    media_results = controller.Read( 'media_results', list( names.keys() ) )

    by_name = { names[ m.GetHash() ] : m for m in media_results }

    video = by_name[ 'audio.mp3' ]

    location = ClientLocation.LocationContext.STATICCreateSimple( CC.LOCAL_FILE_SERVICE_KEY )

    holder = {}

    def open_viewer():

        frame = ClientGUICanvasFrame.CanvasFrame( gui )

        canvas = ClientGUICanvas.CanvasMediaListBrowser( frame, os.urandom( 32 ), location, [ video ], video.GetHash() )

        frame.SetCanvas( canvas )

        holder[ 'frame' ] = frame
        holder[ 'canvas' ] = canvas

    qt( open_viewer )

    time.sleep( 1.5 )

    canvas = holder[ 'canvas' ]

    container = canvas._media_container

    options = controller.new_options

    keys = ( 'global_audio_mute', 'media_viewer_audio_mute', 'media_viewer_uses_its_own_audio_volume' )

    before = { k : options.GetBoolean( k ) for k in keys }

    cases = []

    try:

        for ( global_mute, viewer_mute, own_volume, forced ) in itertools.product( ( False, True ), ( False, True ), ( False, True ), ( None, True, False ) ):

            def one():

                options.SetBoolean( 'global_audio_mute', global_mute )
                options.SetBoolean( 'media_viewer_audio_mute', viewer_mute )
                options.SetBoolean( 'media_viewer_uses_its_own_audio_volume', own_volume )

                container.SetPerPlayerMuteState( forced )

                menu = ClientGUIMenus.GenerateMenu( gui )

                ClientGUICanvas.AddAudioVolumeMenu( menu, canvas.CANVAS_TYPE, container )

                return {
                    'global_mute' : global_mute,
                    'viewer_mute' : viewer_mute,
                    'viewer_uses_its_own_volume' : own_volume,
                    'forced' : forced,
                    'plays_muted' : container._GetCurrentMuteState(),
                    'menu' : tree( menu ),
                }

            cases.append( qt( one ) )

    finally:

        def restore():

            container.SetPerPlayerMuteState( None )

            for k in keys:

                options.SetBoolean( k, before[ k ] )

            holder[ 'frame' ].close()

        qt( restore )

        time.sleep( 0.5 )

    return { 'file' : 'audio.mp3', 'cases' : cases }


def child( out ):

    import hydrus_driver
    import record_api

    db_dir = record_api.unpack_fixture( 'basic' )

    result = hydrus_driver.run_client( db_dir, record )

    with open( out, 'w' ) as f:

        json.dump( result, f )


def main():

    if len( sys.argv ) > 1 and sys.argv[ 1 ] == '--child':

        child( sys.argv[ 2 ] )

        return

    import tempfile

    import hydrus_driver

    with tempfile.TemporaryDirectory() as work:

        path = os.path.join( work, 'viewer_mute.json' )

        hydrus_driver.run_in_subprocess( os.path.abspath( __file__ ), '--child', path )

        with open( path ) as f:

            result = json.load( f )

    with open( OUT, 'w' ) as f:

        json.dump( result, f, indent = 1 )
        f.write( '\n' )

    print( f'wrote {OUT}: {len( result[ "cases" ] )} cases' )


if __name__ == '__main__':

    main()

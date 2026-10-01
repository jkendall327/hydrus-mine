#!/usr/bin/env python3
"""Record the reference's media viewer zooms (`CalculateCanvasZooms`).

On a new client, for files of several types and sizes in canvases of
several sizes and device pixel ratios, the zoom of each zoom type the
media viewer has: its default for the file's type (the per-filetype
"scale up"/"scale down" rules), canvas fit, 100%, filling horizontally,
vertically, and the whole canvas.

Twice: with a new client's options, and with options a user might have
set: other zoom steps, small images shown at 100%, animations scaled to
the largest regular zoom that fits (exact zooms only), webm shown with an
"open externally" button, the zoom centred on the media and the media
viewer's default zoom overridden to fill the canvas. That phase saves the
options object too, for hydrus-rs to migrate.

Usage: QT_QPA_PLATFORM=offscreen python oracle/record_viewer_zoom.py
       (writes fixtures/viewer_zoom.json)
"""

import json
import os
import shutil
import sys
import tempfile

HERE = os.path.dirname( os.path.abspath( __file__ ) )

sys.path.insert( 0, HERE )

OUT = os.path.join( HERE, 'fixtures', 'viewer_zoom.json' )

# ( name, HC mime, resolution )
FILES = [
    ( 'small jpeg', 'IMAGE_JPEG', ( 300, 200 ) ),
    ( 'big jpeg', 'IMAGE_JPEG', ( 4000, 3000 ) ),
    ( 'wide png', 'IMAGE_PNG', ( 3000, 500 ) ),
    ( 'tall png', 'IMAGE_PNG', ( 500, 3000 ) ),
    ( 'canvas-sized png', 'IMAGE_PNG', ( 1000, 750 ) ),
    ( 'one side fits', 'IMAGE_PNG', ( 1000, 400 ) ),
    ( 'small gif', 'ANIMATION_GIF', ( 200, 150 ) ),
    ( 'big gif', 'ANIMATION_GIF', ( 2500, 2500 ) ),
    ( 'gif fitting four times', 'ANIMATION_GIF', ( 250, 150 ) ),
    ( 'ugoira', 'ANIMATION_UGOIRA', ( 640, 480 ) ),
    ( 'webm', 'VIDEO_WEBM', ( 640, 360 ) ),
    ( 'big mp4', 'VIDEO_MP4', ( 3840, 2160 ) ),
    ( 'mp3', 'AUDIO_MP3', ( None, None ) ),
    ( 'pdf', 'APPLICATION_PDF', ( 600, 800 ) ),
    ( 'zip', 'APPLICATION_ZIP', ( None, None ) ),
]

# ( width, height, device pixel ratio )
CANVASES = [ ( 1000, 750, 1.0 ), ( 1920, 1080, 1.0 ), ( 300, 300, 1.0 ), ( 1000, 750, 2.0 ) ]


class FakeMedia( object ):

    def __init__( self, mime, resolution ):

        self._mime = mime
        self._resolution = resolution


    def GetMime( self ):

        return self._mime


    def GetResolution( self ):

        return self._resolution


    def HasUsefulResolution( self ):

        ( width, height ) = self._resolution

        return width is not None and width != 0 and height is not None and height != 0



def customise( options ):

    from hydrus.core import HydrusConstants as HC
    from hydrus.client import ClientConstants as CC
    from hydrus.client.gui.canvas import ClientGUICanvasMedia

    options.SetMediaZooms( [ 0.25, 0.5, 1.0, 1.5, 3.0 ] )

    view = options.GetMediaViewOptions()

    ( show, paused, embed, p_show, p_paused, p_embed, zoom ) = view[ HC.GENERAL_IMAGE ]
    ( up, down, p_up, p_down, exact, up_quality, down_quality ) = zoom
    view[ HC.GENERAL_IMAGE ] = ( show, paused, embed, p_show, p_paused, p_embed, ( CC.MEDIA_VIEWER_SCALE_100, down, p_up, p_down, exact, up_quality, down_quality ) )

    ( show, paused, embed, p_show, p_paused, p_embed, zoom ) = view[ HC.GENERAL_ANIMATION ]
    ( up, down, p_up, p_down, exact, up_quality, down_quality ) = zoom
    view[ HC.GENERAL_ANIMATION ] = ( show, paused, embed, p_show, p_paused, p_embed, ( CC.MEDIA_VIEWER_SCALE_MAX_REGULAR, CC.MEDIA_VIEWER_SCALE_MAX_REGULAR, p_up, p_down, True, up_quality, down_quality ) )

    ( show, paused, embed, p_show, p_paused, p_embed, zoom ) = view[ HC.GENERAL_VIDEO ]
    view[ HC.VIDEO_WEBM ] = ( CC.MEDIA_VIEWER_ACTION_SHOW_OPEN_EXTERNALLY_BUTTON, paused, embed, p_show, p_paused, p_embed, zoom )

    options.SetMediaViewOptions( view )

    options.SetInteger( 'media_viewer_zoom_center', ClientGUICanvasMedia.ZOOM_CENTERPOINT_MEDIA_CENTER )
    options.SetInteger( 'media_viewer_default_zoom_type_override', ClientGUICanvasMedia.MEDIA_VIEWER_ZOOM_TYPE_FILL_AUTO )


def record( session, custom ):

    from qtpy import QtCore as QC

    from hydrus.core import HydrusConstants as HC
    from hydrus.client import ClientConstants as CC
    from hydrus.client.gui.canvas import ClientGUICanvasMedia

    options = session.controller.new_options

    if custom:

        customise( options )


    cases = []

    for ( name, mime_name, resolution ) in FILES:

        mime = getattr( HC, mime_name )

        media = FakeMedia( mime, resolution )

        ( show_action, start_paused, start_with_embed, *rest ) = options._GetMediaViewOptions( mime )

        for ( width, height, dpr ) in CANVASES:

            zooms = ClientGUICanvasMedia.CalculateCanvasZooms( QC.QSize( width, height ), CC.CANVAS_MEDIA_VIEWER, dpr, media, show_action )

            cases.append( {
                'file': name,
                'mime': mime,
                'resolution': list( resolution ),
                'canvas': [ width, height ],
                'device_pixel_ratio': dpr,
                'show_action': show_action,
                'zooms': { str( zoom_type ): zoom for ( zoom_type, zoom ) in zooms.items() },
            } )



    out = {
        'media_zooms': options.GetMediaZooms(),
        'zoom_center': options.GetInteger( 'media_viewer_zoom_center' ),
        'default_zoom_type': options.GetInteger( 'media_viewer_default_zoom_type_override' ),
        'cases': cases,
    }

    # (a new client's options are crates/hydrus-legacy's
    # client_options_defaults.json)
    if custom:

        out[ 'options' ] = json.loads( json.dumps( options.GetSerialisableTuple() ) )


    return out


def run_phase( custom, out_path ):

    import hydrus_driver

    work = tempfile.mkdtemp( prefix = 'hydrus_zoom_' )

    try:

        result = hydrus_driver.run_client( os.path.join( work, 'db' ), lambda s: record( s, custom ) )

    finally:

        shutil.rmtree( work, ignore_errors = True )


    with open( out_path, 'w' ) as f:

        json.dump( result, f )



def main():

    import hydrus_driver

    if len( sys.argv ) > 1 and sys.argv[1] == '--child':

        run_phase( sys.argv[2] == 'custom', sys.argv[3] )

        return


    phases = {}

    for name in ( 'defaults', 'custom' ):

        path = OUT + f'.{name}.tmp'

        hydrus_driver.run_in_subprocess( os.path.abspath( __file__ ), '--child', name, path )

        phases[ name ] = json.load( open( path ) )

        os.remove( path )


    with open( OUT, 'w' ) as f:

        json.dump( phases, f, indent = 1, sort_keys = True )
        f.write( '\n' )


    print( f'wrote {OUT}' )


if __name__ == '__main__':

    main()

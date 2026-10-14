#!/usr/bin/env python3
"""Record the reference's media viewer zooming and panning a picture.

A media viewer (`CanvasMediaListBrowser`, in its `CanvasFrame`) is opened on
one of the `basic` fixture's pictures with the frame at a fixed size. The
zoom is centred on the viewer's centre (the option), and the real
`MediaContainer` is asked to zoom in, out, to 100%, to the canvas, to the
largest zoom, to pan by a step (a twelfth of the smaller of the canvas and
itself), to the edges and centres, and to be moved by a drag
(`MoveDelta`): after each, its zoom, whether it is at its largest zoom, and
its place and size in the canvas. The frame is also made full screen and
back, recording whether the frame says it is.

Usage: QT_QPA_PLATFORM=offscreen python oracle/record_viewer_pan_zoom.py
       (writes fixtures/viewer_pan_zoom.json)
"""

import json
import os
import sys
import time

HERE = os.path.dirname( os.path.abspath( __file__ ) )

sys.path.insert( 0, HERE )

OUT = os.path.join( HERE, 'fixtures', 'viewer_pan_zoom.json' )
MANIFEST = json.load( open( os.path.join( HERE, 'fixtures', 'legacy_db', 'basic.manifest.json' ) ) )

FILE = 'dupe_jpeg_02.png'

SIZE = ( 800, 600 )


def record( session ):

    controller = session.controller
    gui = controller.gui

    from qtpy import QtCore as QC
    from qtpy import QtWidgets as QW

    from hydrus.client import ClientConstants as CC
    from hydrus.client import ClientApplicationCommand as CAC
    from hydrus.client import ClientLocation
    from hydrus.client.gui.canvas import ClientGUICanvas
    from hydrus.client.gui.canvas import ClientGUICanvasFrame
    from hydrus.client.gui.canvas import ClientGUICanvasMedia

    def qt( f ):

        return controller.CallBlockingToQt( gui, f )


    names = { bytes.fromhex( f[ 'hash' ] ) : f[ 'name' ] for f in MANIFEST[ 'files' ] }

    media_results = controller.Read( 'media_results', list( names.keys() ) )

    media_result = [ m for m in media_results if names[ m.GetHash() ] == FILE ][0]

    location = ClientLocation.LocationContext.STATICCreateSimple( CC.LOCAL_FILE_SERVICE_KEY )

    controller.new_options.SetInteger( 'media_viewer_zoom_center', ClientGUICanvasMedia.ZOOM_CENTERPOINT_VIEWER_CENTER )

    holder = {}

    def open_viewer():

        frame = ClientGUICanvasFrame.CanvasFrame( gui )

        canvas = ClientGUICanvas.CanvasMediaListBrowser( frame, os.urandom( 32 ), location, [ media_result ], media_result.GetHash() )

        frame.SetCanvas( canvas )

        frame.showNormal()

        frame.resize( SIZE[0], SIZE[1] )

        holder[ 'frame' ] = frame
        holder[ 'canvas' ] = canvas

    qt( open_viewer )

    time.sleep( 1.5 )

    frame = holder[ 'frame' ]
    canvas = holder[ 'canvas' ]

    def settle():

        QW.QApplication.processEvents()

    qt( settle )

    container = canvas._media_container

    def state():

        return {
            'zoom' : container._current_zoom,
            'at_max' : container.IsAtMaxZoom(),
            'rect' : [ container.pos().x(), container.pos().y(), container.width(), container.height() ],
        }

    steps = []

    def step( name, f = None ):

        def go():

            if f is not None:

                f()

            settle()

            return state()

        result = qt( go )

        result[ 'step' ] = name

        steps.append( result )

    center = ClientGUICanvasMedia.ZOOM_CENTERPOINT_VIEWER_CENTER

    result = {}

    try:

        result[ 'canvas' ] = qt( lambda: [ canvas.width(), canvas.height() ] )
        result[ 'file' ] = FILE
        result[ 'resolution' ] = list( media_result.GetResolution() )

        step( 'fitted' )

        for i in range( 3 ):

            step( f'zoom in {i + 1}', lambda: container.ZoomIn( center ) )

        for i in range( 4 ):

            step( f'zoom out {i + 1}', lambda: container.ZoomOut( center ) )

        step( 'switch', lambda: container.ZoomSwitch( center ) )
        step( 'switch again', lambda: container.ZoomSwitch( center ) )
        step( '100%', lambda: container.Zoom100( center ) )
        step( 'canvas', lambda: container.ZoomCanvas( center ) )
        step( 'default', lambda: container.ZoomDefault( center ) )
        step( 'max', lambda: container.ZoomMax() )
        step( 'zoom in at max', lambda: container.ZoomIn( center ) )
        step( 'out from max', lambda: container.ZoomOut( center ) )
        step( '100% again', lambda: container.Zoom100( center ) )

        step( 'zoom in for panning', lambda: container.ZoomIn( center ) )

        for ( name, dx, dy ) in ( ( 'pan right', 1, 0 ), ( 'pan right again', 1, 0 ), ( 'pan left', -1, 0 ), ( 'pan down', 0, 1 ), ( 'pan up', 0, -1 ), ( 'pan up again', 0, -1 ), ( 'pan diagonally', 1, 1 ) ):

            step( name, lambda dx = dx, dy = dy: container.DoManualPan( dx, dy ) )

        for ( name, pan_type ) in (
            ( 'to the left edge', CAC.SIMPLE_PAN_LEFT_EDGE ),
            ( 'to the right edge', CAC.SIMPLE_PAN_RIGHT_EDGE ),
            ( 'to the top edge', CAC.SIMPLE_PAN_TOP_EDGE ),
            ( 'to the bottom edge', CAC.SIMPLE_PAN_BOTTOM_EDGE ),
            ( 'to the horizontal centre', CAC.SIMPLE_PAN_HORIZONTAL_CENTER ),
            ( 'to the vertical centre', CAC.SIMPLE_PAN_VERTICAL_CENTER ),
        ):

            step( name, lambda pan_type = pan_type: container.DoEdgePan( pan_type ) )

        step( 'dragged', lambda: container.MoveDelta( QC.QPoint( -30, 30 ) ) )

        step( 'zoom in after dragging', lambda: container.ZoomIn( center ) )
        step( 'canvas after dragging', lambda: container.ZoomCanvas( center ) )

        # the frame's full screen
        def full():

            frame.showFullScreen()

            settle()

            return frame.isFullScreen()

        def not_full():

            frame.showNormal()

            settle()

            return frame.isFullScreen()

        result[ 'fullscreen' ] = {
            'before' : qt( lambda: frame.isFullScreen() ),
            'after full' : qt( full ),
            'after normal' : qt( not_full ),
        }

    finally:

        qt( lambda: frame.close() )

        time.sleep( 0.5 )

    result[ 'steps' ] = steps

    return result


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

        path = os.path.join( work, 'viewer_pan_zoom.json' )

        hydrus_driver.run_in_subprocess( os.path.abspath( __file__ ), '--child', path )

        with open( path ) as f:

            result = json.load( f )

    with open( OUT, 'w' ) as f:

        json.dump( result, f, indent = 1 )
        f.write( '\n' )

    print( f'wrote {OUT}: {len( result[ "steps" ] )} steps' )


if __name__ == '__main__':

    main()

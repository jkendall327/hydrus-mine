#!/usr/bin/env python3
"""Record what the reference's media viewer keeps of its window on close.

A media viewer (`CanvasMediaListBrowser`, in its `CanvasFrame`) is opened
with the `media_viewer` frame location remembering size and position, as a
window (not maximised or full screen) at 640x480, then resized, and then
closed, with "save media viewer window size and position on close" off and
on, and closing at once after the resize or a second after it: the frame
location in the options after each step (opened, resized, and
closed).

Usage: QT_QPA_PLATFORM=offscreen python oracle/record_viewer_close_geometry.py
       (writes fixtures/viewer_close_geometry.json)
"""

import json
import os
import sys
import time

HERE = os.path.dirname( os.path.abspath( __file__ ) )

sys.path.insert( 0, HERE )

OUT = os.path.join( HERE, 'fixtures', 'viewer_close_geometry.json' )
MANIFEST = json.load( open( os.path.join( HERE, 'fixtures', 'legacy_db', 'basic.manifest.json' ) ) )

OPENED_SIZE = ( 640, 480 )
RESIZED = ( 900, 650 )


def record( session ):

    controller = session.controller
    gui = controller.gui

    from qtpy import QtWidgets as QW

    from hydrus.client import ClientConstants as CC
    from hydrus.client import ClientLocation
    from hydrus.client.gui.canvas import ClientGUICanvas
    from hydrus.client.gui.canvas import ClientGUICanvasFrame

    def qt( f ):

        return controller.CallBlockingToQt( gui, f )


    names = { bytes.fromhex( f[ 'hash' ] ) : f[ 'name' ] for f in MANIFEST[ 'files' ] }

    media_results = controller.Read( 'media_results', list( names.keys() ) )

    media_result = [ m for m in media_results if names[ m.GetHash() ] == 'jpeg_00.jpg' ][0]

    location = ClientLocation.LocationContext.STATICCreateSimple( CC.LOCAL_FILE_SERVICE_KEY )

    options = controller.new_options

    def kept():

        ( remember_size, remember_position, last_size, last_position, gravity, position, maximised, fullscreen ) = options.GetFrameLocation( 'media_viewer' )

        return { 'remember_size' : remember_size, 'remember_position' : remember_position, 'last_size' : last_size, 'maximised' : maximised, 'fullscreen' : fullscreen, 'has_last_position' : last_position is not None }

    runs = []

    # ( what is done, whether it waits a second between resizing and closing )
    for ( scenario, settles ) in ( ( 'closed at once after resizing', False ), ( 'closed a second after resizing', True ) ):

        for save_on_close in ( False, True ):

            options.SetBoolean( 'save_media_viewer_window_size_and_position_on_close', save_on_close )
            options.SetFrameLocation( 'media_viewer', True, True, OPENED_SIZE, ( 70, 70 ), ( -1, -1 ), 'topleft', False, False )

            holder = {}

            def open_viewer():

                frame = ClientGUICanvasFrame.CanvasFrame( gui )

                canvas = ClientGUICanvas.CanvasMediaListBrowser( frame, os.urandom( 32 ), location, [ media_result ], media_result.GetHash() )

                frame.SetCanvas( canvas )

                frame.showNormal()

                QW.QApplication.processEvents()

                holder[ 'frame' ] = frame

            qt( open_viewer )

            time.sleep( 1.5 )

            frame = holder[ 'frame' ]

            run = { 'scenario' : scenario, 'save_on_close' : save_on_close }

            run[ 'opened' ] = qt( lambda: ( frame.width(), frame.height(), kept() ) )

            if settles:

                qt( lambda: frame.resize( RESIZED[0], RESIZED[1] ) )

                time.sleep( 1.0 )

                run[ 'resized' ] = qt( lambda: ( frame.width(), frame.height(), kept() ) )

                qt( lambda: frame.close() )

            else:

                def resize_and_close():

                    frame.resize( RESIZED[0], RESIZED[1] )

                    run[ 'resized' ] = ( frame.width(), frame.height(), kept() )

                    frame.close()

                qt( resize_and_close )

            time.sleep( 1.0 )

            run[ 'closed' ] = kept()

            runs.append( run )

    return { 'file' : 'jpeg_00.jpg', 'opened_size' : list( OPENED_SIZE ), 'resized_to' : list( RESIZED ), 'runs' : runs }


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

        path = os.path.join( work, 'viewer_close_geometry.json' )

        hydrus_driver.run_in_subprocess( os.path.abspath( __file__ ), '--child', path )

        with open( path ) as f:

            result = json.load( f )

    with open( OUT, 'w' ) as f:

        json.dump( result, f, indent = 1 )
        f.write( '\n' )

    print( f'wrote {OUT}: {len( result[ "runs" ] )} runs' )


if __name__ == '__main__':

    main()

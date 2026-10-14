#!/usr/bin/env python3
"""Record the reference's media viewer slideshow moving on, with time held still.

A media viewer (`CanvasMediaListBrowser`) is opened on four of the `basic`
fixture's pictures. The clock the viewer reads (`HydrusTime.GetNowFloat`) is
scripted, and the slideshow's own work (`_DoSlideshowWork`, the thing the
viewer's UI timer calls) is run at chosen times after the slideshow starts,
recording which file is shown after each tick:

- a plain slideshow, around its period, past the end of the list (it wraps);
- a shuffling one (which file it picks is random: recorded is that it moves
  at its period, never to the file it is on, and only to files of the list);
- one that is stopped (it never moves), and resumed;
- "play media once through" with a file that has a duration (the viewer's
  media container is told it is showing one, and whether it has played once
  through): it stays on the file after its period until it has played
  through, then moves on; with the option off it moves at its period.

Usage: QT_QPA_PLATFORM=offscreen python oracle/record_slideshow_run.py
       (writes fixtures/slideshow_run.json)
"""

import json
import os
import sys
import time

HERE = os.path.dirname( os.path.abspath( __file__ ) )

sys.path.insert( 0, HERE )

OUT = os.path.join( HERE, 'fixtures', 'slideshow_run.json' )
MANIFEST = json.load( open( os.path.join( HERE, 'fixtures', 'legacy_db', 'basic.manifest.json' ) ) )

NAMES = ( 'jpeg_00.jpg', 'jpeg_01.jpg', 'jpeg_02.jpg', 'jpeg_03.jpg' )

PERIOD = 10.0


def record( session ):

    import faulthandler

    faulthandler.dump_traceback_later( 50, exit = True, file = open( "/tmp/claude-0/fh.log", "w" ) )

    controller = session.controller
    gui = controller.gui

    from hydrus.core import HydrusTime

    from hydrus.client import ClientApplicationCommand as CAC
    from hydrus.client import ClientConstants as CC
    from hydrus.client import ClientLocation
    from hydrus.client.gui.canvas import ClientGUICanvas
    from hydrus.client.gui.canvas import ClientGUICanvasFrame

    def qt( f ):

        return controller.CallBlockingToQt( gui, f )


    names = { bytes.fromhex( f[ 'hash' ] ) : f[ 'name' ] for f in MANIFEST[ 'files' ] }

    media_results = controller.Read( 'media_results', list( names.keys() ) )

    by_name = { names[ m.GetHash() ] : m for m in media_results }

    in_viewer = [ by_name[ name ] for name in NAMES ]

    location = ClientLocation.LocationContext.STATICCreateSimple( CC.LOCAL_FILE_SERVICE_KEY )

    clock = { 'now' : 1000.0 }

    real_now = HydrusTime.GetNowFloat

    HydrusTime.GetNowFloat = lambda: clock[ 'now' ]

    holder = {}

    def open_viewer():

        frame = ClientGUICanvasFrame.CanvasFrame( gui )

        canvas = ClientGUICanvas.CanvasMediaListBrowser( frame, os.urandom( 32 ), location, in_viewer, in_viewer[0].GetHash() )

        frame.SetCanvas( canvas )

        holder[ 'frame' ] = frame
        holder[ 'canvas' ] = canvas


    qt( open_viewer )

    time.sleep( 1.0 )

    canvas = holder[ 'canvas' ]

    def shown():

        return names[ canvas._current_media.GetHash() ] if canvas._current_media is not None else None


    def command( action, data = None ):

        qt( lambda: canvas.ProcessApplicationCommand( CAC.ApplicationCommand.STATICCreateSimpleCommand( action, data ) ) )


    def go_to( name ):

        media = [ m for m in canvas._media_list.GetSortedMedia() if names[ m.GetHash() ] == name ][0]

        qt( lambda: canvas.SetMedia( media ) )


    def tick( at ):

        def f():

            clock[ 'now' ] = at

            canvas._DoSlideshowWork()

            return shown()

        return qt( f )


    def reset( shuffle, once_through ):

        def f():

            canvas._StopSlideshow()
            canvas._slideshow_is_shuffling = shuffle
            canvas._slideshow_is_playing_once_through = once_through

        qt( f )

        go_to( NAMES[0] )

    scenarios = []

    try:

        # a plain slideshow: the times after the start at which the work runs
        reset( False, False )

        start = 2000.0

        clock[ 'now' ] = start

        command( CAC.SIMPLE_START_SLIDESHOW, PERIOD )

        offsets = [ 0.0, 5.0, 9.9, 10.0, 10.5, 19.9, 20.0, 29.0, 30.0, 40.0, 50.0 ]

        plain = [ { 'at' : o, 'shown' : tick( start + o ) } for o in offsets ]

        scenarios.append( { 'name' : 'plain', 'period' : PERIOD, 'offsets' : offsets, 'ticks' : plain } )

        # stopped: never moves; resumed from the time it is resumed
        command( CAC.SIMPLE_PAUSE_PLAY_SLIDESHOW )

        stopped = [ { 'at' : o, 'shown' : tick( start + 100.0 + o ) } for o in ( 0.0, 10.0, 100.0 ) ]

        resumed_at = start + 300.0

        clock[ 'now' ] = resumed_at

        command( CAC.SIMPLE_PAUSE_PLAY_SLIDESHOW )

        before = shown()

        resumed = [ { 'at' : o, 'shown' : tick( resumed_at + o ) } for o in ( 0.0, 9.9, 10.0 ) ]

        scenarios.append( { 'name' : 'stopped and resumed', 'stopped' : stopped, 'resumed_from' : before, 'resumed' : resumed } )

        # shuffling
        reset( True, False )

        clock[ 'now' ] = start

        command( CAC.SIMPLE_START_SLIDESHOW, PERIOD )

        shuffled = []

        last = start

        previous = shown()

        for i in range( 20 ):

            early = tick( last + PERIOD - 0.1 )

            moved_before_period = early != previous

            last = last + PERIOD + 0.5

            now_shown = tick( last )

            shuffled.append( { 'moved_before_period' : moved_before_period, 'from' : previous, 'to' : now_shown } )

            previous = now_shown

        scenarios.append( { 'name' : 'shuffle', 'period' : PERIOD, 'ticks' : shuffled } )

        # once through, on a file with a duration (the container is told
        # whether it has played once through; the audio file is a few seconds long
        # and the options that bend the period are off)
        options = controller.new_options

        keys = ( 'slideshow_short_duration_loop_percentage', 'slideshow_short_duration_loop_seconds', 'slideshow_short_duration_cutoff_percentage', 'slideshow_long_duration_overspill_percentage' )

        before = { k : options.GetNoneableInteger( k ) for k in keys }

        for k in keys:

            options.SetNoneableInteger( k, None )

        video_name = 'audio.mp3'

        qt( lambda: canvas._StopSlideshow() )

        qt( lambda: holder[ 'frame' ].close() )

        def open_video_viewer():

            video_list = [ by_name[ video_name ], by_name[ NAMES[0] ] ]

            frame = ClientGUICanvasFrame.CanvasFrame( gui )

            canvas = ClientGUICanvas.CanvasMediaListBrowser( frame, os.urandom( 32 ), location, video_list, video_list[0].GetHash() )

            frame.SetCanvas( canvas )

            holder[ 'frame' ] = frame
            holder[ 'canvas' ] = canvas

        qt( open_video_viewer )

        time.sleep( 1.0 )

        canvas = holder[ 'canvas' ]

        once = []

        for once_through in ( True, False ):

            reset( False, once_through )

            go_to( video_name )

            container = canvas._media_container

            state = { 'played' : False }

            container.CurrentlyPresentingMediaWithDuration = lambda: names[ canvas._current_media.GetHash() ] == video_name
            container.HasPlayedOnceThrough = lambda: state[ 'played' ]
            container.StopForSlideshow = lambda value: None

            clock[ 'now' ] = start

            command( CAC.SIMPLE_START_SLIDESHOW, PERIOD )

            ticks = []

            for ( offset, played ) in ( ( 0.0, False ), ( 9.9, False ), ( 10.5, False ), ( 25.0, False ), ( 300.0, False ), ( 301.0, True ) ):

                state[ 'played' ] = played

                ticks.append( { 'at' : offset, 'played_once_through' : played, 'shown' : tick( start + offset ) } )

            once.append( { 'once_through' : once_through, 'ticks' : ticks } )

            del container.CurrentlyPresentingMediaWithDuration
            del container.HasPlayedOnceThrough
            del container.StopForSlideshow

        for k in keys:

            options.SetNoneableInteger( k, before[ k ] )

        scenarios.append( { 'name' : 'file with a duration', 'period' : PERIOD, 'files' : [ video_name, NAMES[0] ], 'runs' : once } )

    finally:

        HydrusTime.GetNowFloat = real_now

        def stop():

            canvas._StopSlideshow()

            holder[ 'frame' ].close()

        qt( stop )

        time.sleep( 0.5 )

    return { 'names' : list( NAMES ), 'scenarios' : scenarios }


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

        path = os.path.join( work, 'slideshow_run.json' )

        hydrus_driver.run_in_subprocess( os.path.abspath( __file__ ), '--child', path )

        with open( path ) as f:

            result = json.load( f )

    with open( OUT, 'w' ) as f:

        json.dump( result, f, indent = 1 )
        f.write( '\n' )

    print( f'wrote {OUT}: {len( result[ "scenarios" ] )} scenarios' )


if __name__ == '__main__':

    main()

#!/usr/bin/env python3
"""Record the reference's media viewer slideshow.

In the running client:

- the period a file with a duration bends a slideshow's to is worked out
  by the viewer's own `_CalculateAnySpecialSlideshowPeriodForCurrentMedia`
  (on a stand-in viewer, playing a file of that duration) for many
  durations, periods and sets of the slideshow options (the short loop
  percentage and seconds, the short cutoff percentage, the long overspill
  percentage, each possibly off), recording the period it sets, whether it
  tells the player to stop at the file's end, and whether it stops the
  slideshow;
- a media viewer (`CanvasMediaListBrowser`) is opened on the `basic`
  fixture's files in "my files", and its own `ShowMenuFromSignal` builds
  the menu (captured rather than shown) as its slideshow runs, stops,
  shuffles and so on, recording the slideshow submenu's tree: each entry's
  text, separators as "---", and checkable entries as
  `{"check": text, "checked": bool}`.

Usage: QT_QPA_PLATFORM=offscreen python oracle/record_slideshow.py
       (writes fixtures/slideshow.json)
"""

import json
import os
import sys
import time
import types

HERE = os.path.dirname( os.path.abspath( __file__ ) )

sys.path.insert( 0, HERE )

OUT = os.path.join( HERE, 'fixtures', 'slideshow.json' )
MANIFEST = json.load( open( os.path.join( HERE, 'fixtures', 'legacy_db', 'basic.manifest.json' ) ) )

OPTION_KEYS = (
    'slideshow_short_duration_loop_percentage',
    'slideshow_short_duration_loop_seconds',
    'slideshow_short_duration_cutoff_percentage',
    'slideshow_long_duration_overspill_percentage',
)

# each: the four options above, in order (None: off)
OPTION_SETS = (
    ( 20, 10, 75, 50 ), # a new client's
    ( None, None, None, None ),
    ( 20, None, None, None ),
    ( None, 10, None, None ),
    ( None, None, 75, None ),
    ( None, None, None, 50 ),
    ( 50, 3, 90, 100 ),
    ( 5, 30, 10, 0 ),
    ( 100, 1, 99, 1 ),
)

DURATIONS = (
    None, 0.0, 0.033, 0.5, 1.0, 2.0, 2.9, 3.0, 4.0, 5.0, 6.0, 7.5, 9.0, 10.0, 11.0, 12.0, 15.0,
    20.0, 22.5, 25.0, 29.0, 30.0, 31.0, 44.9, 45.0, 46.0, 50.0, 59.0, 60.0, 61.0, 89.0, 90.0,
    91.0, 120.0, 3600.0,
)

PERIODS = ( 0.08, 1.0, 5.0, 10.0, 15.0, 30.0, 60.0 )


def tree( menu ):

    out = []

    for action in menu.actions():

        if action.isSeparator():

            out.append( '---' )

        elif action.menu() is not None:

            out.append( { 'menu' : action.text(), 'entries' : tree( action.menu() ) } )

        elif action.isCheckable():

            out.append( { 'check' : action.text(), 'checked' : action.isChecked() } )

        else:

            out.append( action.text() )


    return out


def record( session ):

    controller = session.controller

    return {
        'periods' : record_periods( controller ),
        'menus' : record_menus( controller, controller.gui ),
    }


def record_periods( controller ):

    from hydrus.client.gui.canvas import ClientGUICanvas

    new_options = controller.new_options

    before = [ new_options.GetNoneableInteger( key ) for key in OPTION_KEYS ]

    cases = []

    try:

        for option_set in OPTION_SETS:

            for ( key, value ) in zip( OPTION_KEYS, option_set ):

                new_options.SetNoneableInteger( key, value )


            for period in PERIODS:

                for duration in DURATIONS:

                    calls = []

                    viewer = types.SimpleNamespace()

                    viewer._media_container = types.SimpleNamespace(
                        CurrentlyPresentingMediaWithDuration = lambda: True,
                        StopForSlideshow = lambda value: calls.append( ( 'stop_for_slideshow', value ) ),
                    )

                    result = types.SimpleNamespace( GetDurationS = ( lambda d = duration: d ) )

                    viewer._current_media = types.SimpleNamespace( GetMediaResult = ( lambda r = result: r ) )
                    viewer._normal_slideshow_period = period
                    viewer._special_slideshow_period_for_current_media = None
                    viewer._StopSlideshow = lambda: calls.append( ( 'stop', ) )

                    ClientGUICanvas.CanvasMediaListBrowser._CalculateAnySpecialSlideshowPeriodForCurrentMedia( viewer )

                    cases.append( {
                        'options' : list( option_set ),
                        'period' : period,
                        'duration' : duration,
                        'special' : viewer._special_slideshow_period_for_current_media,
                        'stop_at_end' : ( 'stop_for_slideshow', True ) in calls,
                        'stopped' : ( 'stop', ) in calls,
                    } )




    finally:

        for ( key, value ) in zip( OPTION_KEYS, before ):

            new_options.SetNoneableInteger( key, value )



    return cases


def record_menus( controller, gui ):

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

    service_key = CC.LOCAL_FILE_SERVICE_KEY

    location = ClientLocation.LocationContext.STATICCreateSimple( service_key )

    in_viewer = [ m for m in media_results if service_key in m.GetLocationsManager().GetCurrent() ]

    first = [ m.GetHash() for m in in_viewer if names[ m.GetHash() ] == 'jpeg_00.jpg' ][0]

    captured = []

    core = CGC.core()

    def capture( widget, menu ):

        captured.append( menu )


    core.PopupMenu = capture

    holder = {}

    def open_viewer():

        frame = ClientGUICanvasFrame.CanvasFrame( gui )

        canvas = ClientGUICanvas.CanvasMediaListBrowser( frame, os.urandom( 32 ), location, in_viewer, first )

        frame.SetCanvas( canvas )

        holder[ 'frame' ] = frame
        holder[ 'canvas' ] = canvas


    qt( open_viewer )

    time.sleep( 1.0 )

    canvas = holder[ 'canvas' ]

    new_options = controller.new_options

    shuffle_before = new_options.GetBoolean( 'slideshows_progress_randomly' )
    once_before = new_options.GetBoolean( 'slideshow_always_play_duration_media_once_through' )

    def slideshow_menu():

        captured.clear()

        canvas.ShowMenuFromSignal( None )

        for action in captured[0].actions():

            if action.menu() is not None and action.text() in ( 'start slideshow', 'slideshow running' ):

                return { 'menu' : action.text(), 'entries' : tree( action.menu() ) }



        return None


    # each: what is done to the viewer, then its menu (done and read at
    # once, so no slideshow moves on between)
    from hydrus.client import ClientApplicationCommand as CAC

    def simple( action, data = None ):

        return lambda: canvas.ProcessApplicationCommand( CAC.ApplicationCommand.STATICCreateSimpleCommand( action, data ) )


    steps = (
        ( 'fresh', [] ),
        ( 'started at a minute', [ simple( CAC.SIMPLE_START_SLIDESHOW, 60.0 ) ] ),
        ( 'stopped', [ simple( CAC.SIMPLE_PAUSE_PLAY_SLIDESHOW ) ] ),
        ( 'resumed', [ simple( CAC.SIMPLE_PAUSE_PLAY_SLIDESHOW ) ] ),
        ( 'this one shuffles', [ simple( CAC.SIMPLE_FLIP_THISWINDOW_SLIDESHOW_SHUFFLE ) ] ),
        ( 'this one plays through', [ simple( CAC.SIMPLE_FLIP_THISWINDOW_SLIDESHOW_ALWAYS_PLAY_DURATION_MEDIA_ONCE_THROUGH ) ] ),
        ( 'very fast, stopped', [ simple( CAC.SIMPLE_START_SLIDESHOW, 0.08 ), simple( CAC.SIMPLE_PAUSE_PLAY_SLIDESHOW ) ] ),
        ( 'two and a half seconds', [ simple( CAC.SIMPLE_START_SLIDESHOW, 2.5 ) ] ),
        ( 'every one shuffles', [ simple( CAC.SIMPLE_FLIP_GLOBAL_SLIDESHOW_SHUFFLE ) ] ),
        ( 'every one plays through', [ simple( CAC.SIMPLE_FLIP_GLOBAL_SLIDESHOW_ALWAYS_PLAY_DURATION_MEDIA_ONCE_THROUGH ) ] ),
        ( 'every one shuffles no more', [ simple( CAC.SIMPLE_FLIP_GLOBAL_SLIDESHOW_SHUFFLE ) ] ),
        ( 'started at nothing', [ simple( CAC.SIMPLE_START_SLIDESHOW, 0.0 ) ] ),
        ( 'two hours', [ simple( CAC.SIMPLE_START_SLIDESHOW, 7200.0 ), simple( CAC.SIMPLE_PAUSE_PLAY_SLIDESHOW ) ] ),
    )

    menus = []

    try:

        for ( name, actions ) in steps:

            def step():

                for action in actions:

                    action()


                return slideshow_menu()


            menus.append( { 'step' : name, 'menu' : qt( step ) } )


    finally:

        def stop():

            canvas._StopSlideshow()

            holder[ 'frame' ].close()


        qt( stop )

        new_options.SetBoolean( 'slideshows_progress_randomly', shuffle_before )
        new_options.SetBoolean( 'slideshow_always_play_duration_media_once_through', once_before )

        time.sleep( 0.5 )


    return menus


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

#!/usr/bin/env python3
"""Record what the reference saves of the sidebar and preview sizes on exit.

On the `basic` fixture, in a client whose window has been shown, a search page's
sidebar and preview are moved to a size other than the saved one, "save current
page's sidebar/preview size on client exit" (`saving_sash_positions_on_exit`) is
set on or off, and the frame's real `SaveAndHide` (what `_Exit` runs on the way
out, after its confirmation) is called. The saved sizes (`hpos`, `vpos`) before
and after are recorded. One client per case, as `SaveAndHide` runs once.

Usage: QT_QPA_PLATFORM=offscreen python oracle/record_sidebar_exit_save.py
       (writes fixtures/sidebar_exit_save.json)
"""

import json
import os
import sys
import tempfile
import time

HERE = os.path.dirname( os.path.abspath( __file__ ) )

sys.path.insert( 0, HERE )

OUT = os.path.join( HERE, 'fixtures', 'sidebar_exit_save.json' )

SAVED = ( 400, -240 )
MOVED = ( 510, -310 )


def record( enabled ):

    def hook( session ):

        from hydrus.core import HydrusConstants as HC
        from hydrus.client import ClientConstants as CC

        controller = session.controller
        gui = controller.gui

        def qt( f ):

            return controller.CallBlockingToQt( gui, f )


        def setup():

            gui.resize( 1400, 1000 )

            HC.options[ 'hpos' ], HC.options[ 'vpos' ] = SAVED

            controller.new_options.SetBoolean( 'saving_sash_positions_on_exit', enabled )

            page = gui._notebook.GetCurrentMediaPage()

            page.SetSplitterPositions( *MOVED )


        qt( setup )

        time.sleep( 0.5 )

        def state():

            page = gui._notebook.GetCurrentMediaPage()

            return {
                'saved' : [ HC.options[ 'hpos' ], HC.options[ 'vpos' ] ],
                'page' : list( page.GetSashPositions() ),
                'have_shown_once' : gui._have_shown_once
            }


        before = qt( state )

        qt( gui.SaveAndHide )

        after = qt( state )

        return { 'saving_sash_positions_on_exit' : enabled, 'before' : before, 'after' : after }


    return hook


def main():

    import hydrus_driver
    import record_api

    if len( sys.argv ) > 1 and sys.argv[ 1 ] == '--child':

        # (the driver replaces sys.argv)
        ( _, _, flag, path ) = sys.argv[ : 4 ]

        enabled = flag == 'on'

        result = hydrus_driver.run_client( record_api.unpack_fixture( 'basic' ), record( enabled ) )

        with open( path, 'w' ) as f:

            json.dump( result, f )


        return


    cases = []

    with tempfile.TemporaryDirectory() as directory:

        for flag in [ 'on', 'off' ]:

            path = os.path.join( directory, flag + '.json' )

            hydrus_driver.run_in_subprocess( os.path.abspath( __file__ ), '--child', flag, path )

            with open( path ) as f:

                cases.append( json.load( f ) )



    with open( OUT, 'w' ) as f:

        json.dump( { 'initial' : list( SAVED ), 'moved' : list( MOVED ), 'cases' : cases }, f, indent = 1, sort_keys = True )
        f.write( '\n' )


    print( f'wrote {OUT}' )


if __name__ == '__main__':

    main()

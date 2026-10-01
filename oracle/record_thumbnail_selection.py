#!/usr/bin/env python3
"""Record the reference's thumbnail grid selection.

In the running client, a page of the `basic` fixture's files is opened on
"my files", and its grid (v688's default, `MediaResultsPanelThumbnails-
GraphicsViewTest`) is driven through a script: clicks (plain, ctrl,
shift; on a file or on none, through `_HitMedia` as a mouse press does),
select all and none, the thumbnail shortcuts' focus moves (through
`ProcessApplicationCommand`, as the arrows, page up and down, home and
end do) and files leaving the page (`_RemoveMediaDirectly`, as a delete
does). After each step, the files selected and the one focused are
recorded, files named by their place in the page at the start. The grid's
row length and rows a page are recorded too. A new client's options.

Usage: QT_QPA_PLATFORM=offscreen python oracle/record_thumbnail_selection.py
       (writes fixtures/thumbnail_selection.json)
"""

import json
import os
import sys
import time

HERE = os.path.dirname( os.path.abspath( __file__ ) )

sys.path.insert( 0, HERE )

OUT = os.path.join( HERE, 'fixtures', 'thumbnail_selection.json' )

# ( 'click', file or None, ctrl, shift ), ( 'all', ), ( 'none', ),
# ( 'move', direction, shift ), ( 'remove', [ files ] )
SCRIPT = [
    # nothing focused yet: the keyboard moves from a ctrl+clicked file
    ( 'click', 4, True, False ),
    ( 'move', 'right', False ),
    ( 'click', 2, False, False ),
    ( 'click', 5, True, False ),
    ( 'move', 'right', False ),
    ( 'click', 9, False, True ),
    ( 'click', 4, False, True ),
    ( 'click', 5, True, False ),
    ( 'click', 8, False, True ),
    ( 'move', 'down', True ),
    ( 'move', 'up', True ),
    ( 'click', 7, False, False ),
    ( 'click', 7, True, False ),
    ( 'move', 'left', False ),
    ( 'all', ),
    ( 'move', 'home', True ),
    ( 'none', ),
    ( 'move', 'right', False ),
    ( 'click', 3, False, False ),
    ( 'click', 6, False, True ),
    ( 'remove', [ 4, 5 ] ),
    ( 'move', 'right', False ),
    ( 'click', 9, False, False ),
    ( 'click', 10, True, False ),
    ( 'click', 8, True, False ),
    ( 'remove', [ 9 ] ),
    ( 'move', 'right', False ),
    ( 'remove', [ 8 ] ),
    ( 'move', 'left', False ),
    ( 'move', 'page_down', False ),
    ( 'move', 'page_up', True ),
    ( 'move', 'end', True ),
    ( 'click', 0, True, True ),
    ( 'move', 'home', False ),
    ( 'click', None, True, False ),
    ( 'click', None, False, False ),
    ( 'move', 'left', False ),
    ( 'click', 1, True, False ),
    ( 'click', 3, True, False ),
    ( 'click', 2, False, True ),
    ( 'all', ),
    ( 'click', 6, True, False ),
    ( 'none', ),
    ( 'move', 'up', False ),
    # clicking the focused file again leaves the keyboard where a ctrl+click
    # put it
    ( 'click', 2, False, False ),
    ( 'click', 6, True, False ),
    ( 'click', 2, False, False ),
    ( 'move', 'right', False ),
    # selecting all with nothing focused ends shift-selecting
    ( 'click', None, False, False ),
    ( 'click', 3, True, False ),
    ( 'all', ),
    ( 'click', 10, False, True ),
    # the focused file ctrl+clicked away: shift and the keyboard still move
    # from it
    ( 'click', 12, False, False ),
    ( 'click', 12, True, False ),
    ( 'move', 'right', True ),
    # a ctrl+clicked file leaving: the keyboard moves from the focused one
    ( 'none', ),
    ( 'click', 14, False, False ),
    ( 'click', 16, True, False ),
    ( 'remove', [ 16 ] ),
    ( 'move', 'right', True ),
]


MANIFEST = json.load( open( os.path.join( HERE, 'fixtures', 'legacy_db', 'basic.manifest.json' ) ) )


def record( session ):

    from hydrus.client import ClientConstants as CC
    from hydrus.client import ClientLocation
    from hydrus.client import ClientApplicationCommand as CAC
    from hydrus.client.media import ClientMediaFileFilter

    controller = session.controller
    gui = controller.gui

    def qt( f ):

        return controller.CallBlockingToQt( gui, f )


    hashes = [ bytes.fromhex( f[ 'hash' ] ) for f in MANIFEST[ 'files' ] ]

    location = ClientLocation.LocationContext.STATICCreateSimple( CC.LOCAL_FILE_SERVICE_KEY )

    # (wide enough for several thumbnails a row)
    qt( lambda: gui.resize( 1400, 900 ) )

    page = qt( lambda: gui._notebook.NewPageQuery( location, initial_hashes = hashes ) )

    panel = None

    for _ in range( 600 ):

        candidate = qt( lambda: page.GetMediaResultsPanel() )

        if hasattr( candidate, '_thumbnail_layout' ) and len( candidate._sorted_media ) > 0:

            panel = candidate

            break


        time.sleep( 0.05 )


    if panel is None:

        raise Exception( 'the page never loaded' )


    # let it lay itself out
    time.sleep( 1 )

    media = qt( lambda: list( panel._sorted_media ) )

    index_of = { m : i for ( i, m ) in enumerate( media ) }

    moves = {
        'left' : CAC.MOVE_LEFT,
        'right' : CAC.MOVE_RIGHT,
        'up' : CAC.MOVE_UP,
        'down' : CAC.MOVE_DOWN,
        'page_up' : CAC.MOVE_PAGE_UP,
        'page_down' : CAC.MOVE_PAGE_DOWN,
        'home' : CAC.MOVE_HOME,
        'end' : CAC.MOVE_END,
    }

    def do( step ):

        ( action, *args ) = step

        if action == 'click':

            ( i, ctrl, shift ) = args

            panel._HitMedia( None if i is None else media[ i ], ctrl, shift )

        elif action == 'all':

            panel._Select( ClientMediaFileFilter.FileFilter( ClientMediaFileFilter.FILE_FILTER_ALL ) )

        elif action == 'none':

            panel._Select( ClientMediaFileFilter.FileFilter( ClientMediaFileFilter.FILE_FILTER_NONE ) )

        elif action == 'move':

            ( direction, shift ) = args

            status = CAC.SELECTION_STATUS_SHIFT if shift else CAC.SELECTION_STATUS_NORMAL

            command = CAC.ApplicationCommand.STATICCreateSimpleCommand( CAC.SIMPLE_MOVE_THUMBNAIL_FOCUS, simple_data = ( moves[ direction ], status ) )

            panel.ProcessApplicationCommand( command )

        elif action == 'remove':

            ( files, ) = args

            panel._RemoveMediaDirectly( { media[ i ] for i in files }, set() )


        return {
            'step' : list( step ),
            'selected' : sorted( index_of[ m ] for m in panel._selected_media ),
            'focused' : None if panel._focused_media is None else index_of[ panel._focused_media ],
        }


    columns = qt( lambda: panel._thumbnail_layout._thumbs_in_a_row )
    page_rows = qt( lambda: panel._thumbnail_layout._thumbs_in_a_col )

    steps = [ qt( lambda: do( step ) ) for step in SCRIPT ]

    return { 'files' : len( media ), 'columns' : columns, 'page_rows' : page_rows, 'steps' : steps }


def main():

    import hydrus_driver
    import record_api

    db_dir = record_api.unpack_fixture( 'basic' )

    result = hydrus_driver.run_client( db_dir, record )

    with open( OUT, 'w' ) as f:

        json.dump( result, f, indent = 1, sort_keys = True )
        f.write( '\n' )


    print( f'wrote {OUT}' )


if __name__ == '__main__':

    main()

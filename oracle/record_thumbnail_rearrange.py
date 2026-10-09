#!/usr/bin/env python3
"""Record the reference's thumbnail "rearrange" menu.

In the running client, a page of the `basic` fixture's files is opened and,
for several selections (thumbnails hit in order, the last one the focus),
the grid's own `GetMenu` is built and its "rearrange" submenu recorded, and
each of the five moves (to start, back one, to here, forward one, to end:
`SIMPLE_REARRANGE_THUMBNAILS`) is run through the panel's
`ProcessApplicationCommand` to record the order the files end in and which
are still selected; the order is put back between moves.

Usage: QT_QPA_PLATFORM=offscreen python oracle/record_thumbnail_rearrange.py
       (writes fixtures/thumbnail_rearrange.json)
"""

import json
import os
import sys
import time

HERE = os.path.dirname( os.path.abspath( __file__ ) )

sys.path.insert( 0, HERE )

OUT = os.path.join( HERE, 'fixtures', 'thumbnail_rearrange.json' )
MANIFEST = json.load( open( os.path.join( HERE, 'fixtures', 'legacy_db', 'basic.manifest.json' ) ) )

# thumbnails hit in order (the first plain, the rest with ctrl): the last is the focus
CASES = [
    [ 5, 6 ],
    [ 6, 5 ],
    [ 5, 1 ],
    [ 0 ],
    [ 0, 1 ],
    [ 2 ],
    [ 7, 3, 9 ],
    [ 'last' ],
    [ 'last', 'before last' ],
    [ 1, 2, 3, 4 ],
]

MOVES = [ 'to start', 'back one', 'to here', 'forward one', 'to end' ]


def tree( menu ):

    out = []

    for action in menu.actions():

        if action.isSeparator():

            out.append( '---' )

        elif action.menu() is not None:

            out.append( { 'menu' : action.text(), 'entries' : tree( action.menu() ) } )

        else:

            out.append( action.text() )


    return out


def record( session ):

    from hydrus.core import HydrusConstants as HC
    from hydrus.client import ClientApplicationCommand as CAC
    from hydrus.client import ClientConstants as CC
    from hydrus.client import ClientLocation
    from hydrus.client.media import ClientMediaFileFilter

    controller = session.controller
    gui = controller.gui

    hashes = [ bytes.fromhex( f[ 'hash' ] ) for f in MANIFEST[ 'files' ] ]

    # (a file in no local file domain, deleted or in the trash, is not a thumbnail on
    # hydrus-rs's page of the local files)
    hashes = [ m.GetHash() for m in controller.Read( 'media_results', hashes ) if any( controller.services_manager.GetServiceType( k ) == HC.LOCAL_FILE_DOMAIN for k in m.GetLocationsManager().GetCurrent() ) ]

    def qt( f ):

        return controller.CallBlockingToQt( gui, f )


    location = ClientLocation.LocationContext.STATICCreateSimple( CC.LOCAL_FILE_SERVICE_KEY )

    page = qt( lambda: gui._notebook.NewPageQuery( location, initial_hashes = hashes ) )

    panel = None

    for _ in range( 600 ):

        panel = qt( lambda: page.GetMediaResultsPanel() )

        if hasattr( panel, 'GetMenu' ) and len( panel._sorted_media ) > 0:

            break


        time.sleep( 0.05 )


    # (sorted by file size, smallest first, which no two files share: the
    # order hydrus-rs can put its own page in, whatever its ties do)
    from hydrus.client.media import ClientMediaSort

    media_sort = ClientMediaSort.MediaSort( sort_type = ( 'system', CC.SORT_FILES_BY_FILESIZE ), sort_order = CC.SORT_ASC )

    qt( lambda: panel.Sort( media_sort ) )

    time.sleep( 0.3 )

    original = qt( lambda: list( panel._sorted_media ) )

    sizes = qt( lambda: [ m.GetSize() for m in panel._sorted_media ] )

    assert sizes == sorted( set( sizes ) ), 'the sizes tie'

    def order():

        return [ m.GetHash().hex() for m in panel._sorted_media ]

    def selected():

        return [ m.GetHash().hex() for m in panel._sorted_media if m in panel._selected_media ]

    initial = qt( order )

    commands = {
        'to start' : CAC.MOVE_HOME,
        'back one' : CAC.MOVE_LEFT,
        'to here' : CAC.MOVE_TO_FOCUS,
        'forward one' : CAC.MOVE_RIGHT,
        'to end' : CAC.MOVE_END,
    }

    cases = []

    for case in CASES:

        indices = [ len( original ) - 1 if i == 'last' else ( len( original ) - 2 if i == 'before last' else i ) for i in case ]

        picked = [ original[ i ] for i in indices ]

        def select():

            panel._Select( ClientMediaFileFilter.FileFilter( ClientMediaFileFilter.FILE_FILTER_NONE ) )

            for ( i, m ) in enumerate( picked ):

                panel._HitMedia( m, i > 0, False )


        qt( select )

        menu = qt( lambda: tree( panel.GetMenu() ) )

        offered = [ e[ 'entries' ] for e in menu if isinstance( e, dict ) and e[ 'menu' ] == 'rearrange' ]

        record_case = {
            'indices' : indices,
            'selected' : [ original[ i ].GetHash().hex() for i in indices ],
            'focus' : picked[ -1 ].GetHash().hex(),
            'offered' : offered[ 0 ] if offered else None,
            'moves' : {},
        }

        for name in MOVES:

            qt( select )

            command = CAC.ApplicationCommand.STATICCreateSimpleCommand( CAC.SIMPLE_REARRANGE_THUMBNAILS, ( CAC.REARRANGE_THUMBNAILS_TYPE_COMMAND, commands[ name ] ) )

            qt( lambda: panel.ProcessApplicationCommand( command ) )

            time.sleep( 0.1 )

            record_case[ 'moves' ][ name ] = { 'order' : qt( order ), 'selected' : qt( selected ) }

            # put it back
            qt( lambda: panel.MoveMedia( list( original ), insertion_index = 0 ) )

            time.sleep( 0.1 )

            assert qt( order ) == initial, 'the order was not put back'


        cases.append( record_case )


    return { 'initial' : initial, 'cases' : cases }


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

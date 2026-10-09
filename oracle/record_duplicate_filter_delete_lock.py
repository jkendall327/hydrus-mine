#!/usr/bin/env python3
"""The archived-file delete lock in the reference's duplicate filter.

On the `auto_resolution` database, each phase sets "delete lock for archived
files" and "After duplicate filter, ensure deletees are inboxed before delete"
(on or off), archives every file, then drives the real filter canvas with
`record_duplicate_filter_canvas.record`: "this is better, delete the other" on
the first pair, then closing and committing. Afterwards each batch file's
inbox state and local file domains are read. Each phase runs on its own copy
of the database. Writes `fixtures/duplicate_filter_delete_lock.json`.

Usage: QT_QPA_PLATFORM=offscreen TZ=UTC ~/pyenv/bin/python oracle/record_duplicate_filter_delete_lock.py
"""

import json
import os
import sys
import tempfile

HERE = os.path.dirname( os.path.abspath( __file__ ) )

sys.path.insert( 0, HERE )

import record_duplicate_filter_canvas as canvas

OUT = os.path.join( HERE, 'fixtures', 'duplicate_filter_delete_lock.json' )

# ( delete lock, reinbox deletees after the duplicate filter )
PHASES = [ ( True, False ), ( True, True ), ( False, True ), ( False, False ) ]

canvas.SCENARIOS.append( ( 'delete_lock', canvas.AUTO_COMMIT_OFF, [
    'better_delete_other',
    { 'do' : 'close', 'answers' : [ 'commit' ] },
] ) )


def record( session ):

    from hydrus.core import HydrusConstants as HC
    from hydrus.client import ClientConstants as CC
    from hydrus.client.metadata import ClientContentUpdates

    c = session.controller

    ( lock, reinbox ) = PHASES[ int( os.environ[ 'PHASE' ] ) ]

    c.new_options.SetBoolean( 'delete_lock_for_archived_files', lock )
    c.new_options.SetBoolean( 'delete_lock_reinbox_deletees_after_duplicate_filter', reinbox )

    import sqlite3

    master = sqlite3.connect( 'file:' + os.path.join( c.db_dir, 'client.master.db' ) + '?mode=ro', uri = True )
    hashes = [ bytes( h ) for ( h, ) in master.execute( 'SELECT hash FROM hashes;' ) ]
    master.close()

    package = ClientContentUpdates.ContentUpdatePackage.STATICCreateFromContentUpdate( CC.HYDRUS_LOCAL_FILE_STORAGE_SERVICE_KEY, ClientContentUpdates.ContentUpdate( HC.CONTENT_TYPE_FILES, HC.CONTENT_UPDATE_ARCHIVE, hashes ) )

    c.WriteSynchronous( 'content_updates', package )

    os.environ[ 'SCENARIO' ] = str( len( canvas.SCENARIOS ) - 1 )

    out = canvas.record( session )

    scenario = out[ 'scenarios' ][ 0 ]

    batch = sorted( { h for pair in scenario[ 'batch' ] for h in pair } )

    media_results = c.Read( 'media_results', [ bytes.fromhex( h ) for h in batch ] )

    states = {}

    for media_result in media_results:

        locations = media_result.GetLocationsManager()

        states[ media_result.GetHash().hex() ] = {
            'inbox' : locations.inbox,
            'domains' : sorted( c.services_manager.GetName( key ) for key in locations.GetCurrent() if c.services_manager.GetServiceType( key ) in HC.FILE_SERVICES_WITH_SPECIFIC_MAPPING_CACHES or key in ( CC.TRASH_SERVICE_KEY, CC.COMBINED_LOCAL_MEDIA_SERVICE_KEY ) or c.services_manager.GetServiceType( key ) == HC.LOCAL_FILE_DOMAIN ),
        }

    return {
        'delete_lock' : lock,
        'reinbox_after_duplicate_filter' : reinbox,
        'steps' : scenario[ 'steps' ],
        'batch' : scenario[ 'batch' ],
        'files' : states,
    }


def child( out ):

    import hydrus_driver
    import record_api

    db_dir = record_api.unpack_fixture( 'auto_resolution' )

    result = hydrus_driver.run_client( db_dir, record )

    with open( out, 'w' ) as f:

        json.dump( result, f, ensure_ascii = False )


def main():

    if len( sys.argv ) > 1 and sys.argv[ 1 ] == '--child':

        child( sys.argv[ 2 ] )

        return

    import hydrus_driver

    phases = []

    for i in range( len( PHASES ) ):

        with tempfile.TemporaryDirectory() as work:

            path = os.path.join( work, 'phase.json' )

            os.environ[ 'PHASE' ] = str( i )

            hydrus_driver.run_in_subprocess( os.path.abspath( __file__ ), '--child', path )

            with open( path ) as f:

                phases.append( json.load( f ) )

    with open( OUT, 'w' ) as f:

        json.dump( { 'phases' : phases }, f, indent = 1, ensure_ascii = False )
        f.write( '\n' )

    print( f'wrote {OUT}' )


if __name__ == '__main__':

    main()

#!/usr/bin/env python3
"""The archived-file delete lock in duplicates auto-resolution.

On the `auto_resolution` database, each phase sets "delete lock for archived
files" and "In duplicates auto-resolution, ensure deletees are inboxed before
delete" (on or off) and archives every file. Then the pair that
`auto_resolution_review.json` approves on "pixel-perfect gifs vs pngs" is
approved as the review window's "approve" does
(`ClientDuplicatesAutoResolution.ActionAutoResolutionReviewPairs` with an
approving `DuplicatePairDecisionApproveDeny`), and the pair's files' inbox
state and local file domains are read. Each phase runs on its own copy of
the database. Writes `fixtures/auto_resolution_delete_lock.json`.

Usage: QT_QPA_PLATFORM=offscreen TZ=UTC ~/pyenv/bin/python oracle/record_auto_resolution_delete_lock.py
"""

import json
import os
import sys
import tempfile

HERE = os.path.dirname( os.path.abspath( __file__ ) )

sys.path.insert( 0, HERE )

OUT = os.path.join( HERE, 'fixtures', 'auto_resolution_delete_lock.json' )

RULE = 'pixel-perfect gifs vs pngs'

# ( delete lock, reinbox deletees in auto-resolution )
PHASES = [ ( True, False ), ( True, True ), ( False, True ), ( False, False ) ]


def approved_pair():

    review = json.load( open( os.path.join( HERE, 'fixtures', 'auto_resolution_review.json' ) ) )

    rows = review[ 'steps' ][ 0 ][ 'state' ][ 'actioned' ][ 'rows' ]

    return ( rows[ 0 ][ 0 ], rows[ 0 ][ 1 ] )


def record( session ):

    from hydrus.core import HydrusConstants as HC
    from hydrus.client import ClientConstants as CC
    from hydrus.client.duplicates import ClientDuplicatesAutoResolution
    from hydrus.client.duplicates import ClientPotentialDuplicatesPairFactory
    from hydrus.client.metadata import ClientContentUpdates

    c = session.controller

    manager = c.duplicates_auto_resolution_manager
    manager._AbleToWorkIdleNormal = lambda: False

    ( lock, reinbox ) = PHASES[ int( os.environ[ 'PHASE' ] ) ]

    c.new_options.SetBoolean( 'delete_lock_for_archived_files', lock )
    c.new_options.SetBoolean( 'delete_lock_reinbox_deletees_in_auto_resolution', reinbox )

    import sqlite3

    master = sqlite3.connect( 'file:' + os.path.join( c.db_dir, 'client.master.db' ) + '?mode=ro', uri = True )
    hashes = [ bytes( h ) for ( h, ) in master.execute( 'SELECT hash FROM hashes;' ) ]
    master.close()

    package = ClientContentUpdates.ContentUpdatePackage.STATICCreateFromContentUpdate( CC.HYDRUS_LOCAL_FILE_STORAGE_SERVICE_KEY, ClientContentUpdates.ContentUpdate( HC.CONTENT_TYPE_FILES, HC.CONTENT_UPDATE_ARCHIVE, hashes ) )

    c.WriteSynchronous( 'content_updates', package )

    [ rule ] = [ r for r in c.Read( 'duplicates_auto_resolution_rules_with_counts' ) if r.GetName() == RULE ]

    wanted = approved_pair()

    pending = c.Read( 'duplicates_auto_resolution_pending_action_pairs', rule )

    [ ( a, b ) ] = [ ( a, b ) for ( a, b ) in pending if ( a.GetHash().hex(), b.GetHash().hex() ) == wanted ]

    decision = ClientPotentialDuplicatesPairFactory.DuplicatePairDecisionApproveDeny( a, b, True )

    ClientDuplicatesAutoResolution.ActionAutoResolutionReviewPairs( rule, [ decision ] )

    states = {}

    for media_result in c.Read( 'media_results', [ a.GetHash(), b.GetHash() ] ):

        locations = media_result.GetLocationsManager()

        current = locations.GetCurrent()

        states[ media_result.GetHash().hex() ] = {
            'inbox' : locations.inbox,
            'my files' : CC.LOCAL_FILE_SERVICE_KEY in current,
            'trash' : CC.TRASH_SERVICE_KEY in current,
        }

    return {
        'delete_lock' : lock,
        'reinbox_in_auto_resolution' : reinbox,
        'rule' : RULE,
        'pair' : list( wanted ),
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

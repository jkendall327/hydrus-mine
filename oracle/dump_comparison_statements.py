#!/usr/bin/env python3
"""Record the duplicate filter's comparison statements.

The duplicate filter shows, for the file on screen against the other, a
statement per property ("1.2MB > 800KB (+50%)", "both are jpegs", "this has
exif data, the other does not"...) with a score, and orders each pair by the
total of the fast statements' scores (`ABPairsUsingFastComparisonScore`).
This records `GetDuplicateComparisonStatementsFast` (with its score and
whether the files are pixel duplicates) and
`GetDuplicateComparisonStatementsSlow` (jpeg quality, visual duplicates) for
every ordered pair of files in two sets:

- `basic`: the files of the `basic` fixture database (tags, audio, video,
  archives and documents...);
- `families`: the images of oracle/fixtures/auto_resolution/ (families of
  re-encodes, bloated copies, exif and icc variants), imported into a new
  client with import times spread over months by the Client API, compared
  within each family.

"Now" is pinned while the statements are made (ages are relative to it), and
recorded, with each file's import time.

Usage: QT_QPA_PLATFORM=offscreen python oracle/dump_comparison_statements.py
"""

import json
import os
import shutil
import sys
import tarfile
import tempfile

HERE = os.path.dirname( os.path.abspath( __file__ ) )

sys.path.insert( 0, HERE )

import hydrus_driver

OUT = os.path.join( HERE, 'fixtures', 'comparison_statements.json' )
FAMILY_DIR = os.path.join( HERE, 'fixtures', 'auto_resolution' )
PORT = 45961

DAY = 86400


def statements( media_results, pairs, now ):

    from hydrus.core import HydrusTime
    from hydrus.client import ClientConstants as CC
    from hydrus.client.duplicates import ClientDuplicatesComparisonStatements as S

    original_get_now = HydrusTime.GetNow

    HydrusTime.GetNow = lambda: now

    try:

        rows = []

        for ( s, c ) in pairs:

            ( fast, pixel_duplicates ) = S.GetDuplicateComparisonStatementsFast( s, c )

            slow = S.GetDuplicateComparisonStatementsSlow( s, c, pixel_duplicates )

            rows.append( {
                'shown' : s.GetHash().hex(),
                'other' : c.GetHash().hex(),
                'fast' : [ [ k, statement, score ] for ( k, ( statement, score ) ) in fast.items() ],
                'pixel_duplicates' : pixel_duplicates,
                'score' : S.GetDuplicateComparisonScoreFast( s, c ),
                'slow' : [ [ k, statement, score ] for ( k, ( statement, score ) ) in slow.items() ],
            } )


    finally:

        HydrusTime.GetNow = original_get_now


    imported = {}

    for m in media_results:

        imported[ m.GetHash().hex() ] = m.GetLocationsManager().GetTimesManager().GetImportedTimestampMS( CC.HYDRUS_LOCAL_FILE_STORAGE_SERVICE_KEY )


    return { 'now' : now, 'imported_ms' : imported, 'pairs' : rows }


def scores( session ):

    options = session.controller.new_options

    return { name : options.GetInteger( name ) for name in sorted( options._dictionary[ 'integers' ] ) if name.startswith( 'duplicate_comparison_score_' ) }


def record_basic( session, manifest ):

    hashes = [ bytes.fromhex( f[ 'hash' ] ) for f in manifest[ 'files' ] ]

    media_results = session.read( 'media_results', hashes )

    from hydrus.client import ClientConstants as CC

    newest_s = max( ( m.GetLocationsManager().GetTimesManager().GetImportedTimestampMS( CC.HYDRUS_LOCAL_FILE_STORAGE_SERVICE_KEY ) or 0 ) for m in media_results ) // 1000

    now = newest_s + 40 * DAY + 1234

    pairs = [ ( s, c ) for s in media_results for c in media_results if s is not c ]

    result = statements( media_results, pairs, now )

    result[ 'scores' ] = scores( session )

    return result


def record_families( session ):

    names = sorted( os.listdir( FAMILY_DIR ) )

    hashes = [ session.api.post( '/add_files/add_file', { 'path' : os.path.join( FAMILY_DIR, name ) } )[ 'hash' ] for name in names ]

    # spread the import times: some pairs a little apart, some months
    base = 1_700_000_000

    for ( i, h ) in enumerate( hashes ):

        timestamp = base + ( i * 11 % 17 ) * 9 * DAY + i * 3600

        session.write( 'content_updates', import_time_update( h, timestamp * 1000 ) )


    media_results = session.read( 'media_results', [ bytes.fromhex( h ) for h in hashes ] )

    by_hash = { m.GetHash().hex() : m for m in media_results }

    families = {}

    for ( name, h ) in zip( names, hashes ):

        families.setdefault( name.split( '_' )[ 0 ], [] ).append( by_hash[ h ] )


    pairs = [ ( s, c ) for members in families.values() for s in members for c in members if s is not c ]

    now = base + 200 * DAY

    result = statements( media_results, pairs, now )

    result[ 'files' ] = [ [ name, h ] for ( name, h ) in zip( names, hashes ) ]

    return result


def import_time_update( hash_hex, timestamp_ms ):
    """Set a file's import time in hydrus local file storage (as
    /edit_times/set_time does for a file domain)."""

    from hydrus.core import HydrusConstants as HC
    from hydrus.client import ClientConstants as CC
    from hydrus.client import ClientTime
    from hydrus.client.metadata import ClientContentUpdates

    timestamp_data = ClientTime.TimestampData( timestamp_type = HC.TIMESTAMP_TYPE_IMPORTED, location = CC.HYDRUS_LOCAL_FILE_STORAGE_SERVICE_KEY, timestamp_ms = timestamp_ms )

    content_update = ClientContentUpdates.ContentUpdate( HC.CONTENT_TYPE_TIMESTAMP, HC.CONTENT_UPDATE_SET, ( ( bytes.fromhex( hash_hex ), ), timestamp_data ) )

    return ClientContentUpdates.ContentUpdatePackage.STATICCreateFromContentUpdates( CC.HYDRUS_LOCAL_FILE_STORAGE_SERVICE_KEY, [ content_update ] )


def main():

    if len( sys.argv ) > 1:

        ( step, out ) = sys.argv[ 1 : 3 ]

        work = tempfile.mkdtemp( prefix = 'hydrus_statements_' )

        try:

            db_dir = os.path.join( work, 'db' )

            if step == 'basic':

                manifest = json.load( open( os.path.join( HERE, 'fixtures', 'legacy_db', 'basic.manifest.json' ) ) )

                with tarfile.open( os.path.join( HERE, 'fixtures', 'legacy_db', 'basic.tar.gz' ) ) as tar:

                    tar.extractall( db_dir, filter = 'data' )


                result = hydrus_driver.run_client( db_dir, lambda s: record_basic( s, manifest ) )

            else:

                result = hydrus_driver.run_client( db_dir, record_families, port = PORT )


        finally:

            shutil.rmtree( work, ignore_errors = True )


        with open( out, 'w' ) as f:

            json.dump( result, f )


        return


    results = {}

    work = tempfile.mkdtemp( prefix = 'hydrus_statements_out_' )

    try:

        for step in ( 'basic', 'families' ):

            out = os.path.join( work, f'{step}.json' )

            hydrus_driver.run_in_subprocess( os.path.abspath( __file__ ), step, out )

            results[ step ] = json.load( open( out ) )


    finally:

        shutil.rmtree( work, ignore_errors = True )


    scores = results[ 'basic' ].pop( 'scores' )

    with open( OUT, 'w' ) as f:

        json.dump( { 'scores' : scores, **results }, f, indent = 1, sort_keys = True, ensure_ascii = False )
        f.write( '\n' )


    print( f'wrote {OUT}: {sum( len( r[ "pairs" ] ) for r in results.values() )} pairs' )


if __name__ == '__main__':

    main()

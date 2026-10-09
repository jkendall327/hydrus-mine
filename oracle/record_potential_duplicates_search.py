#!/usr/bin/env python3
"""Record how many potential pairs the reference's search finds for each
setting of the potential duplicates search context.

On the database `record_auto_resolution.py` left (files with potential pairs
between them), for every dupe search type (at least one file matches, both
files match, the two files match different searches), pixel dupe preference
(required, allowed, excluded), maximum distance (0, 4, 8) and two sets of
file searches (nothing but `system:everything`; a jpeg search, with a png one
as the second search), this counts the pairs the reference finds with its
own fragmentary count over the search space (what the context panel shows),
and the size of the space.

Usage: QT_QPA_PLATFORM=offscreen TZ=UTC python oracle/record_potential_duplicates_search.py
       (writes fixtures/potential_duplicates_search.json)
"""

import json
import os
import sys

HERE = os.path.dirname( os.path.abspath( __file__ ) )

sys.path.insert( 0, HERE )

OUT = os.path.join( HERE, 'fixtures', 'potential_duplicates_search.json' )

KINDS = [ 0, 1, 2 ]
PIXELS = [ 0, 1, 2 ]
DISTANCES = [ 0, 4, 8 ]
SEARCHES = [ ( [], [] ), ( [ 'system:filetype is jpeg' ], [ 'system:filetype is png' ] ) ]


def record( session ):

    controller = session.controller

    from hydrus.client.duplicates import ClientDuplicates
    from hydrus.client.duplicates import ClientPotentialDuplicatesSearchContext as PDSC
    from hydrus.client.search import ClientSearchFileSearchContext
    from hydrus.client.search import ClientSearchParseSystemPredicates

    kinds = [ ClientDuplicates.DUPE_SEARCH_ONE_FILE_MATCHES_ONE_SEARCH, ClientDuplicates.DUPE_SEARCH_BOTH_FILES_MATCH_ONE_SEARCH, ClientDuplicates.DUPE_SEARCH_BOTH_FILES_MATCH_DIFFERENT_SEARCHES ]
    pixels = [ ClientDuplicates.SIMILAR_FILES_PIXEL_DUPES_REQUIRED, ClientDuplicates.SIMILAR_FILES_PIXEL_DUPES_ALLOWED, ClientDuplicates.SIMILAR_FILES_PIXEL_DUPES_EXCLUDED ]

    results = []

    for ( kind_index, kind ) in enumerate( kinds ):

        for ( pixel_index, pixel ) in enumerate( pixels ):

            for distance in DISTANCES:

                for ( one, two ) in SEARCHES:

                    context = PDSC.PotentialDuplicatesSearchContext()

                    for ( setter, getter, strings ) in ( ( context.SetFileSearchContext1, context.GetFileSearchContext1, one ), ( context.SetFileSearchContext2, context.GetFileSearchContext2, two ) ):

                        file_search_context = getter()

                        if len( strings ) > 0:

                            predicates = ClientSearchParseSystemPredicates.ParseSystemPredicateStringsToPredicates( strings )

                            file_search_context = ClientSearchFileSearchContext.FileSearchContext( location_context = file_search_context.GetLocationContext(), predicates = predicates )

                        setter( file_search_context )

                    context.SetDupeSearchType( kind )
                    context.SetPixelDupesPreference( pixel )
                    context.SetMaxHammingDistance( distance )

                    search = PDSC.PotentialDuplicatePairsFragmentarySearch( context, True )

                    space = controller.Read( 'potential_duplicate_id_pairs_and_distances', context.GetLocationContext() )

                    search.SetSearchSpace( space )

                    count = 0

                    while not search.SearchDone():

                        count += controller.Read( 'potential_duplicates_count_fragmentary', search )

                    results.append( { 'kind' : kind_index, 'pixel' : pixel_index, 'distance' : distance, 'one' : one, 'two' : two, 'space' : search.NumPairsInSearchSpace(), 'matches' : count } )

    return { 'results' : results }


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


    import tempfile

    import hydrus_driver

    with tempfile.TemporaryDirectory() as work:

        path = os.path.join( work, 'potential_duplicates_search.json' )

        hydrus_driver.run_in_subprocess( os.path.abspath( __file__ ), '--child', path )

        with open( path ) as f:

            result = json.load( f )


    with open( OUT, 'w' ) as f:

        json.dump( result, f, indent = 1, ensure_ascii = False )
        f.write( '\n' )


    print( f'wrote {OUT}: {len( result[ "results" ] )} searches' )


if __name__ == '__main__':

    main()

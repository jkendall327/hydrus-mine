#!/usr/bin/env python3
"""Record how the reference's pages collect their files.

In the running client, the `basic` fixture's files current in "my files"
are put in a `MediaList` (as a page holds them), collected
(`MediaList.Collect`) by namespaces and rating services, with unmatched
files collected together or left single, and then sorted as a page sorts
them (`MediaList.Sort`, with the options' fallback sort), by every system
sort but random, the options' namespace sorts and each rating service,
both ways. Each case records the page's media in order: a file's hash,
or a collection's files' hashes in its own order. (A collection's files
are collected in the page's order, where the reference's is arbitrary.)

Usage: QT_QPA_PLATFORM=offscreen python oracle/record_media_collect.py
       (writes fixtures/media_collect.json)
"""

import json
import os
import sys

HERE = os.path.dirname( os.path.abspath( __file__ ) )

sys.path.insert( 0, HERE )

OUT = os.path.join( HERE, 'fixtures', 'media_collect.json' )
MANIFEST = json.load( open( os.path.join( HERE, 'fixtures', 'legacy_db', 'basic.manifest.json' ) ) )


def sort_facts( media_sort ):

    ( metatype, data ) = media_sort.sort_type

    if metatype == 'namespaces':

        ( namespaces, tag_display_type ) = data

        data = { 'namespaces' : list( namespaces ), 'tag_display_type' : tag_display_type }

    elif metatype == 'rating':

        data = data.hex()


    return { 'type' : metatype, 'data' : data, 'order' : media_sort.sort_order }


def media_facts( media ):

    if media.IsCollection():

        return [ m.GetHash().hex() for m in media._sorted_media ]

    else:

        return media.GetHash().hex()



def record( session ):

    from hydrus.core import HydrusConstants as HC
    from hydrus.client import ClientConstants as CC
    from hydrus.client import ClientLocation
    from hydrus.client.media import ClientMediaCollect
    from hydrus.client.media import ClientMediaList
    from hydrus.client.media import ClientMediaSort

    controller = session.controller
    new_options = controller.new_options

    hashes = [ bytes.fromhex( f[ 'hash' ] ) for f in MANIFEST[ 'files' ] ]

    media_results = controller.Read( 'media_results', hashes )

    fallback = new_options.GetFallbackSort()

    sorts = [ ( 'system', sort_type ) for sort_type in sorted( CC.SYSTEM_SORT_TYPES ) if sort_type != CC.SORT_FILES_BY_RANDOM ]

    sorts.extend( namespace_sort.sort_type for namespace_sort in new_options.GetDefaultNamespaceSorts() )

    services = controller.services_manager

    rating_service_keys = sorted( services.GetServiceKeys( HC.RATINGS_SERVICES ) )

    sorts.extend( ( 'rating', service_key ) for service_key in rating_service_keys )

    by_name = { services.GetName( key ) : key for key in rating_service_keys }

    collects = [
        ( [ 'series' ], [], True ),
        ( [ 'series' ], [], False ),
        ( [ 'creator' ], [], True ),
        ( [ 'series', 'creator' ], [], False ),
        ( [ 'page' ], [], True ),
        ( [ 'title' ], [], False ),
        ( [], [ 'stars' ], True ),
        ( [], [ 'favourites' ], False ),
        ( [], [ 'stars', 'favourites' ], True ),
        ( [ 'series' ], [ 'stars' ], True ),
    ]

    location_context = ClientLocation.LocationContext.STATICCreateSimple( CC.LOCAL_FILE_SERVICE_KEY )

    in_page = [ m for m in media_results if CC.LOCAL_FILE_SERVICE_KEY in m.GetLocationsManager().GetCurrent() ]

    cases = []

    for ( namespaces, rating_names, collect_unmatched ) in collects:

        media_collect = ClientMediaCollect.MediaCollect( namespaces = namespaces, rating_service_keys = [ by_name[ name ] for name in rating_names ], collect_unmatched = collect_unmatched )

        sorted_cases = []

        for sort_type in sorts:

            for order in ( CC.SORT_ASC, CC.SORT_DESC ):

                media_sort = ClientMediaSort.MediaSort( sort_type, order )

                media_list = ClientMediaList.MediaList( location_context, in_page )

                # the reference takes the files to collect from a set, so the
                # order of a collection's files, which picks its ratings (the
                # first's) and its resolution (the first largest) on ties, is
                # arbitrary; here they keep the page's order
                media_list._singleton_media = dict.fromkeys( media_list._sorted_media )

                media_list.Collect( media_collect )

                media_list.Sort( media_sort, fallback )

                sorted_cases.append( {
                    'sort' : sort_facts( media_sort ),
                    'media' : [ media_facts( m ) for m in media_list._sorted_media ],
                } )



        cases.append( {
            'collect' : {
                'namespaces' : namespaces,
                'ratings' : [ by_name[ name ].hex() for name in rating_names ],
                'collect_unmatched' : collect_unmatched,
            },
            'sorts' : sorted_cases,
        } )


    return {
        'service_key' : CC.LOCAL_FILE_SERVICE_KEY.hex(),
        'files' : [ m.GetHash().hex() for m in in_page ],
        'cases' : cases,
    }


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

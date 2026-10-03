#!/usr/bin/env python3
"""Record how the reference's pages sort their files.

In the running client, the `basic` fixture's files current in "my files",
and in "all local files", are put in a `MediaList` (as a page holds them)
and sorted as a page sorts them (`MediaList.Sort`: the options' fallback
sort first, then the sort chosen, both stable), by every system sort but
random, by the options' namespace sorts and a few more, and by each
rating service, in both orders. Also recorded: the options' default
sort, fallback sort and namespace sorts.

Usage: QT_QPA_PLATFORM=offscreen python oracle/record_media_sort.py
       (writes fixtures/media_sort.json)
"""

import json
import os
import sys

HERE = os.path.dirname( os.path.abspath( __file__ ) )

sys.path.insert( 0, HERE )

OUT = os.path.join( HERE, 'fixtures', 'media_sort.json' )
MANIFEST = json.load( open( os.path.join( HERE, 'fixtures', 'legacy_db', 'basic.manifest.json' ) ) )


def sort_facts( media_sort ):

    ( metatype, data ) = media_sort.sort_type

    if metatype == 'system':

        data = data

    elif metatype == 'namespaces':

        ( namespaces, tag_display_type ) = data

        data = { 'namespaces' : list( namespaces ), 'tag_display_type' : tag_display_type }

    elif metatype == 'rating':

        data = data.hex()


    return { 'type' : metatype, 'data' : data, 'order' : media_sort.sort_order }


def record( session ):

    from hydrus.core import HydrusConstants as HC
    from hydrus.client import ClientConstants as CC
    from hydrus.client import ClientLocation
    from hydrus.client.media import ClientMediaList
    from hydrus.client.media import ClientMediaSort
    from hydrus.client.metadata import ClientTags

    controller = session.controller
    new_options = controller.new_options

    hashes = [ bytes.fromhex( f[ 'hash' ] ) for f in MANIFEST[ 'files' ] ]

    media_results = controller.Read( 'media_results', hashes )

    fallback = new_options.GetFallbackSort()
    namespace_sorts = new_options.GetDefaultNamespaceSorts()

    sorts = []

    for sort_type in sorted( CC.SYSTEM_SORT_TYPES ):

        if sort_type == CC.SORT_FILES_BY_RANDOM:

            continue


        sorts.append( ( 'system', sort_type ) )


    for namespace_sort in namespace_sorts:

        sorts.append( namespace_sort.sort_type )


    # and some of our own: one namespace, and as a single file shows its tags
    sorts.append( ( 'namespaces', ( ( 'title', ), ClientTags.TAG_DISPLAY_DISPLAY_ACTUAL ) ) )
    sorts.append( ( 'namespaces', ( ( 'page', 'creator' ), ClientTags.TAG_DISPLAY_SINGLE_MEDIA ) ) )

    rating_service_keys = controller.services_manager.GetServiceKeys( HC.RATINGS_SERVICES )

    for service_key in sorted( rating_service_keys ):

        sorts.append( ( 'rating', service_key ) )


    pages = []

    for ( name, service_key ) in (
        ( 'my files', CC.LOCAL_FILE_SERVICE_KEY ),
        ( 'all local files', CC.COMBINED_LOCAL_FILE_DOMAINS_SERVICE_KEY ),
    ):

        location_context = ClientLocation.LocationContext.STATICCreateSimple( service_key )

        in_page = [ m for m in media_results if location_context.IsOneDomain() and service_key in m.GetLocationsManager().GetCurrent() ]

        cases = []

        for sort_type in sorts:

            for order in ( CC.SORT_ASC, CC.SORT_DESC ):

                media_sort = ClientMediaSort.MediaSort( sort_type, order )

                media_list = ClientMediaList.MediaList( location_context, in_page )

                media_list.Sort( media_sort, fallback )

                cases.append( {
                    'sort' : sort_facts( media_sort ),
                    'files' : [ m.GetHash().hex() for m in media_list._sorted_media ],
                } )



        pages.append( { 'page' : name, 'service_key' : service_key.hex(), 'files' : [ m.GetHash().hex() for m in in_page ], 'sorts' : cases } )


    return {
        'default_sort' : sort_facts( new_options.GetDefaultSort() ),
        'fallback_sort' : sort_facts( fallback ),
        'namespace_sorts' : [ sort_facts( s ) for s in namespace_sorts ],
        'pages' : pages,
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

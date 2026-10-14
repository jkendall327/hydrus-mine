#!/usr/bin/env python3
"""Record what loading a favourite search does to a search page in the reference.

On the `basic` fixture, a search page is made and favourite searches (a search
with its sort and collect, one with neither, one that is paused, one that is
paused with neither) are loaded into it in turn through its autocomplete's
`ActivateFavouriteSearch` (what the star menu's entries call). After each
the page has time to search; recorded are the search's predicates, whether the
page searches as it changes, its sort and collect, and how many files it holds.

Usage: QT_QPA_PLATFORM=offscreen python oracle/record_favourite_load.py
       (writes fixtures/favourite_load.json)
"""

import json
import os
import sys
import time

HERE = os.path.dirname( os.path.abspath( __file__ ) )

sys.path.insert( 0, HERE )

OUT = os.path.join( HERE, 'fixtures', 'favourite_load.json' )

# ( name, tags the search has (after system:inbox), searches as it changes,
#   sort ( 'system', CC sort type, ascending ) or None, collect ( namespaces ) or None )
ROWS = [
    ( 'by size', [ 'blue eyes' ], True, ( 'SORT_FILES_BY_FILESIZE', False ), [ 'creator' ] ),
    ( 'keeps', [ 'series:metroid' ], True, None, None ),
    ( 'paused', [ 'blue eyes' ], False, ( 'SORT_FILES_BY_IMPORT_TIME', True ), [] ),
    ( 'paused keeps', [], False, None, None ),
    ( 'by width', [], True, ( 'SORT_FILES_BY_WIDTH', True ), None ),
]

LOADS = [ 'by size', 'keeps', 'paused', 'paused keeps', 'by width', 'keeps', 'paused keeps' ]


def record( session ):

    from hydrus.client import ClientConstants as CC
    from hydrus.client import ClientLocation
    from hydrus.client.media import ClientMediaCollect
    from hydrus.client.media import ClientMediaSort
    from hydrus.client.search import ClientSearchFileSearchContext
    from hydrus.client.search import ClientSearchPredicate

    controller = session.controller
    gui = controller.gui
    manager = controller.favourite_search_manager

    def qt( f ):

        return controller.CallBlockingToQt( gui, f )


    location = ClientLocation.LocationContext.STATICCreateSimple( CC.LOCAL_FILE_SERVICE_KEY )

    def search( tags ):

        predicates = [ ClientSearchPredicate.Predicate( ClientSearchPredicate.PREDICATE_TYPE_SYSTEM_INBOX ) ]

        predicates.extend( ClientSearchPredicate.Predicate( ClientSearchPredicate.PREDICATE_TYPE_TAG, t ) for t in tags )

        return ClientSearchFileSearchContext.FileSearchContext( location_context = location, predicates = predicates )


    rows = []

    for ( name, tags, sync, sort, collect ) in ROWS:

        media_sort = None if sort is None else ClientMediaSort.MediaSort( sort_type = ( 'system', getattr( CC, sort[ 0 ] ) ), sort_order = CC.SORT_ASC if sort[ 1 ] else CC.SORT_DESC )
        media_collect = None if collect is None else ClientMediaCollect.MediaCollect( namespaces = collect )

        rows.append( ( None, name, search( tags ), sync, media_sort, media_collect ) )


    manager.SetFavouriteSearchRows( rows )

    page = qt( lambda: gui._notebook.NewPageQuery( location, initial_predicates = [] ) )

    ac = None

    for _ in range( 600 ):

        sidebar = qt( lambda: page.GetSidebar() )

        if sidebar is not None and hasattr( sidebar, '_tag_autocomplete' ):

            ac = sidebar._tag_autocomplete

            break


        time.sleep( 0.05 )


    def state():

        sort = sidebar._media_sort_widget.GetSort()
        collect = sidebar._media_collect_widget.GetValue()

        return {
            'predicates' : sorted( p.ToString( with_count = False ) for p in ac.GetFileSearchContext().GetPredicates() ),
            'synchronised' : ac.IsSynchronised(),
            'sort' : sort.ToString(),
            'collect' : collect.ToString(),
            'collect_namespaces' : list( collect.namespaces ),
            'files' : len( page.GetHashes() )
        }


    out = { 'rows' : [ [ name, tags, sync, sort, collect ] for ( name, tags, sync, sort, collect ) in ROWS ], 'initial' : qt( state ), 'loads' : [] }

    for name in LOADS:

        qt( lambda: ac.ActivateFavouriteSearch( ( None, name ) ) )

        time.sleep( 1.5 )

        out[ 'loads' ].append( { 'name' : name, 'state' : qt( state ) } )


    return out


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

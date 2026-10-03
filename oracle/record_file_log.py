#!/usr/bin/env python3
"""Record the reference's file log window (`EditFileSeedCachePanel`).

An importer's file log lists its files to import: "#", source (a URL as
the reference shows it, or a path), status ("successful", "already in
db", "deleted", "error", "ignored", "skipped", or blank for unknown),
added, last modified, source time ("unknown" without one) and note (its
first line). The list's right-click menu (`_GetListCtrlMenu`) offers, on
selected rows, opening their files, copying their sources and notes,
opening them, searching for the URLs, trying again, skipping and deleting
them, and a "whole log" submenu; with nothing selected, and from the file
log button's arrow, the whole log's menu (`PopulateFileSeedCacheMenu`):
retrying, deleting files of each status, and so on.

With the time held at `NOW`, this makes file logs of `SEEDS` (each file's
type, data, status, when added, modified and posted in seconds before
`NOW`, note, and whether it has a hash), and records:

* `rows`: each file's row in a log of all of them, and the header text;
* `menus`: the whole log's menu for logs of each of `LOGS` (counts by
  status, and whether URLs or paths);
* `row_menus`: the right-click menu with rows selected (`SELECTIONS`).

Statuses are `CC.STATUS_*`: 0 unknown, 1 successful and new, 2 already in
db, 3 deleted, 4 failed, 7 ignored, 8 skipped.

Usage: QT_QPA_PLATFORM=offscreen python oracle/record_file_log.py
       (writes fixtures/file_log.json)
"""

import hashlib
import json
import os
import sys

HERE = os.path.dirname( os.path.abspath( __file__ ) )

sys.path.insert( 0, HERE )

from record_thumbnail_menu import tree

OUT = os.path.join( HERE, 'fixtures', 'file_log.json' )

NOW = 1_700_000_000
HOUR = 3600
DAY = 86400

# ( url?, data, status, added ago, modified ago, posted ago or None, note,
# has a hash )
SEEDS = [
    ( True, 'https://example.com/post/1', 1, DAY, HOUR, 2 * DAY, '', True ),
    ( True, 'https://example.com/post/%E6%97%A5%E6%9C%AC 2', 2, 2 * DAY, DAY, None, 'Found 1 new file.', True ),
    ( True, 'https://example.com/post/3', 3, 3 * HOUR, 30, 5 * DAY, 'Previously deleted\nmore detail', True ),
    ( True, 'https://example.com/post/4', 4, 400 * DAY, 10 * DAY, None, 'Error: 404\ntraceback', False ),
    ( True, 'https://example.com/post/5', 7, 5, 5, 3 * 365 * DAY, 'veto: blacklisted!', False ),
    ( True, 'https://example.com/post/6', 8, HOUR, HOUR, None, '', False ),
    ( True, 'https://example.com/post/7', 0, 90, 90, None, '', False ),
]

# ( counts by status, urls )
LOGS = [
    ( {}, True ),
    ( { 0 : 3 }, True ),
    ( { 1 : 2, 2 : 1 }, True ),
    ( { 4 : 1, 7 : 2, 3 : 1, 8 : 1 }, False ),
    ( { 0 : 1, 1 : 1, 2 : 1, 3 : 1, 4 : 1, 7 : 1, 8 : 1 }, True ),
]

# rows selected, by index in `SEEDS`
SELECTIONS = [
    [ 0 ],
    [ 3, 4 ],
]


def record( session ):

    controller = session.controller
    gui = controller.gui

    def f():

        from hydrus.core import HydrusTime

        from hydrus.client.gui.importing import ClientGUIFileSeedCache
        from hydrus.client.gui import ClientGUIMenus
        from hydrus.client.importing import ClientImportFileSeeds

        get_now = HydrusTime.GetNow

        HydrusTime.GetNow = lambda: NOW

        def make( seeds ):

            cache = ClientImportFileSeeds.FileSeedCache()

            made = []

            for ( url, data, status, added, modified, posted, note, has_hash ) in seeds:

                seed_type = ClientImportFileSeeds.FILE_SEED_TYPE_URL if url else ClientImportFileSeeds.FILE_SEED_TYPE_HDD

                file_seed = ClientImportFileSeeds.FileSeed( seed_type, data )

                file_seed.status = status
                file_seed.created = NOW - added
                file_seed.modified = NOW - modified
                file_seed.source_time = None if posted is None else NOW - posted
                file_seed.note = note

                if has_hash:

                    file_seed.SetHash( hashlib.sha256( data.encode() ).digest() )


                made.append( file_seed )


            cache.AddFileSeeds( made )

            return ( cache, made )


        try:

            ( cache, made ) = make( SEEDS )

            panel = ClientGUIFileSeedCache.EditFileSeedCachePanel( gui, cache )

            panel._UpdateText()

            rows = [ list( panel._ConvertFileSeedToDisplayTuple( s ) ) for s in made ]

            header = panel._text.text()

            menus = []

            for ( counts, urls ) in LOGS:

                seeds = []

                for ( status, n ) in counts.items():

                    for i in range( n ):

                        data = f'https://example.com/{status}/{i}' if urls else f'/home/me/{status}/{i}.jpg'

                        seeds.append( ( urls, data, status, HOUR, HOUR, None, '', False ) )



                ( log, _ ) = make( seeds )

                menu = ClientGUIMenus.GenerateMenu( gui )

                ClientGUIFileSeedCache.PopulateFileSeedCacheMenu( gui, menu, log, [] )

                menus.append( { 'counts' : counts, 'urls' : urls, 'menu' : tree( menu ) } )


            row_menus = []

            for selection in SELECTIONS:

                panel._list_ctrl.SelectDatas( [ made[ i ] for i in selection ], deselect_others = True )

                menu = panel._GetListCtrlMenu()

                row_menus.append( { 'selected' : selection, 'menu' : tree( menu ) } )


            # a log of paths, one selected
            path_seeds = [ ( False, '/home/me/a.jpg', 1, HOUR, HOUR, None, '', True ) ]

            ( path_cache, path_made ) = make( path_seeds )

            path_panel = ClientGUIFileSeedCache.EditFileSeedCachePanel( gui, path_cache )

            path_panel._list_ctrl.SelectDatas( path_made, deselect_others = True )

            row_menus.append( { 'selected' : [ 'path' ], 'menu' : tree( path_panel._GetListCtrlMenu() ) } )

        finally:

            HydrusTime.GetNow = get_now


        return { 'now' : NOW, 'seeds' : SEEDS, 'rows' : rows, 'header' : header, 'menus' : menus, 'row_menus' : row_menus }


    return controller.CallBlockingToQt( gui, f )


def child( out ):

    import hydrus_driver
    import record_api

    db_dir = record_api.unpack_fixture( 'basic' )

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

        path = os.path.join( work, 'file_log.json' )

        hydrus_driver.run_in_subprocess( os.path.abspath( __file__ ), '--child', path )

        with open( path ) as f:

            result = json.load( f )



    with open( OUT, 'w' ) as f:

        json.dump( result, f, indent = 1, ensure_ascii = False )
        f.write( '\n' )


    print( f'wrote {OUT}: {len( result[ "rows" ] )} rows' )


if __name__ == '__main__':

    main()

#!/usr/bin/env python3
"""Record the reference's search log window (`EditGallerySeedLogPanel`).

A gallery search's (or watcher's) search log lists the gallery pages it
read: "#", url (as the reference shows URLs), status (blank for unknown),
added, last modified and note (its first line). Its right-click menu
(`_GetListCtrlMenu`) offers, on selected rows, copying their urls and
notes, opening them, the page's details, trying it again (just this page,
or letting the search continue), skipping, and a "whole log" submenu; with
nothing selected, and from the log button's arrow, the whole log's menu
(`PopulateGallerySeedLogButton`): deleting entries of each status (asking
first), restarting a failed search, exporting and importing urls.

With the time held at `NOW`, this makes logs of `SEEDS` (each page's url,
status, when added and modified in seconds before `NOW`, and note) and
records:

* `rows`: each page's row in a log of all of them, and the header text;
* `menus`: the whole log's menu for logs of each of `LOGS` (counts by
  status, whether the last page failed, read only, and whether it can
  generate more pages);
* `row_menus`: the right-click menu with rows selected (`SELECTIONS`), a
  gallery search's (which can generate more pages) and a watcher's;
* `delete_question`: what deleting the failed entries asks.

Statuses are `CC.STATUS_*`: 0 unknown, 1 successful, 4 failed, 7
ignored, 8 skipped.

Usage: QT_QPA_PLATFORM=offscreen python oracle/record_search_log.py
       (writes fixtures/search_log.json)
"""

import json
import os
import sys

HERE = os.path.dirname( os.path.abspath( __file__ ) )

sys.path.insert( 0, HERE )

from record_thumbnail_menu import tree

OUT = os.path.join( HERE, 'fixtures', 'search_log.json' )

NOW = 1_700_000_000
HOUR = 3600
DAY = 86400

# ( url, status, added ago, modified ago, note )
SEEDS = [
    ( 'https://example.com/gallery?tags=blue%20eyes&page=1', 1, DAY, DAY, '3 new urls found\n(and more)' ),
    ( 'https://example.com/gallery?tags=blue%20eyes&page=2', 1, DAY, HOUR, 'no new urls found' ),
    ( 'https://example.com/gallery?tags=blue%20eyes&page=3', 4, HOUR, 30, 'Error: 404' ),
    ( 'https://example.com/gallery?tags=blue%20eyes&page=4', 7, 5, 5, '' ),
    ( 'https://example.com/gallery?tags=blue%20eyes&page=5', 8, 90, 90, '' ),
    ( 'https://example.com/gallery?tags=blue%20eyes&page=6', 0, 2 * DAY, 2 * DAY, '' ),
]

# ( counts by status, last failed, read only, can generate more pages )
LOGS = [
    ( {}, False, False, True ),
    ( { 1 : 2 }, False, False, True ),
    ( { 1 : 2, 4 : 1 }, True, False, True ),
    ( { 1 : 2, 4 : 1 }, True, True, True ),
    ( { 1 : 1, 7 : 2, 8 : 1, 0 : 1 }, False, False, False ),
]

# rows selected, by index in `SEEDS`
SELECTIONS = [
    [ 0 ],
    [ 2 ],
    [ 1, 3 ],
]


def record( session ):

    controller = session.controller
    gui = controller.gui

    def f():

        from qtpy import QtWidgets as QW

        from hydrus.core import HydrusTime

        from hydrus.client.gui import ClientGUIDialogsQuick
        from hydrus.client.gui import ClientGUIMenus
        from hydrus.client.gui.importing import ClientGUIGallerySeedLog
        from hydrus.client.importing import ClientImportGallerySeeds

        get_now = HydrusTime.GetNow

        HydrusTime.GetNow = lambda: NOW

        asked = []

        def get_yes_no( win, message, **kwargs ):

            asked.append( message )

            return QW.QDialog.DialogCode.Rejected


        ClientGUIDialogsQuick.GetYesNo = get_yes_no

        def make( seeds ):

            log = ClientImportGallerySeeds.GallerySeedLog()

            made = []

            for ( url, status, added, modified, note ) in seeds:

                gallery_seed = ClientImportGallerySeeds.GallerySeed( url )

                gallery_seed.status = status
                gallery_seed.created = NOW - added
                gallery_seed.modified = NOW - modified
                gallery_seed.note = note

                made.append( gallery_seed )


            log.AddGallerySeeds( made )

            return ( log, made )


        try:

            ( log, made ) = make( SEEDS )

            panel = ClientGUIGallerySeedLog.EditGallerySeedLogPanel( gui, False, True, 'search', log )

            panel._UpdateText()

            rows = [ list( panel._ConvertGallerySeedToDisplayTuple( s ) ) for s in made ]

            header = panel._text.text()

            menus = []

            for ( counts, last_failed, read_only, can_generate ) in LOGS:

                seeds = []

                for ( status, n ) in counts.items():

                    if last_failed and status == 4:

                        continue


                    for i in range( n ):

                        seeds.append( ( f'https://example.com/{status}/{i}', status, HOUR, HOUR, '' ) )



                if last_failed:

                    seeds.extend( ( f'https://example.com/4/{i}', 4, HOUR, HOUR, '' ) for i in range( counts[ 4 ] ) )


                ( small, _ ) = make( seeds )

                menu = ClientGUIMenus.GenerateMenu( gui )

                ClientGUIGallerySeedLog.PopulateGallerySeedLogButton( gui, menu, small, [], read_only, can_generate, 'search' )

                menus.append( { 'counts' : counts, 'last_failed' : last_failed, 'read_only' : read_only, 'can_generate' : can_generate, 'menu' : tree( menu ) } )


            row_menus = []

            for ( can_generate, kind ) in ( ( True, 'search' ), ( False, 'check' ) ):

                kind_panel = ClientGUIGallerySeedLog.EditGallerySeedLogPanel( gui, False, can_generate, kind, log )

                for selection in SELECTIONS:

                    kind_panel._list_ctrl.SelectDatas( [ made[ i ] for i in selection ], deselect_others = True )

                    row_menus.append( { 'kind' : kind, 'selected' : selection, 'menu' : tree( kind_panel._GetListCtrlMenu() ) } )



            ClientGUIGallerySeedLog.ClearGallerySeeds( gui, log, ( 4, ), 'search' )

        finally:

            HydrusTime.GetNow = get_now


        return { 'now' : NOW, 'seeds' : SEEDS, 'rows' : rows, 'header' : header, 'menus' : menus, 'row_menus' : row_menus, 'delete_question' : asked[0] }


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

        path = os.path.join( work, 'search_log.json' )

        hydrus_driver.run_in_subprocess( os.path.abspath( __file__ ), '--child', path )

        with open( path ) as f:

            result = json.load( f )



    with open( OUT, 'w' ) as f:

        json.dump( result, f, indent = 1, ensure_ascii = False )
        f.write( '\n' )


    print( f'wrote {OUT}: {len( result[ "rows" ] )} rows' )


if __name__ == '__main__':

    main()

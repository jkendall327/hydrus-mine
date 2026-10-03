#!/usr/bin/env python3
"""Record the right-click menus of a gallery and a watcher downloader
page's lists (`_GetListCtrlMenu`), and presentation options' summaries.

A gallery page's list menu, on its selected queries, offers copying their
queries, "show files" (a submenu of presentation options:
`AddPresentationSubmenu`), their file and search logs (with one selected,
submenus with "show file log" or "show search log" and that log's whole
menu; with more, "show file logs" and "show search log"), removing them,
and pausing or playing their files and searches. A watcher page's offers
copying and opening their URLs, copying subjects, "show files", the file
and check logs, retrying failed and ignored files (when any selected
watcher has them), removing them and pausing or playing their files and
checking.

In the running client, on the `basic` fixture, with all new network
traffic paused, this opens a gallery page with `QUERIES` (files and
searches paused at start) and a watcher page with `URLS` (checking
paused), gives the first query and the first watcher the file seeds in
`FILE_SEEDS` and gallery seeds in `GALLERY_SEEDS` (by status), and
records each list's menu with the rows of each of `SELECTIONS` selected
(as `record_thumbnail_menu.tree` writes menus).

`summaries` are `PresentationImportOptions.GetSummary` for `PRESENTATIONS`
(status, inbox, location: "combined" for all my files, "storage" for all
local files and trash), for a gallery downloader (post URLs) and a
watcher.

Statuses are `CC.STATUS_*`: 0 unknown, 1 successful, 4 failed, 7 ignored.

Usage: QT_QPA_PLATFORM=offscreen python oracle/record_importer_menus.py
       (writes fixtures/importer_menus.json)
"""

import json
import os
import sys

HERE = os.path.dirname( os.path.abspath( __file__ ) )

sys.path.insert( 0, HERE )

from record_thumbnail_menu import tree

OUT = os.path.join( HERE, 'fixtures', 'importer_menus.json' )

QUERIES = [ 'blue', 'green', 'red' ]
URLS = [ 'https://boards.example/thread/1', 'https://boards.example/thread/2', 'https://boards.example/thread/3' ]

# file seeds' statuses, and gallery seeds', of the first query and watcher
FILE_SEEDS = [ 1, 4, 7, 0 ]
GALLERY_SEEDS = [ 1, 4 ]

SELECTIONS = [ [ 0 ], [ 1 ], [ 0, 1 ] ]

# ( status, inbox, location ): status 0 all files, 1 new files, 2 none;
# inbox 0 agnostic, 1 require inbox, 2 and include all inbox
PRESENTATIONS = [
    ( 0, 0, 'combined' ),
    ( 1, 0, 'combined' ),
    ( 2, 0, 'combined' ),
    ( 0, 1, 'combined' ),
    ( 1, 1, 'combined' ),
    ( 0, 2, 'combined' ),
    ( 1, 2, 'combined' ),
    ( 0, 0, 'storage' ),
    ( 1, 1, 'storage' ),
]


def record( session ):

    controller = session.controller
    gui = controller.gui

    def f():

        from hydrus.client import ClientConstants as CC
        from hydrus.client import ClientLocation
        from hydrus.client.importing import ClientImportFileSeeds
        from hydrus.client.importing import ClientImportGallerySeeds
        from hydrus.client.importing.options import ImportOptionsConstants as IOC
        from hydrus.client.importing.options import PresentationImportOptions
        from hydrus.client.networking import ClientNetworkingGUG

        def give_seeds( importer, prefix ):

            file_seeds = []

            for ( i, status ) in enumerate( FILE_SEEDS ):

                file_seed = ClientImportFileSeeds.FileSeed( ClientImportFileSeeds.FILE_SEED_TYPE_URL, f'{prefix}/post/{i}' )

                file_seed.status = status

                file_seeds.append( file_seed )


            importer.GetFileSeedCache().AddFileSeeds( file_seeds )

            gallery_seeds = []

            for ( i, status ) in enumerate( GALLERY_SEEDS ):

                gallery_seed = ClientImportGallerySeeds.GallerySeed( f'{prefix}/page/{i}' )

                gallery_seed.status = status

                gallery_seeds.append( gallery_seed )


            importer.GetGallerySeedLog().AddGallerySeeds( gallery_seeds )


        out = {}

        # gallery
        gug = ClientNetworkingGUG.GalleryURLGenerator( 'example tag search', url_template = 'https://booru.example/search/%tags%/1', replacement_phrase = '%tags%', search_terms_separator = '+', initial_search_text = 'tag', example_search_text = 'blue_eyes' )

        controller.network_engine.domain_manager.SetGUGs( [ gug ] )

        page = gui._notebook.NewPageImportGallery()

        sidebar = page.GetSidebar()

        sidebar._multiple_gallery_import.SetStartFileQueuesPaused( True )
        sidebar._multiple_gallery_import.SetStartGalleryQueuesPaused( True )
        sidebar._multiple_gallery_import.SetGUGKeyAndName( ( gug.GetGUGKey(), gug.GetName() ) )

        sidebar._PendQueries( QUERIES )

        importers = list( sidebar._multiple_gallery_import.GetGalleryImports() )
        order = [ next( i for i in importers if i.GetQueryText() == q ) for q in QUERIES ]

        give_seeds( order[ 0 ], 'https://booru.example' )

        menus = []

        for selection in SELECTIONS:

            sidebar._gallery_importers_listctrl.SelectDatas( [ order[ i ] for i in selection ], deselect_others = True )

            menus.append( { 'selected' : selection, 'menu' : tree( sidebar._GetListCtrlMenu() ) } )


        out[ 'gallery' ] = menus

        # watchers
        page = gui._notebook.NewPageImportMultipleWatcher()

        sidebar = page.GetSidebar()

        sidebar._AddURLs( URLS )

        importers = list( sidebar._multiple_watcher_import.GetWatchers() )

        for importer in importers:

            importer.PausePlayChecking()


        order = [ next( i for i in importers if i.GetURL() == u ) for u in URLS ]

        give_seeds( order[ 0 ], 'https://boards.example' )

        menus = []

        for selection in SELECTIONS:

            sidebar._watchers_listctrl.SelectDatas( [ order[ i ] for i in selection ], deselect_others = True )

            menus.append( { 'selected' : selection, 'menu' : tree( sidebar._GetListCtrlMenu() ) } )


        out[ 'watcher' ] = menus

        summaries = []

        for ( status, inbox, location ) in PRESENTATIONS:

            options = PresentationImportOptions.PresentationImportOptions()

            options.SetPresentationStatus( status )
            options.SetPresentationInbox( inbox )

            if location == 'storage':

                options.SetLocationContext( ClientLocation.LocationContext.STATICCreateSimple( CC.HYDRUS_LOCAL_FILE_STORAGE_SERVICE_KEY ) )


            summaries.append( {
                'status' : status,
                'inbox' : inbox,
                'location' : location,
                'downloader' : options.GetSummary( IOC.IMPORT_OPTIONS_CALLER_TYPE_POST_URLS ),
                'watcher' : options.GetSummary( IOC.IMPORT_OPTIONS_CALLER_TYPE_WATCHER_URLS ),
            } )


        out[ 'summaries' ] = summaries

        return out


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

        path = os.path.join( work, 'importer_menus.json' )

        hydrus_driver.run_in_subprocess( os.path.abspath( __file__ ), '--child', path )

        with open( path ) as f:

            result = json.load( f )



    with open( OUT, 'w' ) as f:

        json.dump( result, f, indent = 1, ensure_ascii = False )
        f.write( '\n' )


    print( f'wrote {OUT}' )


if __name__ == '__main__':

    main()

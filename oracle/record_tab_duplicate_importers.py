#!/usr/bin/env python3
"""Record duplicating importer pages from the tab right-click menu
(`PagesNotebook._DuplicatePage`, "duplicate page").

In the running client, on the `basic` fixture, the main notebook is given a
url import page holding four URLs and a simple downloader page holding two
(network traffic paused, their file work paused), and a search page. Each
importer page is duplicated through its tab's menu; then the copy is given a
URL and, after that, the original is, to see that the two queues are separate.

After each step: the tabs, and for each importer page (`original`, `copy`)
its file log (each URL and its status) and whether its work is paused.

Usage: QT_QPA_PLATFORM=offscreen ~/pyenv/bin/python oracle/record_tab_duplicate_importers.py
       (writes fixtures/tab_duplicate_importers.json)
"""

import json
import sys
import tempfile
import time
from pathlib import Path

HERE = Path( __file__ ).resolve().parent

sys.path.insert( 0, str( HERE ) )

URLS = [ f'https://site.example/post/{n}' for n in range( 1, 5 ) ]

SIMPLE_URLS = [ f'https://site.example/file/{n}.jpg' for n in range( 1, 3 ) ]

EXTRA = 'https://site.example/extra/1'


def record( session ):

    from qtpy import QtWidgets as QW

    from hydrus.client import ClientConstants as CC, ClientLocation
    from hydrus.client.gui import ClientGUICore
    from hydrus.client.importing import ClientImportFileSeeds

    controller = session.controller
    gui = controller.gui

    def qt( f ):

        return controller.CallBlockingToQt( gui, f )


    notebook = gui._notebook

    live = []

    def capture( window, menu ):

        live.append( menu )

    ClientGUICore.core().PopupMenu = capture

    def names():

        return [ p.GetName() for p in notebook.GetPages() ]


    def setup():

        while notebook.count() > 0:

            notebook.widget( 0 ).deleteLater()
            notebook.removeTab( 0 )


        del notebook._closed_pages[:]

        location = ClientLocation.LocationContext.STATICCreateSimple( CC.LOCAL_FILE_SERVICE_KEY )

        urls_page = notebook.NewPageImportURLs( page_name = 'url import' )

        simple_page = notebook.NewPageImportSimpleDownloader( page_name = 'simple downloader' )

        notebook.NewPageQuery( location, page_name = 'c' )

        urls_import = urls_page.GetSidebar()._urls_import

        urls_import.PausePlay()

        urls_import.PendURLs( URLS )

        simple_import = simple_page.GetSidebar()._simple_downloader_import

        simple_import.PausePlayFiles()

        simple_import._file_seed_cache.AddFileSeeds( [ ClientImportFileSeeds.FileSeed( ClientImportFileSeeds.FILE_SEED_TYPE_URL, url ) for url in SIMPLE_URLS ] )

        notebook.setCurrentIndex( 0 )

    qt( setup )

    time.sleep( 1 )

    def importer_of( page ):

        sidebar = page.GetSidebar()

        if hasattr( sidebar, '_urls_import' ):

            importer = sidebar._urls_import

            paused = importer._paused

        else:

            importer = sidebar._simple_downloader_import

            paused = importer._files_paused


        seeds = importer._file_seed_cache.GetFileSeeds()

        return {
            'urls' : [ s.file_seed_data for s in seeds ],
            'statuses' : [ s.status for s in seeds ],
            'paused' : paused,
        }


    def describe( first_name ):

        pages = [ p for p in notebook.GetPages() if p.GetName() == first_name ]

        return {
            'tabs' : names(),
            'pages' : [ importer_of( p ) for p in pages ],
            'keys' : [ p.GetPageKey().hex() for p in pages ],
        }


    def duplicate( name ):

        index = names().index( name )

        del live[:]

        notebook._ShowMenuForTabIndex( index )

        next( a for a in live[0].actions() if a.text() == 'duplicate page' ).trigger()

        QW.QApplication.processEvents()


    def add_url( name, which, url ):

        page = [ p for p in notebook.GetPages() if p.GetName() == name ][ which ]

        sidebar = page.GetSidebar()

        if hasattr( sidebar, '_urls_import' ):

            sidebar._urls_import.PendURLs( [ url ] )

        else:

            sidebar._simple_downloader_import._file_seed_cache.AddFileSeeds( [ ClientImportFileSeeds.FileSeed( ClientImportFileSeeds.FILE_SEED_TYPE_URL, url ) ] )


    result = []

    for name in ( 'url import', 'simple downloader' ):

        steps = [ { 'do' : 'start', 'state' : qt( lambda: describe( name ) ) } ]

        qt( lambda: duplicate( name ) )

        time.sleep( 0.5 )

        steps.append( { 'do' : 'duplicate', 'state' : qt( lambda: describe( name ) ) } )

        qt( lambda: add_url( name, 1, EXTRA ) )

        steps.append( { 'do' : 'add to the copy', 'state' : qt( lambda: describe( name ) ) } )

        qt( lambda: add_url( name, 0, EXTRA + '/2' ) )

        steps.append( { 'do' : 'add to the original', 'state' : qt( lambda: describe( name ) ) } )

        result.append( { 'page' : name, 'steps' : steps } )

        qt( setup )

        time.sleep( 1 )


    return { 'urls' : URLS, 'simple_urls' : SIMPLE_URLS, 'extra' : EXTRA, 'pages' : result }


def main():

    import hydrus_driver
    import record_api

    if len( sys.argv ) > 1 and sys.argv[ 1 ] == '--child':

        Path( sys.argv[ 2 ] ).write_text( json.dumps( hydrus_driver.run_client( record_api.unpack_fixture( 'basic' ), record ) ) )

        return


    with tempfile.TemporaryDirectory() as directory:

        path = Path( directory ) / 'result.json'

        hydrus_driver.run_in_subprocess( str( Path( __file__ ).resolve() ), '--child', str( path ) )

        result = json.loads( path.read_text() )


    ( HERE / 'fixtures' / 'tab_duplicate_importers.json' ).write_text( json.dumps( result, indent = 2 ) + '\n' )

    print( 'wrote tab_duplicate_importers.json' )


if __name__ == '__main__':

    main()

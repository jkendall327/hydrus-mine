#!/usr/bin/env python3
"""Record closing pages by a middle click on their tab and by Ctrl+W, with
the importer pages' close questions.

In the running client, on the `basic` fixture, the main notebook is given
four pages: a search page "a", a url import page holding four URLs, a
simple downloader page holding two, and a search page "c" (network traffic
paused, so nothing is fetched). The importers start paused. Then a script
of steps:

* `["select", name]`: that tab made current;
* `["middle", name, answer]`: a real middle click on that page's tab;
* `["ctrl_w", answer]`: a real Ctrl+W key press on the main window;
* `["play", name]`: that importer page's file work resumed.

The yes/no question a close asks (`GetYesNo`) is answered `answer`, and
recorded. After each step: the questions asked, the tabs in order, and the
current page.

Usage: QT_QPA_PLATFORM=offscreen ~/pyenv/bin/python oracle/record_page_close.py
       (writes fixtures/page_close.json)
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

STEPS = [
    [ 'middle', 'url import', False ],
    [ 'play', 'url import' ],
    [ 'select', 'url import' ],
    [ 'ctrl_w', False ],
    [ 'ctrl_w', True ],
    [ 'select', 'simple downloader' ],
    [ 'ctrl_w', False ],
    [ 'middle', 'simple downloader', True ],
    [ 'middle', 'a', True ],
    [ 'ctrl_w', True ],
]


def record( session ):

    from qtpy import QtCore as QC
    from qtpy import QtWidgets as QW
    from qtpy import QtTest as QT

    from hydrus.client.gui import ClientGUIDialogsQuick
    from hydrus.client.importing import ClientImportFileSeeds

    controller = session.controller
    gui = controller.gui

    def qt( f ):

        return controller.CallBlockingToQt( gui, f )


    asked = []

    answer = { 'yes' : False }

    def get_yes_no( parent, message, *args, **kwargs ):

        asked.append( message )

        return QW.QDialog.DialogCode.Accepted if answer[ 'yes' ] else QW.QDialog.DialogCode.Rejected


    ClientGUIDialogsQuick.GetYesNo = get_yes_no

    notebook = gui._notebook

    def tabs():

        return [ notebook.tabText( i ) for i in range( notebook.count() ) ]


    def names():

        return [ p.GetName() for p in notebook.GetPages() ]


    def current():

        page = notebook.GetCurrentMediaPage()

        return None if page is None else page.GetName()


    def page_named( name ):

        return next( p for p in notebook.GetPages() if p.GetName() == name )


    def setup():

        while notebook.count() > 0:

            notebook.widget( 0 ).deleteLater()
            notebook.removeTab( 0 )


        del notebook._closed_pages[:]

        location_context = controller.new_options.GetDefaultLocalLocationContext()

        notebook.NewPageQuery( location_context, page_name = 'a' )

        urls_page = notebook.NewPageImportURLs()

        simple_page = notebook.NewPageImportSimpleDownloader()

        notebook.NewPageQuery( location_context, page_name = 'c' )

        urls_import = urls_page.GetSidebar()._urls_import

        urls_import.PausePlay()

        urls_import.PendURLs( URLS )

        simple_import = simple_page.GetSidebar()._simple_downloader_import

        simple_import.PausePlayFiles()

        simple_import._file_seed_cache.AddFileSeeds( [ ClientImportFileSeeds.FileSeed( ClientImportFileSeeds.FILE_SEED_TYPE_URL, url ) for url in SIMPLE_URLS ] )

        notebook.setCurrentIndex( 0 )

        return {
            'confirm_non_empty_downloader_page_close' : controller.new_options.GetBoolean( 'confirm_non_empty_downloader_page_close' ),
            'close_page_focus_goes' : controller.new_options.GetInteger( 'close_page_focus_goes' ),
            'urls_paused' : urls_import._paused,
            'simple_paused' : simple_import._files_paused,
        }


    options = qt( setup )

    time.sleep( 1 )

    def state():

        return { 'tabs' : names(), 'current' : current() }


    steps = [ { 'state' : qt( state ) } ]

    for step in STEPS:

        del asked[:]

        def do():

            kind = step[ 0 ]

            if kind == 'select':

                notebook.setCurrentIndex( names().index( step[ 1 ] ) )

            elif kind == 'play':

                sidebar = page_named( step[ 1 ] ).GetSidebar()

                sidebar._urls_import.PausePlay()

            elif kind == 'middle':

                answer[ 'yes' ] = step[ 2 ]

                bar = notebook.tabBar()

                index = names().index( step[ 1 ] )

                QT.QTest.mouseClick( bar, QC.Qt.MouseButton.MiddleButton, QC.Qt.KeyboardModifier.NoModifier, bar.tabRect( index ).center() )

            elif kind == 'ctrl_w':

                answer[ 'yes' ] = step[ 1 ]

                gui.activateWindow()

                target = QW.QApplication.focusWidget() or notebook.currentWidget()

                QT.QTest.keyClick( target, QC.Qt.Key.Key_W, QC.Qt.KeyboardModifier.ControlModifier )


            QW.QApplication.processEvents()


        qt( do )

        time.sleep( 0.3 )

        steps.append( { 'do' : step, 'asked' : list( asked ), 'state' : qt( state ) } )


    return { 'options' : options, 'urls' : URLS, 'simple_urls' : SIMPLE_URLS, 'steps' : steps }


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


    ( HERE / 'fixtures' / 'page_close.json' ).write_text( json.dumps( result, indent = 2 ) + '\n' )

    print( 'wrote page_close.json' )


if __name__ == '__main__':

    main()

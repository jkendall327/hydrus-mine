#!/usr/bin/env python3
"""Record what the reference's pages menu entries open.

In the running client, on the `basic` fixture, the real menu bar's pages
menu is walked to its entries and each is triggered: "new page of pages"
(special), "new url download page", "new watcher page", "new gallery
page", "new simple downloader page" (download), "new duplicates
processing page" (special) and each entry of "file search" (a search page
of each local file domain, the trash). Each is done twice, with the
current page a search page and with the current page a page of pages
(`on_deepest_notebook`: the new page goes inside the deepest page of
pages). The notebook tree is recorded afterwards: each page's name, type
and, for a page of pages, its pages; with the path of tabs that are
current, and what the "file search" entries are called.

Usage: QT_QPA_PLATFORM=offscreen python oracle/record_new_page_menu.py
       (writes fixtures/new_page_menu.json)
"""

import json
import os
import sys
import time

HERE = os.path.dirname( os.path.abspath( __file__ ) )

sys.path.insert( 0, HERE )

OUT = os.path.join( HERE, 'fixtures', 'new_page_menu.json' )

ENTRIES = [
    ( 'special', 'new page of pages' ),
    ( 'special', 'new duplicates processing page' ),
    ( 'download', 'new url download page' ),
    ( 'download', 'new watcher page' ),
    ( 'download', 'new gallery page' ),
    ( 'download', 'new simple downloader page' ),
]


def record( session ):

    from qtpy import QtWidgets as QW

    from hydrus.client import ClientConstants as CC
    from hydrus.client import ClientLocation
    from hydrus.client.gui.pages import ClientGUIPages

    controller = session.controller
    gui = controller.gui

    def qt( f ):

        return controller.CallBlockingToQt( gui, f )


    def tree( notebook ):

        out = []

        for page in notebook.GetPages():

            entry = { 'name' : page.GetName(), 'type' : page._page_manager.GetType() if hasattr( page, '_page_manager' ) else 'pages' }

            if isinstance( page, ClientGUIPages.PagesNotebook ):

                entry[ 'type' ] = 'pages'
                entry[ 'pages' ] = tree( page )

            elif entry[ 'type' ] == 6:

                location_context = page._page_manager.GetLocationContext()

                entry[ 'location' ] = {
                    'current' : sorted( controller.services_manager.GetName( k ) for k in location_context.current_service_keys ),
                    'deleted' : sorted( controller.services_manager.GetName( k ) for k in location_context.deleted_service_keys ),
                }


            out.append( entry )


        return out


    def current_path():

        path = []

        notebook = gui._notebook

        while isinstance( notebook, ClientGUIPages.PagesNotebook ):

            path.append( notebook.currentIndex() )

            notebook = notebook.currentWidget()

            if not isinstance( notebook, ClientGUIPages.PagesNotebook ):

                break



        return path


    def find_action( menu, labels ):

        for action in menu.actions():

            if action.text().replace( '&', '' ) == labels[ 0 ]:

                if len( labels ) == 1:

                    return action


                if action.menu() is not None:

                    return find_action( action.menu(), labels[ 1 : ] )



        return None


    def pages_menu():

        for action in gui.menuBar().actions():

            if action.text().replace( '&', '' ) == 'pages':

                return action.menu()


        raise Exception( 'no pages menu' )


    # (let the menu fill in its file search entries)
    qt( lambda: gui._menu_updater_pages.update() )

    time.sleep( 1.5 )

    search_labels = qt( lambda: [ a.text() for a in find_action( pages_menu(), [ 'file search' ] ).menu().actions() ] )

    def reset( inside ):

        def f():

            notebook = gui._notebook

            while notebook.count() > 0:

                notebook.widget( 0 ).deleteLater()
                notebook.removeTab( 0 )


            default_location_context = controller.new_options.GetDefaultLocalLocationContext()

            if inside:

                outer = notebook.NewPagesNotebook( name = 'outer', give_it_a_blank_page = False )

                outer.NewPageQuery( default_location_context, page_name = 'inner' )

            else:

                notebook.NewPageQuery( default_location_context, page_name = 'start' )


        qt( f )

        time.sleep( 0.3 )


    cases = []

    # (the new page chooser, standing in for the user's choice: a page of
    # pages, or none)
    from qtpy import QtWidgets as QW

    from hydrus.client.gui.pages import ClientGUINewPageChooser

    real_chooser = ClientGUINewPageChooser.DialogPageChooser

    class Chosen:

        cancel = False

        def __init__( self, *args ): pass
        def __enter__( self ): return self
        def __exit__( self, *args ): pass
        def exec( self ): return QW.QDialog.DialogCode.Rejected if Chosen.cancel else QW.QDialog.DialogCode.Accepted
        def GetValue( self ): return ( 'pages', None )


    ClientGUINewPageChooser.DialogPageChooser = Chosen

    for cancel in ( False, True ):

        for inside in ( False, True ):

            reset( inside )

            Chosen.cancel = cancel

            before = qt( lambda: tree( gui._notebook ) )

            qt( lambda: find_action( pages_menu(), [ 'new page\u2026' ] ).trigger() )

            time.sleep( 0.5 )

            cases.append( {
                'entry' : [ 'new page\u2026' ],
                'chooser' : 'cancelled' if cancel else 'page of pages',
                'inside' : inside,
                'before' : before,
                'after' : qt( lambda: tree( gui._notebook ) ),
                'current' : qt( current_path ),
            } )


    ClientGUINewPageChooser.DialogPageChooser = real_chooser

    for ( labels, name ) in [ ( [ s, e ], e ) for ( s, e ) in ENTRIES ] + [ ( [ 'file search', label ], label ) for label in search_labels ]:

        for inside in ( False, True ):

            reset( inside )

            before = qt( lambda: tree( gui._notebook ) )

            def trigger():

                action = find_action( pages_menu(), labels )

                action.trigger()


            qt( trigger )

            time.sleep( 0.5 )

            cases.append( {
                'entry' : labels,
                'inside' : inside,
                'before' : before,
                'after' : qt( lambda: tree( gui._notebook ) ),
                'current' : qt( current_path ),
            } )


    return { 'search_entries' : search_labels, 'cases' : cases }


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

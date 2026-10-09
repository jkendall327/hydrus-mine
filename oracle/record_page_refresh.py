#!/usr/bin/env python3
"""Record the reference's pages > refresh (`ClientGUI._RefreshCurrentPage`).

In the running client, on the `basic` fixture, with new searches running as
they are given (`default_search_synchronised`, as it comes), these pages are made, each made current
in turn, and "refresh" is run on it: a search page paused with a predicate typed since, a search page whose search has run and then been paused, a
search page opened on given files (its search locked to their hashes), and a
URL importer page. Before and after, whether the page's search is
synchronised and how many files it shows (once they have loaded) are
recorded.

Usage: QT_QPA_PLATFORM=offscreen python oracle/record_page_refresh.py
       (writes fixtures/page_refresh.json)
"""

import json
import os
import sys
import time

HERE = os.path.dirname( os.path.abspath( __file__ ) )

sys.path.insert( 0, HERE )

OUT = os.path.join( HERE, 'fixtures', 'page_refresh.json' )
MANIFEST = json.load( open( os.path.join( HERE, 'fixtures', 'legacy_db', 'basic.manifest.json' ) ) )


def record( session ):

    from hydrus.client import ClientConstants as CC
    from hydrus.client import ClientLocation
    from hydrus.client.search import ClientSearchPredicate

    controller = session.controller
    gui = controller.gui

    def qt( f ):

        return controller.CallBlockingToQt( gui, f )


    context = ClientLocation.LocationContext.STATICCreateSimple( CC.LOCAL_FILE_SERVICE_KEY )

    everything = [ ClientSearchPredicate.Predicate( ClientSearchPredicate.PREDICATE_TYPE_SYSTEM_EVERYTHING ) ]

    given = [ bytes.fromhex( f[ 'hash' ] ) for f in MANIFEST[ 'files' ][ :3 ] ]

    # (new searches run as they are given, the default; the cases pause them by hand)
    controller.new_options.SetBoolean( 'default_search_synchronised', True )

    def state( page ):

        panel = page._sidebar_management_panel

        synchronised = None

        if hasattr( panel, '_tag_autocomplete' ):

            synchronised = panel._tag_autocomplete.IsSynchronised()


        return { 'synchronised' : synchronised, 'files' : len( page.GetHashes() ) }


    def settle( page ):

        for _ in range( 200 ):

            time.sleep( 0.05 )

        return qt( lambda: state( page ) )


    results = []

    def run( name, make, prepare = None ):

        def build():

            gui._notebook.NewPageQuery

            page = make()

            return page


        page = qt( build )

        for _ in range( 400 ):

            if qt( lambda: page._initialised ):

                break


            time.sleep( 0.025 )


        if prepare is not None:

            qt( lambda: prepare( page ) )

            time.sleep( 1.0 )


        qt( lambda: gui._notebook.setCurrentWidget( page ) )

        before = settle( page )

        qt( lambda: gui._RefreshCurrentPage() )

        after = settle( page )

        results.append( { 'page' : name, 'before' : before, 'after' : after } )


    inbox = ClientSearchPredicate.Predicate( ClientSearchPredicate.PREDICATE_TYPE_SYSTEM_INBOX )

    def pause_and_type( page ):

        page._sidebar_management_panel._tag_autocomplete.SetSynchronised( False )

        page.EnterPredicates( [ inbox ] )

    run( 'paused with a predicate typed since', lambda: gui._notebook.NewPageQuery( context, page_name = 'typed', initial_predicates = everything ), prepare = pause_and_type )

    def search_then_pause( page ):

        panel = page._sidebar_management_panel

        panel._tag_autocomplete.SetSynchronised( True )

    def pause( page ):

        page._sidebar_management_panel._tag_autocomplete.SetSynchronised( False )

    def searched_and_paused():

        page = gui._notebook.NewPageQuery( context, page_name = 'paused', initial_predicates = everything )

        search_then_pause( page )

        return page

    run( 'searched, then paused', searched_and_paused, prepare = pause )

    run( 'opened on given files', lambda: gui._notebook.NewPageQuery( context, page_name = 'given', initial_hashes = given ) )

    run( 'url importer', lambda: gui._notebook.NewPageImportURLs( page_name = 'urls' ) )

    return { 'given': len( given ), 'cases' : results }


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

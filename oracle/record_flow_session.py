#!/usr/bin/env python3
"""Flow: pages saved as the client closes and restored when it opens again.

On the `basic` fixture, with the fake booru and imageboard (`flows.Site`)
and their downloaders installed, two runs of the client on one database:

1. "build" (the first run): a search page for `safe` with two of its files
   selected (the first two it shows); a page of pages (with the blank
   search page it starts with) holding a URL downloader page that has
   downloaded a post (not one of the thread's files), and a watcher page that has checked a thread (three
   files); the watcher page left shown. Wait until all is idle, then the
   session saved as the client saves it on exit (`SaveAndHide`: "last
   session" and "exit session").
2. "restored" (the second run): the client opens on its last session; wait
   until it is idle.

After each: the tree of pages (each one's name and kind; a search page's
predicates, how many files it shows and how many are selected; a URL page's
file log; a watcher page's watchers, each with its URL, subject and file
log; and how many files each importer page shows), which page is shown
(its names from the top), and the site's requests (after the restart,
nothing should be fetched again).

Usage: QT_QPA_PLATFORM=offscreen ~/pyenv/bin/python oracle/record_flow_session.py
       (writes oracle/fixtures/flow_session.json)
"""

import json
import os
import sys
import tempfile
import time

HERE = os.path.dirname( os.path.abspath( __file__ ) )
sys.path.insert( 0, HERE )

import flows

OUT = os.path.join( HERE, 'fixtures', 'flow_session.json' )

SITE_PORT = 46016

THREAD = 100

POST = 4


def phases():

    # (post 4: a file the thread does not have, so neither importer finds
    # the other's file already there, whichever runs first)
    booru = flows.booru_phase( { 'blue_eyes' : [ POST ] } )
    thread = flows.thread_phase( { THREAD : ( 'Blue sky thread', [ ( 1, 'anon' ), ( 2, 'sky' ), ( 3, 'anon' ) ] ) } )

    return [ { 'pages' : { **booru[ 'pages' ], **thread[ 'pages' ] }, 'files' : { **booru[ 'files' ], **thread[ 'files' ] } } ]


def definitions( site ):

    ( u1, p1, g1 ) = flows.definitions( site )
    ( u2, p2, g2 ) = flows.thread_definitions( site )

    return ( u1 + u2, p1 + p2, g1 + g2 )


def tree_of( session, normalise ):
    """The pages, as the user can see them, and the shown page's names."""

    from hydrus.client import ClientConstants as CC
    from hydrus.client.gui.pages import ClientGUIPages
    from hydrus.client.gui.pages import ClientGUIPagesCore as Core

    controller = session.controller
    gui = controller.gui

    kinds = {
        Core.PAGE_TYPE_QUERY : 'search',
        Core.PAGE_TYPE_IMPORT_URLS : 'urls',
        Core.PAGE_TYPE_IMPORT_MULTIPLE_WATCHER : 'watcher',
    }

    def describe( page ):

        if isinstance( page, ClientGUIPages.PagesNotebook ):

            return { 'name' : page.GetName(), 'kind' : 'pages', 'pages' : [ describe( p ) for p in page.GetPages() ] }


        manager = page.GetPageManager()
        kind = kinds[ manager.GetType() ]
        panel = page.GetMediaResultsPanel()

        out = { 'name' : page.GetName(), 'kind' : kind, 'shown files' : len( panel.GetSortedMedia() ) }

        if kind == 'search':

            out[ 'predicates' ] = [ p.ToString() for p in manager.GetVariable( 'file_search_context' ).GetPredicates() ]
            out[ 'selected' ] = len( panel._selected_media )

        elif kind == 'urls':

            out[ 'files' ] = flows.file_log_state( manager.GetVariable( 'urls_import' ).GetFileSeedCache(), normalise )

        elif kind == 'watcher':

            out[ 'watchers' ] = [
                {
                    'url' : normalise( w.GetURL() ),
                    'subject' : w.GetSubject(),
                    'files' : flows.file_log_state( w.GetFileSeedCache(), normalise ),
                }
                for w in manager.GetVariable( 'multiple_watcher_import' ).GetWatchers()
            ]


        return out


    def shown():

        names = []
        notebook = gui._notebook

        while True:

            page = notebook.currentWidget()

            if page is None:

                return names


            names.append( page.GetName() )

            if not isinstance( page, ClientGUIPages.PagesNotebook ):

                return names


            notebook = page



    def f():

        return { 'pages' : [ describe( p ) for p in gui._notebook.GetPages() ], 'shown' : shown() }


    return controller.CallBlockingToQt( gui, f )


def importers_idle( session ):
    """Every importer page has nothing left to do and shows what it got."""

    from hydrus.client import ClientConstants as CC
    from hydrus.client.gui.pages import ClientGUIPagesCore as Core

    controller = session.controller
    gui = controller.gui

    def f():

        for page in gui._notebook.GetMediaPages():

            manager = page.GetPageManager()

            if manager.GetType() == Core.PAGE_TYPE_IMPORT_URLS:

                importers = [ manager.GetVariable( 'urls_import' ) ]

            elif manager.GetType() == Core.PAGE_TYPE_IMPORT_MULTIPLE_WATCHER:

                importers = manager.GetVariable( 'multiple_watcher_import' ).GetWatchers()

            else:

                continue


            for importer in importers:

                if importer.GetFileSeedCache().GetFileSeedCount( CC.STATUS_UNKNOWN ) > 0 or importer.CurrentlyWorking():

                    return False




        return True


    return controller.CallBlockingToQt( gui, f )


def build( session, site ):

    from hydrus.client import ClientConstants as CC, ClientLocation
    from hydrus.client.gui.pages import ClientGUIPages
    from hydrus.client.media import ClientMediaFileFilter
    from hydrus.client.search import ClientSearchPredicate as P

    controller = session.controller
    gui = controller.gui
    normalise = flows.normaliser( site )

    def qt( f ):

        return controller.CallBlockingToQt( gui, f )


    domain_manager = qt( lambda: flows.install( controller, site, definitions ) )

    location = ClientLocation.LocationContext.STATICCreateSimple( CC.LOCAL_FILE_SERVICE_KEY )

    search = qt( lambda: gui._notebook.NewPageQuery( location, initial_predicates = [ P.Predicate( P.PREDICATE_TYPE_TAG, value = 'safe' ) ], do_sort = False ) )

    def loaded():

        panel = qt( search.GetMediaResultsPanel )

        return hasattr( panel, '_sorted_media' ) and len( panel._sorted_media ) > 0


    deadline = time.time() + 60

    while not loaded():

        if time.time() > deadline:

            raise Exception( 'the search page did not load' )


        time.sleep( 0.1 )


    time.sleep( 1.5 )

    def select_two():

        panel = search.GetMediaResultsPanel()

        panel._Select( ClientMediaFileFilter.FileFilter( ClientMediaFileFilter.FILE_FILTER_NONE ) )

        for ( i, media ) in enumerate( panel.GetSortedMedia()[ : 2 ] ):

            panel._HitMedia( media, i > 0, False )



    qt( select_two )

    qt( lambda: gui._notebook.NewPagesNotebook() )

    urls = qt( lambda: gui._notebook.NewPageImportURLs( on_deepest_notebook = True ) )

    def enter_url():

        sidebar = urls.GetSidebar()

        sidebar._url_input.SetValue( site.base + f'/post/{POST}' )
        sidebar._url_input.EnterText()


    qt( enter_url )

    watcher = qt( lambda: gui._notebook.NewPageImportMultipleWatcher( on_deepest_notebook = True ) )

    def enter_thread():

        sidebar = watcher.GetSidebar()

        sidebar._watcher_url_input.SetValue( site.base + f'/thread/{THREAD}' )
        sidebar._watcher_url_input.EnterText()


    qt( enter_thread )

    def done():

        if not importers_idle( session ):

            return False


        # (the pages show what their importers got)
        return qt( lambda: len( urls.GetMediaResultsPanel().GetSortedMedia() ) ) == 1 and qt( lambda: len( watcher.GetMediaResultsPanel().GetSortedMedia() ) ) == 3


    flows.wait_until_idle( controller, also = done )

    tree = tree_of( session, normalise )

    # what the client does with its pages as it closes (`SaveAndHide`)
    def save():

        only_changed_page_data = True
        about_to_save = True

        saved = gui._notebook.GetCurrentGUISession( CC.LAST_SESSION_SESSION_NAME, only_changed_page_data, about_to_save )
        saved = gui._FleshOutSessionWithCleanDataIfNeeded( gui._notebook, CC.LAST_SESSION_SESSION_NAME, saved )

        controller.SaveGUISession( saved )

        saved.SetName( CC.EXIT_SESSION_SESSION_NAME )

        controller.SaveGUISession( saved )


    qt( save )

    flows.wait_until_idle( controller )

    return { 'domain_manager' : domain_manager, 'step' : { 'step' : 'build', **tree, 'hits' : site.take_hits() } }


def restore( session, site ):

    controller = session.controller
    normalise = flows.normaliser( site )

    # (the session loads a moment after the client boots)
    def loaded():

        return controller.CallBlockingToQt( controller.gui, lambda: len( controller.gui._notebook.GetMediaPages() ) ) >= 4


    deadline = time.time() + 60

    while not loaded():

        if time.time() > deadline:

            raise Exception( 'the session did not load' )


        time.sleep( 0.2 )


    flows.wait_until_idle( controller, also = lambda: importers_idle( session ) )

    time.sleep( 2.0 )

    tree = tree_of( session, normalise )

    return { 'step' : { 'step' : 'restored', **tree, 'hits' : site.take_hits() } }


def avoid_shutdown_deadlock( controller ):
    """The reference's exit can deadlock: a thumbnail cache's loop, ending,
    clears itself and asks the Qt thread for `image.svg`'s size
    (`GetSVGResolution`) while holding its lock, as the Qt thread, closing
    the pages, waits on that lock. The size is fetched once here, on the Qt
    thread, and kept, so the cache never waits on Qt."""

    from hydrus.client import ClientSVGHandling
    from hydrus.core import HydrusStaticDir

    original = ClientSVGHandling.GetSVGResolution
    known = {}

    def remembered( path ):

        if path not in known:

            known[ path ] = original( path )


        return known[ path ]


    path = HydrusStaticDir.GetStaticPath( 'image.svg' )

    known[ path ] = original( path )

    ClientSVGHandling.GetSVGResolution = remembered


def child( which, db_dir, out ):

    import hydrus_driver

    with flows.Site( SITE_PORT, phases() ) as site:

        hook = build if which == 'build' else restore

        def run( session ):

            avoid_shutdown_deadlock( session.controller )

            result = hook( session, site )

            result[ 'site' ] = site.describe()

            # (written before the client closes: the reference's shutdown can
            # deadlock, a thumbnail cache thread waiting on the Qt thread
            # while that waits on the cache, and is then killed; what is
            # recorded, the session included, is in the database by now)
            with open( out, 'w' ) as f:

                json.dump( result, f )


        hydrus_driver.run_client( db_dir, run, network = True )



def run_child( which, db_dir, out ):
    """The child, killed if it has written its result but does not close
    within a minute (see `child`)."""

    import subprocess

    env = dict( os.environ )
    env.setdefault( 'QT_QPA_PLATFORM', 'offscreen' )

    process = subprocess.Popen( [ sys.executable, os.path.abspath( __file__ ), '--child', which, db_dir, out ], env = env, cwd = os.path.dirname( HERE ) )

    written_at = None

    while process.poll() is None:

        if written_at is None and os.path.exists( out ):

            written_at = time.time()


        if written_at is not None and time.time() - written_at > 60:

            process.kill()
            process.wait()

            break


        time.sleep( 0.5 )


    if not os.path.exists( out ):

        raise Exception( f'the {which} run failed' )



def main():

    if len( sys.argv ) > 4 and sys.argv[1] == '--child':

        child( sys.argv[2], sys.argv[3], sys.argv[4] )

        return


    import record_api

    db_dir = record_api.unpack_fixture( 'basic' )

    with tempfile.TemporaryDirectory() as work:

        built = os.path.join( work, 'build.json' )
        restored = os.path.join( work, 'restore.json' )

        run_child( 'build', db_dir, built )
        run_child( 'restore', db_dir, restored )

        first = json.load( open( built ) )
        second = json.load( open( restored ) )


    flows.write_fixture( OUT, {
        'domain_manager' : first[ 'domain_manager' ],
        'site' : first[ 'site' ],
        'steps' : [ first[ 'step' ], second[ 'step' ] ],
    } )


if __name__ == '__main__':

    main()

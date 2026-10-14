#!/usr/bin/env python3
"""Flow: a gallery downloader page, searching with a file limit.

On the `basic` fixture, with the fake booru (`flows.Site`: tag searches
three posts a page, newest first) and its downloader installed:

1. "search": a new gallery downloader page (`NewPageImportGallery`), the
   downloader chosen, "stop after this many files" set to 5, and `blue_eyes`
   (eight posts, so three result pages) entered. The new query is
   highlighted (hydrus's default) and searches until it has five files, then
   fetches them.
2. "second search": `red_hair` entered (posts 3, 9 and 10: post 3 is already
   in the database from the first search). It searches its one page and
   fetches the new files; the highlight stays on the first query.

3. "raise the limit": the highlighted query's own "stop after this many
   files" raised to 20. Its search had stopped at the limit, and with no
   page left to read it cannot go on (the limit stopped it adding the next
   page): nothing more is fetched, but its search status ("hit file limit")
   clears.

After each step: each query's file log and search log, the files (tags,
URLs, inbox, domains), each query's row in the list, the list's totals, the
highlighted query's box, the page's thumbnails, the popups still shown and
the site's requests. Texts are normalised by `flows.normaliser`.

Usage: QT_QPA_PLATFORM=offscreen ~/pyenv/bin/python oracle/record_flow_gallery.py
       (writes oracle/fixtures/flow_gallery.json)
"""

import os
import sys

HERE = os.path.dirname( os.path.abspath( __file__ ) )
sys.path.insert( 0, HERE )

import flows

OUT = os.path.join( HERE, 'fixtures', 'flow_gallery.json' )

SITE_PORT = 46033

FILE_LIMIT = 5

PHASES = [
    flows.booru_phase( { 'blue_eyes' : [ 1, 2, 3, 4, 5, 6, 7, 8 ], 'red_hair' : [ 3, 9, 10 ] } ),
]


def record( session, site ):

    from hydrus.client import ClientConstants as CC

    controller = session.controller
    gui = controller.gui
    normalise = flows.normaliser( site )

    domain_manager = controller.CallBlockingToQt( gui, lambda: flows.install( controller, site ) )

    gug = controller.network_engine.domain_manager.GetGUGs()[ 0 ]

    steps = []

    def qt( f ):

        return controller.CallBlockingToQt( gui, f )


    page = qt( lambda: gui._notebook.NewPageImportGallery() )
    sidebar = page.GetSidebar()
    multiple = sidebar._multiple_gallery_import

    def media_panel():

        return page.GetMediaResultsPanel()


    def imports():

        return multiple.GetGalleryImports()


    def searched( n ):
        """`n` queries made, their searches finished and their files done."""

        gis = imports()

        if len( gis ) != n or sidebar._highlighted_gallery_import is None:

            return False


        for gi in gis:

            cache = gi.GetFileSeedCache()

            if not gi.GalleryFinished() or cache.GetFileSeedCount( CC.STATUS_UNKNOWN ) > 0 or gi.CurrentlyWorking():

                return False



        return True


    def texts():

        panel = sidebar._highlighted_gallery_import_panel

        sidebar._UpdateImportStatusNow()
        panel._UpdateStatus()
        panel._file_seed_cache_control._Update()
        panel._gallery_seed_log_control._Update()

        # ("just added" and the like are times: masked like the other times)
        rows = [ [ normalise( t ) for t in sidebar._ConvertDataToDisplayTuple( gi ) ] for gi in imports() ]

        return {
            'rows' : rows,
            'top' : normalise( sidebar._gallery_importers_status_st_top.text() ),
            'bottom' : normalise( sidebar._gallery_importers_status_st_bottom.text() ),
            'query' : normalise( panel._query_text.text() ),
            'files status' : normalise( panel._file_status.text() ),
            'file log summary' : normalise( panel._file_seed_cache_control._import_summary_st.text() ),
            'file log progress' : normalise( panel._file_seed_cache_control._progress_st.text() ),
            'search status' : normalise( panel._gallery_status.text() ),
            'search log summary' : normalise( panel._gallery_seed_log_control._log_summary_st.text() ),
        }


    def query_state( gi ):

        return {
            'text' : gi.GetQueryText(),
            'source' : gi.GetSourceName(),
            'files' : flows.file_log_state( gi.GetFileSeedCache(), normalise ),
            'searches' : [ flows.gallery_seed_state( g, normalise ) for g in gi.GetGallerySeedLog().GetGallerySeeds() ],
            'finished' : gi.GalleryFinished(),
            'files finished' : gi.FilesFinished(),
        }


    def step( name ):

        queries = [ query_state( gi ) for gi in imports() ]

        hashes = set()

        for q in queries:

            hashes |= flows.hashes_in_log( q[ 'files' ] )


        steps.append( {
            'step' : name,
            'dialog' : {},
            'page' : {
                'queries' : queries,
                'texts' : qt( texts ),
                'thumbnails' : qt( lambda: len( media_panel().GetSortedMedia() ) ),
            },
            'files' : flows.files_state( session, hashes, normalise ),
            'popups' : flows.popups_shown( controller, normalise ),
            'hits' : site.take_hits(),
        } )


    def enter( query ):

        sidebar._query_input.SetValue( query )
        sidebar._query_input.EnterText()


    # 1. search ----------------------------------------------------------

    def set_up_and_search():

        sidebar._gug_key_and_name.SetValue( gug.GetGUGKeyAndName() )
        sidebar._file_limit.SetValue( FILE_LIMIT )

        enter( 'blue_eyes' )


    with flows.scripted_dialogs( {} ) as dialogs:

        qt( set_up_and_search )

        flows.wait_until_idle( controller, also = lambda: dialogs.check() and searched( 1 ) )


    step( 'search' )

    # 2. second search ---------------------------------------------------

    with flows.scripted_dialogs( {} ) as dialogs:

        qt( lambda: enter( 'red_hair' ) )

        flows.wait_until_idle( controller, also = lambda: dialogs.check() and searched( 2 ) )


    step( 'second search' )

    # 3. raise the limit ------------------------------------------------

    def raise_limit():

        panel = sidebar._highlighted_gallery_import_panel

        panel._file_limit.SetValue( 20 )
        panel.EventFileLimit()


    def status_cleared():
        """The search's work loop has looked again (it does, now and then)
        and found no reason left to stop."""

        return imports()[ 0 ]._gallery_status == ''


    with flows.scripted_dialogs( {} ) as dialogs:

        qt( raise_limit )

        flows.wait_until_idle( controller, also = lambda: dialogs.check() and status_cleared() and searched( 2 ) )


    step( 'raise the limit' )

    return { 'domain_manager' : domain_manager, 'steps' : steps }


def child( out ):

    import hydrus_driver
    import record_api

    with flows.Site( SITE_PORT, PHASES ) as site:

        result = hydrus_driver.run_client( record_api.unpack_fixture( 'basic' ), lambda session: record( session, site ), network = True )


    result[ 'site' ] = site.describe()

    flows.write_fixture( out, result )


def main():

    if len( sys.argv ) > 2 and sys.argv[1] == '--child':

        child( sys.argv[2] )

        return


    import hydrus_driver

    hydrus_driver.run_in_subprocess( os.path.abspath( __file__ ), '--child', OUT )


if __name__ == '__main__':

    main()

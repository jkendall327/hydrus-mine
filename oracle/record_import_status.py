#!/usr/bin/env python3
"""Record how the reference words importers' progress, and pages' tab names.

In the running client:
- a file log's status (`FileSeedCacheStatus`), as its own `GetStatusText`
  writes it in full and simply (with the options that add new and deleted
  counts to the short summary each way), and its `GetValueRange`, for many
  sets of counts by status;
- a search log's status (`GenerateGallerySeedLogStatus`) for many sets of
  counts by status;
- a tab's name, as the notebook's own `_RefreshPageName` writes it, for
  stand-in pages and notebooks of many names, file counts and import
  progresses, under each of the options that shape it (the longest name,
  which pages show their file count, whether importers show progress,
  whether notebooks are decorated and with what).
- a network job's line (`NetworkJobControl`, under an importer's file
  log and search log), as the widget's own `_Update` sets it for stand-in
  jobs of many statuses, sizes, speeds and states: the left text, the
  right text, the gauge and whether cancel is enabled.

Usage: QT_QPA_PLATFORM=offscreen python oracle/record_import_status.py
       (writes fixtures/import_status.json)
"""

import json
import os
import random
import sys

HERE = os.path.dirname( os.path.abspath( __file__ ) )

sys.path.insert( 0, HERE )

OUT = os.path.join( HERE, 'fixtures', 'import_status.json' )

# CC.STATUS_*: unknown, successful and new, already in db, deleted, error,
# vetoed (ignored), skipped, created children
STATUSES = ( 0, 1, 2, 3, 4, 7, 8, 9 )

NAMES = (
    'url import',
    'files',
    'gallery',
    'a page name of some thirty chars',
    'a',
    '',
    'two\nlines',
    'x' * 300,
    'résumé — ünïcode',
)


def counts( rng ):

    out = {}

    for status in STATUSES:

        roll = rng.random()

        if roll < 0.45:

            continue

        elif roll < 0.85:

            out[ status ] = rng.randint( 1, 12 )

        elif roll < 0.97:

            out[ status ] = rng.randint( 13, 5000 )

        else:

            out[ status ] = rng.randint( 5001, 2_000_000 )



    return out


def record( session ):

    from hydrus.client import ClientConstants as CC
    from hydrus.client.gui.pages import ClientGUIPages
    from hydrus.client.importing import ClientImportFileSeeds
    from hydrus.client.importing import ClientImportGallerySeeds

    controller = session.controller
    new_options = controller.new_options

    rng = random.Random( 688 )

    count_sets = [ {} ] + [ counts( rng ) for _ in range( 400 ) ]

    file_logs = []

    before = ( new_options.GetBoolean( 'show_new_on_file_seed_short_summary' ), new_options.GetBoolean( 'show_deleted_on_file_seed_short_summary' ) )

    try:

        for statuses_to_counts in count_sets:

            status = ClientImportFileSeeds.FileSeedCacheStatus()

            for ( s, n ) in statuses_to_counts.items():

                status._statuses_to_counts[ s ] = n


            simple = {}

            for show_new in ( False, True ):

                for show_deleted in ( False, True ):

                    new_options.SetBoolean( 'show_new_on_file_seed_short_summary', show_new )
                    new_options.SetBoolean( 'show_deleted_on_file_seed_short_summary', show_deleted )

                    simple[ f'{int( show_new )}{int( show_deleted )}' ] = status.GetStatusText( simple = True )



            file_logs.append( {
                'counts' : { str( s ) : n for ( s, n ) in statuses_to_counts.items() },
                'full' : status.GetStatusText(),
                # keyed by show new, show deleted
                'simple' : simple,
                'value_range' : list( status.GetValueRange() ),
            } )


    finally:

        new_options.SetBoolean( 'show_new_on_file_seed_short_summary', before[ 0 ] )
        new_options.SetBoolean( 'show_deleted_on_file_seed_short_summary', before[ 1 ] )


    search_logs = []

    for statuses_to_counts in count_sets:

        import collections

        counter = collections.Counter( statuses_to_counts )

        ( text, ( value, rng_total ) ) = ClientImportGallerySeeds.GenerateGallerySeedLogStatus( counter )

        search_logs.append( {
            'counts' : { str( s ) : n for ( s, n ) in statuses_to_counts.items() },
            'status' : text,
            'value_range' : [ value, rng_total ],
        } )


    # tab names: the notebook's own _RefreshPageName on stand-ins

    class StandInPage( object ):

        def __init__( self, name, summary, importer ):

            self._name = name
            self._summary = summary
            self._importer = importer


        def GetName( self ):

            return self._name


        def GetNumFileSummary( self ):

            return self._summary


        def IsImporter( self ):

            return self._importer



    class StandInNotebook( ClientGUIPages.PagesNotebook ):

        def GetName( self ):

            return self._stand_in_name


        def GetNumFileSummary( self ):

            return self._stand_in_summary


        def IsImporter( self ):

            return False



    class TabBar( object ):

        def __init__( self ):

            self.text = None


        def tabText( self, index ):

            return ''


        def setTabText( self, index, text ):

            self.text = text



    class Signal( object ):

        def emit( self, *args ):

            pass



    class Holder( object ):

        def __init__( self, page ):

            self._page = page
            self._tab_bar = TabBar()
            self.dataChanged = Signal()
            self.tooltip = None


        def count( self ):

            return 1


        def widget( self, index ):

            return self._page


        def tabBar( self ):

            return self._tab_bar


        def setTabToolTip( self, index, text ):

            self.tooltip = text



    option_sets = [
        # max chars, file count display, import progress, decorate, decorator
        ( 20, CC.PAGE_FILE_COUNT_DISPLAY_ALL_BUT_ONLY_IF_GREATER_THAN_ZERO, True, True, ' ↓' ), # a new client's
        ( 20, CC.PAGE_FILE_COUNT_DISPLAY_ALL, True, True, ' ↓' ),
        ( 20, CC.PAGE_FILE_COUNT_DISPLAY_NONE, True, True, ' ↓' ),
        ( 20, CC.PAGE_FILE_COUNT_DISPLAY_ONLY_IMPORTERS, True, True, ' ↓' ),
        ( 20, CC.PAGE_FILE_COUNT_DISPLAY_ALL_BUT_ONLY_IF_GREATER_THAN_ZERO, False, False, ' ↓' ),
        ( 8, CC.PAGE_FILE_COUNT_DISPLAY_ALL, True, True, ' [+]' ),
        ( 64, CC.PAGE_FILE_COUNT_DISPLAY_ALL_BUT_ONLY_IF_GREATER_THAN_ZERO, True, True, '' ),
    ]

    summaries = [
        ( 0, ( 0, 0 ) ),
        ( 5, ( 0, 0 ) ),
        ( 1234567, ( 0, 0 ) ),
        ( 0, ( 3, 10 ) ),
        ( 5, ( 6, 10 ) ),
        ( 12, ( 12, 12 ) ),
        ( 3, ( 1500, 20000 ) ),
    ]

    keys = ( 'max_page_name_chars', 'page_file_count_display', 'import_page_progress_display', 'decorate_page_of_pages_tab_names', 'page_of_pages_decorator' )

    before = (
        new_options.GetInteger( 'max_page_name_chars' ),
        new_options.GetInteger( 'page_file_count_display' ),
        new_options.GetBoolean( 'import_page_progress_display' ),
        new_options.GetBoolean( 'decorate_page_of_pages_tab_names' ),
        new_options.GetString( 'page_of_pages_decorator' ),
    )

    tab_names = []

    try:

        for options in option_sets:

            ( max_chars, count_display, progress, decorate, decorator ) = options

            new_options.SetInteger( 'max_page_name_chars', max_chars )
            new_options.SetInteger( 'page_file_count_display', count_display )
            new_options.SetBoolean( 'import_page_progress_display', progress )
            new_options.SetBoolean( 'decorate_page_of_pages_tab_names', decorate )
            new_options.SetString( 'page_of_pages_decorator', decorator )

            for name in NAMES:

                for summary in summaries:

                    for kind in ( 'page', 'importer', 'notebook' ):

                        if kind == 'notebook':

                            page = StandInNotebook.__new__( StandInNotebook )
                            page._stand_in_name = name
                            page._stand_in_summary = summary

                        else:

                            page = StandInPage( name, summary, kind == 'importer' )


                        holder = Holder( page )

                        ClientGUIPages.PagesNotebook._RefreshPageName( holder, 0 )

                        tab_names.append( {
                            'options' : dict( zip( keys, options ) ),
                            'name' : name,
                            'kind' : kind,
                            'num_files' : summary[ 0 ],
                            'value_range' : list( summary[ 1 ] ),
                            'tab' : holder.tabBar().text,
                            'tooltip' : holder.tooltip,
                        } )





    finally:

        new_options.SetInteger( 'max_page_name_chars', before[ 0 ] )
        new_options.SetInteger( 'page_file_count_display', before[ 1 ] )
        new_options.SetBoolean( 'import_page_progress_display', before[ 2 ] )
        new_options.SetBoolean( 'decorate_page_of_pages_tab_names', before[ 3 ] )
        new_options.SetString( 'page_of_pages_decorator', before[ 4 ] )


    network_jobs = record_network_job_controls( controller )

    return { 'file_logs' : file_logs, 'search_logs' : search_logs, 'tab_names' : tab_names, 'network_jobs' : network_jobs }


def record_network_job_controls( controller ):

    from hydrus.client.gui.networking import ClientGUINetworkJobControl

    class StandInJob( object ):

        def __init__( self, status_text, speed, bytes_read, bytes_to_read, has_error, is_done, no_engine_yet ):

            self._status = ( status_text, speed, bytes_read, bytes_to_read )
            self._has_error = has_error
            self._is_done = is_done
            self._no_engine_yet = no_engine_yet


        def GetStatus( self ):

            return self._status


        def HasError( self ):

            return self._has_error


        def IsDone( self ):

            return self._is_done


        def NoEngineYet( self ):

            return self._no_engine_yet



    texts = ( '', 'downloading\u2026', 'waiting for a slot', 'Error: 404\nthe page was not found', 'sending request\u2026' )

    # ( bytes read, bytes to read )
    sizes = (
        ( None, None ),
        ( 0, None ),
        ( 0, 0 ),
        ( 0, 5000 ),
        ( 512, None ),
        ( 512, 512 ),
        ( 1023, 2048 ),
        ( 1536, 1536 ),
        ( 1048576, 5242880 ),
        ( 5242880, 5242880 ),
        ( 3000000000, 4500000000 ),
        ( 7000, 3000 ),
    )

    speeds = ( 0, 512, 1536, 300000, 5242880 )

    cases = [ None ]

    for text in texts:

        for ( bytes_read, bytes_to_read ) in sizes:

            for speed in speeds:

                cases.append( ( text, speed, bytes_read, bytes_to_read, False, False, False ) )



    for ( has_error, is_done, no_engine_yet ) in ( ( True, True, False ), ( False, True, False ), ( False, False, True ) ):

        for ( bytes_read, bytes_to_read ) in sizes:

            cases.append( ( 'Error: connection failed', 1536, bytes_read, bytes_to_read, has_error, is_done, no_engine_yet ) )



    def work():

        out = []

        for case in cases:

            # a fresh control each time: the right text is only set when
            # there is some (it is hidden, keeping the last, when not)
            control = ClientGUINetworkJobControl.NetworkJobControl( controller.gui )

            control._network_job = None if case is None else StandInJob( *case )

            control._Update()

            gauge = control._gauge

            out.append( {
                'job' : None if case is None else dict( zip( ( 'status_text', 'speed', 'bytes_read', 'bytes_to_read', 'has_error', 'is_done', 'no_engine_yet' ), case ) ),
                'left' : control._left_text._last_set_text,
                'right' : control._right_text._last_set_text,
                'gauge' : [ gauge.value(), gauge.maximum() ],
                'can_cancel' : control._cancel_button.isEnabled(),
            } )

            control._network_job = None

            control.deleteLater()


        return out


    return controller.CallBlockingToQt( controller.gui, work )


def main():

    import hydrus_driver
    import record_api

    db_dir = record_api.unpack_fixture( 'basic' )

    result = hydrus_driver.run_client( db_dir, record )

    with open( OUT, 'w' ) as f:

        json.dump( result, f, indent = 1, ensure_ascii = False )
        f.write( '\n' )


    print( f'{len( result[ "file_logs" ] )} file logs, {len( result[ "search_logs" ] )} search logs, {len( result[ "tab_names" ] )} tab names, {len( result[ "network_jobs" ] )} network jobs' )


if __name__ == '__main__':

    main()

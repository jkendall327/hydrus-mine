#!/usr/bin/env python3
"""Flow: watch a thread, and watch it grow.

On the `basic` fixture, with a fake imageboard (`flows.Site`, a thread of
three posts, each with a file and a poster tag) and its downloader installed:

1. "watch": a new watcher page (`NewPageImportMultipleWatcher`), the thread's
   URL typed in its "watcher url" box and entered. The new watcher is
   highlighted (hydrus's default), checks the thread, and fetches its three
   files.
2. "check again": the site adds a fourth post; the highlighted watcher's
   "check now" is pressed. It checks again (the reference waits half a
   minute after its last check) and fetches only the new file.

After each step: the watcher's file log and check log, its files (tags, URLs,
inbox, domains), the checker's timing (whether it has checked, whether its
next check is in the future, its checking status, paused or checking now),
the texts the user reads (the watcher's row in the list, the list's totals,
the highlighted watcher box), the page's thumbnails, the popups still shown
and the site's requests. Texts are normalised by `flows.normaliser`.

Usage: QT_QPA_PLATFORM=offscreen ~/pyenv/bin/python oracle/record_flow_watcher.py
       (writes oracle/fixtures/flow_watcher.json)
"""

import os
import sys
import time

HERE = os.path.dirname( os.path.abspath( __file__ ) )
sys.path.insert( 0, HERE )

import flows

OUT = os.path.join( HERE, 'fixtures', 'flow_watcher.json' )

SITE_PORT = 46031

THREAD = 100

PHASES = [
    flows.thread_phase( { THREAD : ( 'Blue sky thread', [ ( 1, 'anon' ), ( 2, 'sky' ), ( 3, 'anon' ) ] ) } ),
    flows.thread_phase( { THREAD : ( 'Blue sky thread', [ ( 1, 'anon' ), ( 2, 'sky' ), ( 3, 'anon' ), ( 4, 'late' ) ] ) } ),
]


def record( session, site ):

    from hydrus.core import HydrusTime
    from hydrus.client import ClientConstants as CC

    controller = session.controller
    gui = controller.gui
    normalise = flows.normaliser( site )

    domain_manager = controller.CallBlockingToQt( gui, lambda: flows.install( controller, site, flows.thread_definitions ) )

    steps = []

    def qt( f ):

        return controller.CallBlockingToQt( gui, f )


    page = qt( lambda: gui._notebook.NewPageImportMultipleWatcher() )
    sidebar = page.GetSidebar()
    multiple = sidebar._multiple_watcher_import

    def media_panel():

        return page.GetMediaResultsPanel()


    def watcher():

        watchers = multiple.GetWatchers()

        return watchers[0] if len( watchers ) > 0 else None


    def checked( n ):
        """The watcher has made `n` checks, all done, and fetched what they found."""

        w = watcher()

        if w is None or sidebar._highlighted_watcher is None:

            return False


        log = w.GetGallerySeedLog()
        cache = w.GetFileSeedCache()

        shown = qt( lambda: len( media_panel().GetSortedMedia() ) )

        return log.GetGallerySeedCount() == n and log.GetGallerySeedCount( CC.STATUS_UNKNOWN ) == 0 and cache.GetFileSeedCount( CC.STATUS_UNKNOWN ) == 0 and not w.CurrentlyWorking() and shown == len( flows.hashes_in_log( flows.file_log_state( cache, normalise ) ) )


    def texts():
        """What the user reads: the watcher's row and the list's totals, and
        the highlighted watcher's box, after the UI has updated."""

        w = watcher()
        panel = sidebar._highlighted_watcher_panel

        # (the list's totals come from a cached status that is refreshed as
        # the file logs change, a moment after; wait for it to catch up)
        count = len( w.GetFileSeedCache().GetFileSeeds() )
        deadline = time.time() + 15

        while True:

            multiple._SetDirty()
            sidebar._UpdateImportStatusNow()

            if sidebar._watchers_status_st_top.text() == f'1 watchers - {count}/{count}' or time.time() > deadline:

                break


            time.sleep( 0.2 )


        panel._UpdateStatus()
        panel._file_seed_cache_control._Update()
        panel._gallery_seed_log_control._Update()

        # ("just added" shows for fifteen seconds, then the next check: a slow
        # machine passes it, so it is masked like the other times)
        row = [ normalise( t ) for t in sidebar._ConvertDataToDisplayTuple( w ) ]
        row = [ 'DELTA' if t == 'just added' else t for t in row ]

        return {
            'row' : row,
            'top' : normalise( sidebar._watchers_status_st_top.text() ),
            'bottom' : normalise( sidebar._watchers_status_st_bottom.text() ),
            'subject' : normalise( panel._watcher_subject.text() ),
            'url' : normalise( panel._watcher_url.text() ),
            'files status' : normalise( panel._file_status.text() ),
            'file log summary' : normalise( panel._file_seed_cache_control._import_summary_st.text() ),
            'file log progress' : normalise( panel._file_seed_cache_control._progress_st.text() ),
            'velocity' : normalise( panel._file_velocity_status.text() ),
            'checker status' : normalise( panel._watcher_status.text() ),
            'check log summary' : normalise( panel._gallery_seed_log_control._log_summary_st.text() ),
            'check now enabled' : panel._check_now_button.isEnabled(),
            'checking pause enabled' : panel._checking_pause_button.isEnabled(),
        }


    def timing():

        w = watcher()

        ( files_status, files_paused, velocity, next_check_time, watcher_status, subject, checking_status, check_now, checking_paused ) = w.GetStatus()

        return {
            'checked' : w._last_check_time > 0,
            'next check in the future' : next_check_time is not None and next_check_time > HydrusTime.GetNow(),
            'checking status' : checking_status,
            'checking paused' : checking_paused,
            'files paused' : files_paused,
            'check now' : check_now,
            'dead' : w.IsDead(),
            'subject' : subject,
        }


    def step( name ):

        w = watcher()

        files = flows.file_log_state( w.GetFileSeedCache(), normalise )

        steps.append( {
            'step' : name,
            'dialog' : {},
            'page' : {
                'files' : files,
                'checks' : [ flows.gallery_seed_state( g, normalise ) for g in w.GetGallerySeedLog().GetGallerySeeds() ],
                'timing' : timing(),
                'texts' : qt( texts ),
                'thumbnails' : qt( lambda: len( media_panel().GetSortedMedia() ) ),
            },
            'files' : flows.files_state( session, flows.hashes_in_log( files ), normalise ),
            'popups' : flows.popups_shown( controller, normalise ),
            'hits' : site.take_hits(),
        } )


    # 1. watch -----------------------------------------------------------

    def enter_url():

        sidebar._watcher_url_input.SetValue( site.base + f'/thread/{THREAD}' )
        sidebar._watcher_url_input.EnterText()


    with flows.scripted_dialogs( {} ) as dialogs:

        qt( enter_url )

        flows.wait_until_idle( controller, also = lambda: dialogs.check() and checked( 1 ) )


    step( 'watch' )

    # 2. check again -----------------------------------------------------

    site.phase = 1

    with flows.scripted_dialogs( {} ) as dialogs:

        qt( lambda: sidebar._highlighted_watcher_panel.EventCheckNow() )

        flows.wait_until_idle( controller, also = lambda: dialogs.check() and checked( 2 ) )


    step( 'check again' )

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

#!/usr/bin/env python3
"""Record what setting a watcher's checker options does in the reference.

A watcher page has two "checker options" buttons: the page's, under its
URL box, which sets the checker options its new watchers get
(`MultipleWatcherImport.SetCheckerOptions`, given to each watcher
`AddURL` makes); and the highlighted watcher's, which sets that
watcher's (`WatcherImport.SetCheckerOptions`): if they differ from its
own, it times its next check again (`_UpdateNextCheckTime`, which may
find the thread dead and pause checking) and words its file velocity
again; if not, nothing changes.

In the running client, on the `basic` fixture, with the time held at
`now`, this records:

* `watchers`: watchers made with files found at given times (seconds
  before `now`; their source times, or none, then their creation times),
  a last check time, and checker options, and some checking now, or
  waiting on an error, or dead; each with its state before (as made, its
  next check time set by `_UpdateNextCheckTime`) and after its checker
  options are set to others (or the same): its checker options, next
  check time, checking status (0 ok, 1 dead, 2 404) and whether checking
  is paused, and its file velocity status;
* `page`: a page's checker options set, then a URL added: the new
  watcher's checker options, and the earlier watcher's (unchanged).

Checker options are `[intended files per check, never faster than,
never slower than, [files, seconds]]` (as `ToTuple`).

Usage: QT_QPA_PLATFORM=offscreen python oracle/record_watcher_checker.py
       (writes fixtures/watcher_checker.json)
"""

import json
import os
import sys

HERE = os.path.dirname( os.path.abspath( __file__ ) )

sys.path.insert( 0, HERE )

OUT = os.path.join( HERE, 'fixtures', 'watcher_checker.json' )

NOW = 1_700_000_000
DAY = 86400

THREAD = ( 4, 300, DAY, ( 1, 3 * DAY ) )
SLOW_THREAD = ( 1, 4 * 3600, 7 * DAY, ( 1, 30 * DAY ) )
NEVER_DIES = ( 4, 300, DAY, ( 0, 3 * DAY ) )
STATIC = ( 4, 2 * 3600, 2 * 3600, ( 1, 3 * DAY ) )

# ( name, [ ( seconds ago its source time or None, seconds ago made ) ],
# last checked seconds ago, checker options, then those set, and extra
# state: check_now, no_work_until (seconds from now), status )
WATCHERS = [
    ( 'busy, to slow thread', [ ( 3600 * i, 3600 * i ) for i in range( 1, 11 ) ], 60, THREAD, SLOW_THREAD, {} ),
    ( 'busy, to static', [ ( 3600 * i, 3600 * i ) for i in range( 1, 11 ) ], 60, THREAD, STATIC, {} ),
    ( 'the same again', [ ( 3600 * i, 3600 * i ) for i in range( 1, 11 ) ], 60, THREAD, THREAD, {} ),
    ( 'quiet, now dies', [ ( 10 * DAY, 10 * DAY ) ], 60, NEVER_DIES, THREAD, {} ),
    ( 'quiet, slow thread lives', [ ( 10 * DAY, 10 * DAY ) ], 60, NEVER_DIES, SLOW_THREAD, {} ),
    ( 'no source times', [ ( None, 3600 * i ) for i in range( 1, 6 ) ], 600, THREAD, SLOW_THREAD, {} ),
    ( 'checking now', [ ( 3600, 3600 ) ], 60, THREAD, SLOW_THREAD, { 'check_now' : True } ),
    ( 'waiting on an error', [ ( 3600, 3600 ) ], 60, THREAD, SLOW_THREAD, { 'no_work_until' : 1800 } ),
    ( 'dead already', [ ( 3600, 3600 ) ], 60, THREAD, SLOW_THREAD, { 'status' : 1 } ),
    ( 'never checked', [], None, THREAD, SLOW_THREAD, {} ),
]


def checker( t ):

    from hydrus.client.importing.options import CheckerImportOptions

    ( intended, faster, slower, death ) = t

    return CheckerImportOptions.CheckerOptions( intended_files_per_check = intended, never_faster_than = faster, never_slower_than = slower, death_file_velocity = death )


def as_list( checker_options ):

    ( intended, faster, slower, ( files, seconds ) ) = checker_options.ToTuple()

    return [ intended, faster, slower, [ files, seconds ] ]


def state( watcher ):

    return {
        'checker' : as_list( watcher._checker_options ),
        'next_check_time' : watcher._next_check_time,
        'status' : watcher._checking_status,
        'checking_paused' : watcher._checking_paused,
        'file_velocity_status' : watcher._file_velocity_status,
    }


def record( session ):

    controller = session.controller
    gui = controller.gui

    def f():

        from hydrus.core import HydrusTime

        from hydrus.client.importing import ClientImportFileSeeds
        from hydrus.client.importing import ClientImportWatchers

        get_now = HydrusTime.GetNow

        HydrusTime.GetNow = lambda: NOW

        try:

            watchers = []

            for ( name, seeds, last_checked, before, after, extra ) in WATCHERS:

                watcher = ClientImportWatchers.WatcherImport()

                watcher._url = 'https://example.com/thread/1'

                cache = ClientImportFileSeeds.FileSeedCache()

                file_seeds = []

                for ( i, ( source_ago, made_ago ) ) in enumerate( seeds ):

                    file_seed = ClientImportFileSeeds.FileSeed( ClientImportFileSeeds.FILE_SEED_TYPE_URL, f'https://example.com/file/{i}' )

                    file_seed.created = NOW - made_ago
                    file_seed.modified = file_seed.created
                    file_seed.source_time = None if source_ago is None else NOW - source_ago

                    file_seeds.append( file_seed )


                cache.AddFileSeeds( file_seeds )

                watcher._file_seed_cache = cache

                watcher._last_check_time = 0 if last_checked is None else NOW - last_checked
                watcher._checker_options = checker( before )
                watcher._check_now = extra.get( 'check_now', False )
                watcher._no_work_until = NOW + extra[ 'no_work_until' ] if 'no_work_until' in extra else 0
                watcher._checking_status = extra.get( 'status', 0 )
                watcher._checking_paused = watcher._checking_status != 0

                with watcher._lock:

                    watcher._UpdateNextCheckTime()
                    watcher._UpdateFileVelocityStatus()


                made = state( watcher )

                watcher.SetCheckerOptions( checker( after ) )

                watchers.append( {
                    'name' : name,
                    'seeds' : [ [ s, m ] for ( s, m ) in seeds ],
                    'last_check_time' : watcher._last_check_time,
                    'extra' : extra,
                    'before' : made,
                    'set' : as_list( checker( after ) ),
                    'after' : state( watcher ),
                } )


            # a page: its checker options set, then a URL added
            page = ClientImportWatchers.MultipleWatcherImport()

            first = page.AddURL( 'https://example.com/thread/1' )

            page.SetCheckerOptions( checker( SLOW_THREAD ) )

            second = page.AddURL( 'https://example.com/thread/2' )

            result_page = {
                'default' : as_list( controller.new_options.GetDefaultWatcherCheckerOptions() ),
                'set' : as_list( checker( SLOW_THREAD ) ),
                'first' : as_list( first.GetCheckerOptions() ),
                'second' : as_list( second.GetCheckerOptions() ),
            }

        finally:

            HydrusTime.GetNow = get_now


        return { 'now' : NOW, 'watchers' : watchers, 'page' : result_page }


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

        path = os.path.join( work, 'watcher_checker.json' )

        hydrus_driver.run_in_subprocess( os.path.abspath( __file__ ), '--child', path )

        with open( path ) as f:

            result = json.load( f )



    with open( OUT, 'w' ) as f:

        json.dump( result, f, indent = 1, ensure_ascii = False )
        f.write( '\n' )


    print( f'wrote {OUT}: {len( result[ "watchers" ] )} watchers' )


if __name__ == '__main__':

    main()

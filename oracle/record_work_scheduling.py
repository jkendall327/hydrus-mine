#!/usr/bin/env python3
"""Record when the reference's two duplicates daemons are allowed to work.

In the running client, on the `basic` fixture, the real
`PotentialDuplicatesMaintenanceManager` (the similar-files search) and
`DuplicatesAutoResolutionManager` are asked, for every combination of the
idle state, the "during idle" and "during normal time" options, whether it is
a good time to start background work and (for the search) whether the user
has pressed "work hard", whether they would work (`_WorkPermitted`,
`_AbleToWork`), for how long (`_GetWorkPeriod`) and how long they would rest
after a pass that took 0.05 s (`_GetRestTime`). The idle state is scripted
(`controller.CurrentlyIdle`); the options are set on the real `new_options`.

Usage: QT_QPA_PLATFORM=offscreen python oracle/record_work_scheduling.py
       (writes fixtures/work_scheduling.json)
"""

import itertools
import json
import os
import sys

HERE = os.path.dirname( os.path.abspath( __file__ ) )

sys.path.insert( 0, HERE )

OUT = os.path.join( HERE, 'fixtures', 'work_scheduling.json' )

TOOK = 0.05


def record( session ):

    controller = session.controller

    options = controller.new_options

    result = {}

    def work():

        search = controller.potential_duplicates_manager
        auto = controller.duplicates_auto_resolution_manager

        search_cases = []

        for ( idle, during_idle, during_active, work_hard ) in itertools.product( ( False, True ), repeat = 4 ):

            controller.CurrentlyIdle = lambda idle = idle: idle

            options.SetBoolean( 'maintain_similar_files_duplicate_pairs_during_idle', during_idle )
            options.SetBoolean( 'maintain_similar_files_duplicate_pairs_during_active', during_active )
            options.SetInteger( 'potential_duplicates_search_work_time_ms_idle', 5000 )
            options.SetInteger( 'potential_duplicates_search_work_time_ms_active', 100 )
            options.SetInteger( 'potential_duplicates_search_rest_percentage_idle', 50 )
            options.SetInteger( 'potential_duplicates_search_rest_percentage_active', 1900 )

            search.SetWorkHard( work_hard )

            period = search._GetWorkPeriod()

            search_cases.append( {
                'idle': idle,
                'during_idle': during_idle,
                'during_active': during_active,
                'work_hard': work_hard,
                'permitted': search._WorkPermitted(),
                'work_period': period,
                'rest': search._GetRestTime( period, TOOK ),
            } )

        search.SetWorkHard( False )

        auto_cases = []

        for ( idle, good_time, during_idle, during_active, work_hard ) in itertools.product( ( False, True ), repeat = 5 ):

            controller.CurrentlyIdle = lambda idle = idle: idle
            controller.GoodTimeToStartBackgroundWork = lambda good_time = good_time: good_time

            options.SetBoolean( 'duplicates_auto_resolution_during_idle', during_idle )
            options.SetBoolean( 'duplicates_auto_resolution_during_active', during_active )
            options.SetInteger( 'duplicates_auto_resolution_work_time_ms_idle', 1000 )
            options.SetInteger( 'duplicates_auto_resolution_work_time_ms_active', 100 )
            options.SetInteger( 'duplicates_auto_resolution_rest_percentage_idle', 100 )
            options.SetInteger( 'duplicates_auto_resolution_rest_percentage_active', 900 )

            auto._working_hard_rules = set( [ 'a rule' ] ) if work_hard else set()

            period = auto._GetWorkPeriod()

            auto_cases.append( {
                'idle': idle,
                'good_time': good_time,
                'during_idle': during_idle,
                'during_active': during_active,
                'work_hard': work_hard,
                'able': auto._AbleToWork(),
                'work_period': period,
                'rest_if_work_left': auto._GetRestTime( period, TOOK, True ),
            } )

        auto._working_hard_rules = set()

        result[ 'took' ] = TOOK
        result[ 'search' ] = search_cases
        result[ 'auto_resolution' ] = auto_cases

    controller.CallBlockingToQt( controller.gui, work )

    return result


def child( out ):

    import hydrus_driver
    import record_api

    db_dir = record_api.unpack_fixture( 'basic' )

    result = hydrus_driver.run_client( db_dir, record )

    with open( out, 'w' ) as f:

        json.dump( result, f )


def main():

    if len( sys.argv ) > 1 and sys.argv[ 1 ] == '--child':

        child( sys.argv[ 2 ] )

        return

    import tempfile

    import hydrus_driver

    with tempfile.TemporaryDirectory() as work:

        path = os.path.join( work, 'work_scheduling.json' )

        hydrus_driver.run_in_subprocess( os.path.abspath( __file__ ), '--child', path )

        with open( path ) as f:

            result = json.load( f )

    with open( OUT, 'w' ) as f:

        json.dump( result, f, indent = 1 )
        f.write( '\n' )

    print( f'wrote {OUT}: {len( result[ "search" ] )} + {len( result[ "auto_resolution" ] )} cases' )


if __name__ == '__main__':

    main()

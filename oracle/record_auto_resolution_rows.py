#!/usr/bin/env python3
"""Record the reference duplicates page's auto-resolution tab's rule rows.

The tab (`ReviewDuplicatesAutoResolutionPanel`) lists the rules: name,
progress (`DuplicatesAutoResolutionRule.GetSearchSummary`: "5 to search,
2 still to test, 1 ready to resolve, 3 pairs resolved (1 failed the
test)") and status (the manager's `GetRunningStatus`: "paused", "working",
"waiting", "queued", "done"). For each of `CASES` (a rule's pair counts by
status, whether it is paused, semi-automatic with its most pending pairs,
and whether the manager may work now), this makes a rule with those
counts and records its row, with the column titles.

Statuses are `DUPLICATE_STATUS_*`: 0 did not match the search, 1 matches
but not tested, 2 failed the test, 3 actioned, 4 not searched, 5 ready to
action, 6 denied.

Usage: QT_QPA_PLATFORM=offscreen python oracle/record_auto_resolution_rows.py
       (writes fixtures/auto_resolution_rows.json)
"""

import json
import os
import sys

HERE = os.path.dirname( os.path.abspath( __file__ ) )

sys.path.insert( 0, HERE )

OUT = os.path.join( HERE, 'fixtures', 'auto_resolution_rows.json' )

# ( counts by status, paused, semi-automatic's most pending pairs (None:
# fully automatic; 0: semi-automatic with no limit), able to work now )
CASES = [
    ( {}, False, None, True ),
    ( { 4 : 5 }, False, None, True ),
    ( { 4 : 5 }, False, None, False ),
    ( { 4 : 5 }, True, None, True ),
    ( { 1 : 2, 3 : 1200 }, False, None, True ),
    ( { 3 : 3, 2 : 1, 6 : 2, 0 : 7 }, False, None, True ),
    ( { 5 : 4 }, False, 0, True ),
    ( { 5 : 4, 1 : 3 }, False, 4, True ),
    ( { 5 : 3, 1 : 3 }, False, 4, False ),
    ( { 4 : 1, 1 : 1, 5 : 1, 3 : 1, 2 : 1, 6 : 1, 0 : 1 }, False, 0, True ),
    ( { 0 : 12000 }, False, None, True ),
]


def record( session ):

    controller = session.controller
    gui = controller.gui

    def f():

        import collections

        from hydrus.client.duplicates import ClientDuplicatesAutoResolution as AR
        from hydrus.client.gui.lists import ClientGUIListConstants as CGLC

        manager = controller.duplicates_auto_resolution_manager

        able = manager._AbleToWorkIdleNormal

        rows = []

        try:

            for ( counts, paused, max_pending, can_work ) in CASES:

                rule = AR.DuplicatesAutoResolutionRule( 'a rule' )

                if max_pending is None:

                    rule.SetOperationMode( AR.DUPLICATES_AUTO_RESOLUTION_RULE_OPERATION_MODE_FULLY_AUTOMATIC )

                else:

                    rule.SetOperationMode( AR.DUPLICATES_AUTO_RESOLUTION_RULE_OPERATION_MODE_WORK_BUT_NO_ACTION )
                    rule.SetMaxPendingPairs( None if max_pending == 0 else max_pending )


                rule.SetPaused( paused )

                rule.SetCountsCache( collections.Counter( counts ) )

                manager._AbleToWorkIdleNormal = lambda: can_work

                rows.append( {
                    'counts' : counts,
                    'paused' : paused,
                    'max_pending' : max_pending,
                    'can_work' : can_work,
                    'progress' : rule.GetSearchSummary(),
                    'status' : manager.GetRunningStatus( rule ),
                } )


        finally:

            manager._AbleToWorkIdleNormal = able


        list_id = CGLC.COLUMN_LIST_REVIEW_DUPLICATES_AUTO_RESOLUTION_RULES.ID

        columns = [ CGLC.column_list_column_name_lookup[ list_id ][ i ] for i in range( 3 ) ]

        return { 'columns' : columns, 'rows' : rows }


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

        path = os.path.join( work, 'auto_resolution_rows.json' )

        hydrus_driver.run_in_subprocess( os.path.abspath( __file__ ), '--child', path )

        with open( path ) as f:

            result = json.load( f )



    with open( OUT, 'w' ) as f:

        json.dump( result, f, indent = 1, ensure_ascii = False )
        f.write( '\n' )


    print( f'wrote {OUT}: {len( result[ "rows" ] )} rows' )


if __name__ == '__main__':

    main()

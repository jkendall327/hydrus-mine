#!/usr/bin/env python3
"""Record the reference duplicates page's preparation tab (`PreparationPanel`).

The tab shows how far the similar files search has got: how many files
are eligible, how many have been searched at the search distance (with a
gauge), whether its "work harder" button can be pressed, and the tab's
name ("preparation (55.0% done)"). This replaces the maintenance numbers
store's `GetMaintenanceNumbers` with each of `CASES` (files searched by
distance, -1 for not yet) and sets the search distance and the
`hide_duplicates_needs_work_message_when_reasonably_caught_up` option,
then records what the tab says, as `_InitialiseMaintenanceStatusUpdater`'s
publisher writes it; and the search distance button's label for each
distance from 0 to 10.

Usage: QT_QPA_PLATFORM=offscreen python oracle/record_duplicates_preparation.py
       (writes fixtures/duplicates_preparation.json)
"""

import json
import os
import sys

HERE = os.path.dirname( os.path.abspath( __file__ ) )

sys.path.insert( 0, HERE )

OUT = os.path.join( HERE, 'fixtures', 'duplicates_preparation.json' )

# ( files searched by distance (-1 for not yet, as the reference stores
# it), search distance,
# hide the message when caught up )
CASES = [
    ( {}, 8, False ),
    ( { -1 : 10 }, 8, False ),
    ( { -1 : 4, 8 : 6 }, 8, False ),
    ( { -1 : 4, 4 : 6 }, 8, False ),
    ( { 4 : 3, 8 : 1, 10 : 2 }, 8, False ),
    ( { 8 : 1200 }, 8, False ),
    ( { -1 : 1, 8 : 1999 }, 8, False ),
    ( { -1 : 1, 8 : 1999 }, 8, True ),
    ( { -1 : 1, 8 : 99 }, 8, True ),
    ( { -1 : 1, 8 : 99 }, 8, False ),
    ( { -1 : 3, 0 : 7 }, 0, False ),
    ( { -1 : 1, 0 : 2 }, 0, False ),
    ( { -1 : 12345, 2 : 678 }, 2, False ),
]


def record( session ):

    controller = session.controller
    gui = controller.gui

    def f():

        from hydrus.client import ClientConstants as CC
        from hydrus.client.duplicates import ClientPotentialDuplicatesManager
        from hydrus.client.gui.pages import ClientGUISidebarDuplicates

        store = ClientPotentialDuplicatesManager.PotentialDuplicatesMaintenanceNumbersStore.instance()

        get_numbers = store.GetMaintenanceNumbers

        new_options = controller.new_options

        original_distance = new_options.GetInteger( 'similar_files_duplicate_pairs_search_distance' )
        original_hide = new_options.GetBoolean( 'hide_duplicates_needs_work_message_when_reasonably_caught_up' )

        recorded = []

        try:

            for ( counts, distance, hide ) in CASES:

                numbers = dict( counts )

                store.GetMaintenanceNumbers = lambda: numbers

                new_options.SetInteger( 'similar_files_duplicate_pairs_search_distance', distance )
                new_options.SetBoolean( 'hide_duplicates_needs_work_message_when_reasonably_caught_up', hide )

                panel = ClientGUISidebarDuplicates.PreparationPanel( gui )

                names = []

                panel.pageTabNameChanged.connect( names.append )

                # the updater's work, as it does it
                publish = panel._maintenance_status_updater._publish_callable

                publish( numbers )

                gauge = panel._num_searched._gauge

                recorded.append( {
                    'counts' : counts,
                    'distance' : distance,
                    'hide' : hide,
                    'eligible' : panel._eligible_files.text(),
                    'searched' : panel._num_searched._st.text(),
                    'gauge' : list( gauge.GetValueRange() ),
                    'can_start' : panel._start_search_button.isEnabled(),
                    'page_name' : names[-1] if names else None,
                } )


            labels = {}

            for distance in range( 11 ):

                labels[ str( distance ) ] = CC.hamming_string_lookup.get( distance, 'custom' )


        finally:

            store.GetMaintenanceNumbers = get_numbers

            new_options.SetInteger( 'similar_files_duplicate_pairs_search_distance', original_distance )
            new_options.SetBoolean( 'hide_duplicates_needs_work_message_when_reasonably_caught_up', original_hide )


        return { 'cases' : recorded, 'distance_labels' : labels }


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

        path = os.path.join( work, 'duplicates_preparation.json' )

        hydrus_driver.run_in_subprocess( os.path.abspath( __file__ ), '--child', path )

        with open( path ) as f:

            result = json.load( f )



    with open( OUT, 'w' ) as f:

        json.dump( result, f, indent = 1, ensure_ascii = False )
        f.write( '\n' )


    print( f'wrote {OUT}: {len( result[ "cases" ] )} cases' )


if __name__ == '__main__':

    main()

#!/usr/bin/env python3
"""Record the reference's potential duplicate search context counter.

`EditPotentialDuplicatesSearchContextPanel` (the "potential duplicate pairs
search" box on the duplicates page and in the auto-resolution rule editor)
counts the pairs its search finds a block at a time: it fetches the search
space (every pair in the file domain), searches it in blocks off the UI
thread, and shows "initialising…", "no potential pairs in this file domain!",
"N/M pairs searched; K match…", "M pairs searched; K match" or, once it has
enough for an estimate, "M pairs; ~K match". A play/pause button stops and
resumes it, a refresh button fetches the space again, and any change to the
search restarts the count over the same space. Its cog menu holds the
starts-paused option and the two optimisations (stop to estimate; allow the
file-search-based single slow search).

This drives the real panel in the running client, on the `basic` fixture.
The database is replaced where it would be slow or random:

* the search space is scripted (`SPACES`: pairs as `( smaller media id,
  larger media id, distance )`), and the shuffling that orders it is
  switched off so the order is the script's;
* a block's search is scripted: it pops the block from the real
  `PotentialDuplicatePairsFragmentarySearch`, and a pair is a hit if its
  distance is within the search's maximum distance (it keeps the real
  search's bookkeeping: `AddHits`, `PopBlock`, the relative-error
  arithmetic);
* the block size guideline is set per scenario and blocks are released one
  at a time, so every label can be recorded between two blocks.

Each scenario is a list of steps (`do`): `show` (the panel opens and its
page is shown), `space` (the scripted fetch finishes), `block` (one block is
searched), `pause`, `play` and `refresh` (the buttons), `distance` (the spin
box), `kind` and `pixel` (the choices), `cog` (a cog item is ticked) and
`option` (an option is set directly). After each step the label, the pause
state, the count the panel holds and the signals it emitted since the last
step are recorded.

Usage: QT_QPA_PLATFORM=offscreen python oracle/record_potential_duplicates_count.py
       (writes fixtures/potential_duplicates_count.json)
"""

import json
import os
import sys
import threading
import time

HERE = os.path.dirname( os.path.abspath( __file__ ) )

sys.path.insert( 0, HERE )

OUT = os.path.join( HERE, 'fixtures', 'potential_duplicates_count.json' )

# a space is `n` pairs; pair `i` is ( 2i, 2i + 1, ( i * mult ) % modulus )
SPACES = {
    'none' : { 'n' : 0, 'mult' : 1, 'modulus' : 1 },
    'small' : { 'n' : 30, 'mult' : 7, 'modulus' : 13 },
    'large' : { 'n' : 30000, 'mult' : 7, 'modulus' : 13 },
    'tail' : { 'n' : 5000, 'mult' : 7, 'modulus' : 13 },
}

SETTLE = 0.5

OPTIONS = {
    'starts_paused' : 'potential_duplicate_pairs_search_starts_paused',
    'stops_to_estimate' : 'potential_duplicate_pairs_search_context_panel_stops_to_estimate',
    'file_search_optimisation' : 'potential_duplicate_pairs_search_can_do_file_search_based_optimisation',
}

# ( name, space, block size guideline, options, steps )
SCENARIOS = [
    ( 'counts_to_the_end', 'small', 10, {}, [
        { 'do' : 'show' },
        { 'do' : 'space' },
        { 'do' : 'block' },
        { 'do' : 'block' },
        { 'do' : 'block' },
    ] ),
    ( 'empty_domain', 'none', 10, {}, [
        { 'do' : 'show' },
        { 'do' : 'space' },
        { 'do' : 'refresh' },
        { 'do' : 'space' },
    ] ),
    ( 'pause_play_refresh', 'small', 10, {}, [
        { 'do' : 'show' },
        { 'do' : 'space' },
        { 'do' : 'block' },
        { 'do' : 'pause' },
        { 'do' : 'block' },
        { 'do' : 'play' },
        { 'do' : 'block' },
        { 'do' : 'pause' },
        { 'do' : 'refresh' },
        { 'do' : 'space' },
        { 'do' : 'block' },
        { 'do' : 'play' },
        { 'do' : 'block' },
        { 'do' : 'block' },
        { 'do' : 'block' },
        { 'do' : 'pause' },
        { 'do' : 'play' },
        { 'do' : 'refresh' },
        { 'do' : 'space' },
    ] ),
    ( 'search_changes_restart', 'small', 10, {}, [
        { 'do' : 'show' },
        { 'do' : 'space' },
        { 'do' : 'block' },
        { 'do' : 'distance', 'value' : 2 },
        { 'do' : 'block' },
        { 'do' : 'distance', 'value' : 12 },
        { 'do' : 'block' },
        { 'do' : 'block' },
        { 'do' : 'block' },
        { 'do' : 'kind', 'value' : 1 },
        { 'do' : 'block' },
        { 'do' : 'kind', 'value' : 2 },
        { 'do' : 'block' },
        { 'do' : 'pixel', 'value' : 0 },
        { 'do' : 'block' },
        { 'do' : 'pixel', 'value' : 2 },
        { 'do' : 'block' },
    ] ),
    ( 'starts_paused', 'small', 10, { 'starts_paused' : True }, [
        { 'do' : 'show' },
        { 'do' : 'space' },
        { 'do' : 'block' },
        { 'do' : 'play' },
        { 'do' : 'block' },
        { 'do' : 'block' },
        { 'do' : 'block' },
    ] ),
    ( 'estimates', 'large', 4000, {}, [
        { 'do' : 'show' },
        { 'do' : 'space' },
        { 'do' : 'block' },
        { 'do' : 'block' },
        { 'do' : 'block' },
        { 'do' : 'block' },
        { 'do' : 'block' },
        { 'do' : 'block' },
        { 'do' : 'block' },
        { 'do' : 'cog', 'value' : 'stops_to_estimate' },
        { 'do' : 'block' },
        { 'do' : 'block' },
        { 'do' : 'cog', 'value' : 'stops_to_estimate' },
        { 'do' : 'block' },
        { 'do' : 'block' },
        { 'do' : 'block' },
        { 'do' : 'block' },
    ] ),
    ( 'estimate_off_counts_everything', 'tail', 1000, { 'stops_to_estimate' : False }, [
        { 'do' : 'show' },
        { 'do' : 'space' },
        { 'do' : 'block' },
        { 'do' : 'block' },
        { 'do' : 'block' },
        { 'do' : 'block' },
        { 'do' : 'cog', 'value' : 'stops_to_estimate' },
        { 'do' : 'block' },
    ] ),
    ( 'block_size_follows_the_time_a_block_takes', 'tail', 1000, { 'stops_to_estimate' : False }, [
        { 'do' : 'show' },
        { 'do' : 'space' },
        { 'do' : 'block' },
        { 'do' : 'block' },
        { 'do' : 'block', 'seconds' : 1.0 },
        { 'do' : 'block' },
        { 'do' : 'block', 'seconds' : 1.0 },
        { 'do' : 'block', 'seconds' : 1.0 },
        { 'do' : 'block' },
        { 'do' : 'block' },
        { 'do' : 'block' },
    ] ),
    ( 'cog_items', 'small', 10, {}, [
        { 'do' : 'show' },
        { 'do' : 'space' },
        { 'do' : 'cog', 'value' : 'starts_paused' },
        { 'do' : 'cog', 'value' : 'file_search_optimisation' },
        { 'do' : 'cog', 'value' : 'starts_paused' },
        { 'do' : 'cog', 'value' : 'file_search_optimisation' },
    ] ),
]


def space_rows( space ):

    return [ ( 2 * i, 2 * i + 1, ( i * space[ 'mult' ] ) % space[ 'modulus' ] ) for i in range( space[ 'n' ] ) ]


class Gates( object ):
    """Work waits here until the recording lets it through. A search's block
    waits at its own gate; `waiting` says whether one is there to be let
    through (a block is let through only if it waits, so no permits pile
    up)."""

    def __init__( self ):

        self.lock = threading.Lock()
        self.space = threading.Semaphore( 0 )
        self.space_waiting = 0
        self.blocks = {}
        self.period = 0.0

    def gate( self, search ):

        with self.lock:

            # (the search is kept so that its id is not used again)
            return self.blocks.setdefault( id( search ), { 'search' : search, 'sem' : threading.Semaphore( 0 ), 'waiting' : 0 } )

    def release_block( self, search, period = 0.0 ):
        """Let the block waiting for `search` go; whether there was one."""

        gate = self.gate( search )

        with self.lock:

            if gate[ 'waiting' ] == 0:

                return False

            self.period = period

        gate[ 'sem' ].release()

        return True

    def release_space( self ):

        with self.lock:

            if self.space_waiting == 0:

                return False

        self.space.release()

        return True

    def release_all( self ):

        for _ in range( 50 ):

            self.space.release()

        with self.lock:

            for gate in self.blocks.values():

                for _ in range( 50 ):

                    gate[ 'sem' ].release()


def record( session ):

    controller = session.controller
    gui = controller.gui

    from qtpy import QtWidgets as QW

    from hydrus.core import HydrusLists
    from hydrus.client import ClientGlobals as CG
    from hydrus.client.duplicates import ClientPotentialDuplicatesSearchContext as PDSC
    from hydrus.client.gui.duplicates import ClientGUIPotentialDuplicatesSearchContext as Panel

    new_options = controller.new_options

    original_options = { name : new_options.GetBoolean( key ) for ( name, key ) in OPTIONS.items() }
    original_read = controller.Read
    original_randomise = HydrusLists.RandomiseListByChunks
    original_block = PDSC.POTENTIAL_DUPLICATE_PAIRS_BLOCK_SIZE_GUIDELINE

    original_throttle = PDSC.PotentialDuplicatePairsFragmentarySearch.NotifyWorkTimeForAutothrottle

    scripted = {}

    def throttle( self, actual_work_period, ideal_work_period ):

        # the time a block took is the script's (a block waits for the recording)
        return original_throttle( self, scripted[ 'gates' ].period, ideal_work_period )

    PDSC.PotentialDuplicatePairsFragmentarySearch.NotifyWorkTimeForAutothrottle = throttle

    # the order the script gives is the order searched
    HydrusLists.RandomiseListByChunks = lambda items, chunk: list( items )

    recorded = []

    try:

        for ( name, space_name, block_size, options, steps ) in SCENARIOS:

            space = SPACES[ space_name ]
            rows = space_rows( space )

            gates = Gates()
            scripted[ 'gates' ] = gates

            def read( action, *args, **kwargs ):

                if action == 'potential_duplicate_id_pairs_and_distances':

                    with gates.lock:

                        gates.space_waiting += 1

                    gates.space.acquire()

                    with gates.lock:

                        gates.space_waiting -= 1

                    return PDSC.PotentialDuplicateIdPairsAndDistances( rows )

                if action == 'potential_duplicates_count_fragmentary':

                    search = args[ 0 ]

                    gate = gates.gate( search )

                    with gates.lock:

                        gate[ 'waiting' ] += 1

                    gate[ 'sem' ].acquire()

                    with gates.lock:

                        gate[ 'waiting' ] -= 1

                    block = search.PopBlock().GetRows()

                    context = search.GetPotentialDuplicatesSearchContext()

                    hits = [ row for row in block if row[ 2 ] <= context.GetMaxHammingDistance() ]

                    search.AddHits( hits )

                    return len( hits )

                return original_read( action, *args, **kwargs )

            controller.Read = read

            PDSC.POTENTIAL_DUPLICATE_PAIRS_BLOCK_SIZE_GUIDELINE = block_size

            for ( option_name, key ) in OPTIONS.items():

                new_options.SetBoolean( key, options.get( option_name, original_options[ option_name ] if option_name != 'starts_paused' else False ) )

            state = {}

            def make_panel():

                context = PDSC.PotentialDuplicatesSearchContext()

                panel = Panel.EditPotentialDuplicatesSearchContextPanel( gui, context, synchronised = False )

                signals = { 'restarted' : 0, 'has_pairs' : 0, 'no_pairs' : 0, 'changed' : 0 }

                panel.restartedSearch.connect( lambda: signals.__setitem__( 'restarted', signals[ 'restarted' ] + 1 ) )
                panel.thisSearchHasPairs.connect( lambda: signals.__setitem__( 'has_pairs', signals[ 'has_pairs' ] + 1 ) )
                panel.thisSearchDefinitelyHasNoPairs.connect( lambda: signals.__setitem__( 'no_pairs', signals[ 'no_pairs' ] + 1 ) )
                panel.valueChanged.connect( lambda: signals.__setitem__( 'changed', signals[ 'changed' ] + 1 ) )

                panel.show()
                panel.PageShown()

                state[ 'panel' ] = panel
                state[ 'signals' ] = signals

            def observe():

                panel = state[ 'panel' ]

                search = panel._potential_duplicate_pairs_fragmentary_search

                signals = dict( state[ 'signals' ] )

                for key in state[ 'signals' ]:

                    state[ 'signals' ][ key ] = 0

                return {
                    'label' : panel._num_potential_duplicate_pairs_label.text(),
                    'has_tooltip' : panel._num_potential_duplicate_pairs_label.toolTip() != '',
                    'paused' : panel._count_paused,
                    'matches' : panel._num_potential_duplicate_pairs,
                    'searched' : search.NumPairsSearched(),
                    'in_space' : search.NumPairsInSearchSpace(),
                    'distance' : panel._max_hamming_distance.value(),
                    'distance_enabled' : panel._max_hamming_distance.isEnabled(),
                    'second_search_shown' : not panel._tag_autocomplete_2.isHidden(),
                    'signals' : signals,
                }

            recorded_steps = []

            def current_search():

                return state[ 'panel' ]._potential_duplicate_pairs_fragmentary_search

            for step in steps:

                do = step[ 'do' ]

                released = None

                if do == 'show':

                    controller.CallBlockingToQt( gui, make_panel )

                elif do == 'space':

                    released = gates.release_space()

                elif do == 'block':

                    search = controller.CallBlockingToQt( gui, current_search )

                    released = gates.release_block( search, step.get( 'seconds', 0.0 ) )

                else:

                    dropped = controller.CallBlockingToQt( gui, current_search )

                    if do == 'pause' or do == 'play':

                        controller.CallBlockingToQt( gui, state[ 'panel' ]._pause_count_button.click )

                    elif do == 'refresh':

                        controller.CallBlockingToQt( gui, state[ 'panel' ]._refresh_dupe_counts_button.click )

                    elif do == 'distance':

                        controller.CallBlockingToQt( gui, state[ 'panel' ]._max_hamming_distance.setValue, step[ 'value' ] )

                    elif do == 'kind':

                        controller.CallBlockingToQt( gui, state[ 'panel' ]._dupe_search_type.setCurrentIndex, step[ 'value' ] )

                    elif do == 'pixel':

                        controller.CallBlockingToQt( gui, state[ 'panel' ]._pixel_dupes_preference.setCurrentIndex, step[ 'value' ] )

                    elif do == 'cog':

                        def tick():

                            key = OPTIONS[ step[ 'value' ] ]

                            for item in state[ 'panel' ]._cog_button._menu_template_items:

                                manager = getattr( item, 'check_manager', None )

                                if manager is not None and getattr( manager, '_boolean_name', None ) == key:

                                    manager.Invert()

                        controller.CallBlockingToQt( gui, tick )

                    else:

                        raise Exception( 'unknown step ' + do )

                    if do in ( 'refresh', 'distance', 'kind', 'pixel' ):

                        # a block already under way finishes (for the search that is dropped)
                        time.sleep( SETTLE )

                        gates.release_block( dropped )

                time.sleep( SETTLE )

                observed = controller.CallBlockingToQt( gui, observe )

                observed[ 'step' ] = step
                observed[ 'released' ] = released

                if do == 'cog':

                    observed[ 'option' ] = new_options.GetBoolean( OPTIONS[ step[ 'value' ] ] )

                recorded_steps.append( observed )

            def menu():

                items = []

                for item in state[ 'panel' ]._cog_button._menu_template_items:

                    if type( item ).__name__ == 'MenuTemplateItemSeparator':

                        items.append( { 'separator' : True } )

                        continue

                    items.append( {
                        'title' : item.GetTitle() if hasattr( item, 'GetTitle' ) else item.title,
                        'description' : item.GetDescription() if hasattr( item, 'GetDescription' ) else item.description,
                        'option' : item.check_manager._boolean_name,
                    } )

                return items

            recorded.append( { 'name' : name, 'space' : space_name, 'block_size' : block_size, 'options' : options, 'steps' : recorded_steps, 'cog_menu' : controller.CallBlockingToQt( gui, menu ) } )

            gates.release_all()

            def close():

                state[ 'panel' ].close()
                state[ 'panel' ].deleteLater()

            controller.CallBlockingToQt( gui, close )

            time.sleep( SETTLE )

    finally:

        controller.Read = original_read
        HydrusLists.RandomiseListByChunks = original_randomise
        PDSC.PotentialDuplicatePairsFragmentarySearch.NotifyWorkTimeForAutothrottle = original_throttle
        PDSC.POTENTIAL_DUPLICATE_PAIRS_BLOCK_SIZE_GUIDELINE = original_block

        for ( name, key ) in OPTIONS.items():

            new_options.SetBoolean( key, original_options[ name ] )

    return { 'spaces' : SPACES, 'defaults' : { name : bool( v ) for ( name, v ) in original_options.items() }, 'scenarios' : recorded }


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

        path = os.path.join( work, 'potential_duplicates_count.json' )

        hydrus_driver.run_in_subprocess( os.path.abspath( __file__ ), '--child', path )

        with open( path ) as f:

            result = json.load( f )


    with open( OUT, 'w' ) as f:

        json.dump( result, f, indent = 1, ensure_ascii = False )
        f.write( '\n' )


    print( f'wrote {OUT}: {len( result[ "scenarios" ] )} scenarios' )


if __name__ == '__main__':

    main()

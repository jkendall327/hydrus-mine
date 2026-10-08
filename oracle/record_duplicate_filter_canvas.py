#!/usr/bin/env python3
"""Record the reference's duplicate filter canvas (`CanvasFilterDuplicates`)
through short batches of pairs.

On the database `record_auto_resolution.py` left (`legacy_db/auto_resolution
.tar.gz`: files with potential pairs between them), this opens the real
filter canvas on a `PotentialDuplicatePairFactoryDBMixed` (what the duplicates
page's "launch the filter" makes) and drives it with the real application
commands the shortcuts and the hover's buttons send: better (delete the other,
or keep both), the same, alternates, not related, skip, back and view the
other file. After each step it records what the canvas shows: the index
string ("A - 2/5 - 1 decisions"), which file of the pair is shown, the
decisions made so far by kind, and what the canvas then did.

At the end of a batch it asks "commit N decisions and continue?" (unless the
`duplicate_filter_auto_commit_batch_size` option lets it commit a small batch
without asking) and, on closing with decisions pending, "commit N
decisions?" with its three buttons ("forget" asks again). The dialogs are the
real ones: the recording presses their buttons and notes the question they
ask, the buttons they offer and what pressing each did. A commit is recorded
as the writes the canvas makes (`duplicate_pair_status`, `content_updates`),
passed on to the database, and afterwards each pair's files' relationships.

Time is held still. Usage: QT_QPA_PLATFORM=offscreen TZ=UTC python
oracle/record_duplicate_filter_canvas.py (writes fixtures/duplicate_filter_canvas.json)
"""

import json
import os
import sys
import time

HERE = os.path.dirname( os.path.abspath( __file__ ) )

sys.path.insert( 0, HERE )

OUT = os.path.join( HERE, 'fixtures', 'duplicate_filter_canvas.json' )

NOW = 1900000000

SETTLE = 1.0

# commands by name: the reference's application commands
COMMANDS = {
    'better_delete_other' : 'SIMPLE_DUPLICATE_FILTER_THIS_IS_BETTER_AND_DELETE_OTHER',
    'better_keep_both' : 'SIMPLE_DUPLICATE_FILTER_THIS_IS_BETTER_BUT_KEEP_BOTH',
    'same' : 'SIMPLE_DUPLICATE_FILTER_EXACTLY_THE_SAME',
    'alternates' : 'SIMPLE_DUPLICATE_FILTER_ALTERNATES',
    'false_positive' : 'SIMPLE_DUPLICATE_FILTER_FALSE_POSITIVE',
    'skip' : 'SIMPLE_DUPLICATE_FILTER_SKIP',
    'back' : 'SIMPLE_DUPLICATE_FILTER_BACK',
    'switch' : 'SIMPLE_VIEW_NEXT',
}

# Each scenario: ( name, options, steps ). A step is a command name (above)
# or a dict: `do` (a command, `close` for closing the window, `wait` for a
# moment), and `answers`, the buttons pressed in turn on the dialogs it opens.
# `cycle` steps repeat the commands in turn for as long as the canvas shows
# a pair, until it asks.
AUTO_COMMIT_OFF = { 'duplicate_filter_auto_commit_batch_size' : None, 'duplicate_filter_max_batch_size' : 6 }

SCENARIOS = [
    ( 'every_decision_then_commit', AUTO_COMMIT_OFF, [
        { 'cycle' : [ 'better_delete_other', 'better_keep_both', 'same', 'alternates', 'false_positive' ], 'answers' : [ 'commit and continue' ], 'stop_after_answers' : True },
    ] ),
    ( 'back_undoes_decisions_and_the_batch_end_can_go_back', AUTO_COMMIT_OFF, [
        'same',
        'back',
        'back',
        'switch',
        'alternates',
        'switch',
        'switch',
        'skip',
        'back',
        'false_positive',
        { 'cycle' : [ 'false_positive' ], 'answers' : [ 'go back', 'commit and continue' ], 'stop_after_answers' : True },
    ] ),
    ( 'closing_with_decisions_pending', AUTO_COMMIT_OFF, [
        'alternates',
        'false_positive',
        { 'do' : 'close', 'answers' : [ 'back to filtering' ] },
        'same',
        { 'do' : 'close', 'answers' : [ 'forget' ] },
    ] ),
    ( 'closing_commits_when_asked', AUTO_COMMIT_OFF, [
        'alternates',
        'false_positive',
        { 'do' : 'close', 'answers' : [ 'commit' ] },
    ] ),
    ( 'a_small_batch_is_committed_without_asking', { 'duplicate_filter_auto_commit_batch_size' : 1, 'duplicate_filter_max_batch_size' : 6 }, [
        { 'cycle' : [ 'alternates', 'skip' ], 'answers' : [ 'commit and continue' ], 'stop_after_answers' : True },
    ] ),
    ( 'files_merged_or_deleted_skip_their_other_pairs', AUTO_COMMIT_OFF, [
        { 'cycle' : [ 'better_delete_other' ], 'answers' : [ 'commit and continue' ], 'stop_after_answers' : True },
    ] ),
]

MAX_STEPS = 60


def record( session ):

    controller = session.controller
    gui = controller.gui

    from hydrus.core import HydrusTime

    HydrusTime.GetNow = lambda: NOW
    HydrusTime.GetNowMS = lambda: NOW * 1000
    HydrusTime.GetNowFloat = lambda: float( NOW )

    manager = controller.duplicates_auto_resolution_manager
    manager._AbleToWorkIdleNormal = lambda: False
    manager._AbleToWorkActiveNormal = lambda: False

    from qtpy import QtWidgets as QW

    from hydrus.client import ClientGlobals as CG
    from hydrus.client import ClientApplicationCommand as CAC
    from hydrus.client import ClientConstants as CC
    from hydrus.client import ClientLocation
    from hydrus.client.duplicates import ClientDuplicates
    from hydrus.client.duplicates import ClientPotentialDuplicatesPairFactory
    from hydrus.client.duplicates import ClientPotentialDuplicatesSearchContext
    from hydrus.client.gui import ClientGUIDialogsMessage
    from hydrus.client.gui import ClientGUIDialogsQuick
    from hydrus.client.gui import ClientGUITopLevelWindowsPanels
    from hydrus.client.gui.canvas import ClientGUICanvasDuplicates
    from hydrus.client.gui.canvas import ClientGUICanvasFrame

    # The mixed batch is a random sample of the matching pairs, sorted. A
    # sample cannot be replayed, so the batch is the first pairs by the sort
    # (which is what hydrus-rs fetches; docs/rust/DIFFERENCES.md): the same
    # search and sort, all the matching pairs, then the first `no_more_than`.
    def do_search_work( factory, *args ):

        search = factory._potential_duplicate_pairs_fragmentary_search

        rows = []

        while not search.SearchDone():

            found = CG.client_controller.Read( 'potential_duplicate_media_result_pairs_and_distances_fragmentary', search, no_more_than = 10 ** 9 )

            rows.extend( list( found.IterateRows() ) )

        everything = ClientPotentialDuplicatesSearchContext.PotentialDuplicateMediaResultPairsAndDistances( rows )

        everything.Sort( factory._duplicate_pair_sort_type, factory._duplicate_pair_sort_asc )

        for row in list( everything.IterateRows() )[ : factory._no_more_than ]:

            factory._fetched_media_result_pairs_and_distances.AppendRow( row )

        return True

    ClientPotentialDuplicatesPairFactory.PotentialDuplicatePairFactoryDBMixed.DoSearchWork = do_search_work

    asked = []
    answers = []

    def yes_no( win, message, **kwargs ):

        asked.append( { 'yes_no' : message } )

        return QW.QDialog.DialogCode.Accepted

    def show_information( win, message, *args, **kwargs ):

        asked.append( { 'information' : message } )

    def show_critical( win, title, message, *args, **kwargs ):

        asked.append( { 'critical' : message } )

    ClientGUIDialogsQuick.GetYesNo = yes_no
    ClientGUIDialogsMessage.ShowInformation = show_information
    ClientGUIDialogsMessage.ShowCritical = show_critical

    def exec_( dlg ):

        labels = [ w.text() for w in dlg.findChildren( QW.QLabel ) if w.text() != '' ]
        buttons = [ b.text() for b in dlg.findChildren( QW.QPushButton ) if b.text() != '' ]

        if len( answers ) == 0:

            raise Exception( 'unexpected dialog {} {}'.format( labels, buttons ) )

        wanted = answers.pop( 0 )

        asked.append( { 'texts' : labels, 'buttons' : buttons, 'pressed' : wanted } )

        for b in dlg.findChildren( QW.QPushButton ):

            if b.text() == wanted:

                b.click()

                break

        else:

            raise Exception( 'no button {} in {}'.format( wanted, buttons ) )

        return dlg.result()

    ClientGUITopLevelWindowsPanels.DialogCustomButtonQuestion.exec = exec_

    writes = []

    original_write = controller.WriteSynchronous

    def write( action, *args, **kwargs ):

        if action == 'duplicate_pair_status':

            for ( duplicate_type, hash_a, hash_b, packages ) in args[ 0 ]:

                deleted = []

                for package in packages:

                    for ( service_key, content_updates ) in package.IterateContentUpdates():

                        for content_update in content_updates:

                            if content_update.GetDataType() == 0 and content_update.GetAction() == 1:

                                deleted.extend( sorted( h.hex() for h in content_update.GetHashes() ) )

                writes.append( { 'type' : duplicate_type, 'a' : hash_a.hex(), 'b' : hash_b.hex(), 'deleted' : deleted } )

        return original_write( action, *args, **kwargs )

    controller.WriteSynchronous = write

    qt = lambda f, *a: controller.CallBlockingToQt( gui, f, *a )

    state = {}

    def make( options ):

        new_options = controller.new_options

        for ( key, value ) in options.items():

            if key == 'duplicate_filter_max_batch_size':

                new_options.SetInteger( key, value )

            else:

                new_options.SetNoneableInteger( key, value )

        context = ClientPotentialDuplicatesSearchContext.PotentialDuplicatesSearchContext()
        context.SetMaxHammingDistance( 4 )

        factory = ClientPotentialDuplicatesPairFactory.PotentialDuplicatePairFactoryDBMixed( context, ClientDuplicates.DUPE_PAIR_SORT_MAX_FILESIZE, False, new_options.GetInteger( 'duplicate_filter_max_batch_size' ) )

        frame = ClientGUICanvasFrame.CanvasFrame( gui )
        canvas = ClientGUICanvasDuplicates.CanvasFilterDuplicates( frame, factory )
        frame.SetCanvas( canvas )
        frame.showNormal()
        frame.resize( 1000, 750 )

        state[ 'frame' ] = frame
        state[ 'canvas' ] = canvas

    def observe():

        canvas = state[ 'canvas' ]
        frame = state[ 'frame' ]

        media = canvas._current_media

        out = {
            'index' : canvas._GetIndexString(),
            'loading' : canvas._loading_text,
            'shown' : None if media is None else media.GetHash().hex(),
            'other' : None,
            'pair_index' : canvas._current_pair_index,
            'num_pairs' : len( canvas._batch_of_pairs_to_process ),
            'decisions' : [ type( d ).__name__.replace( 'DuplicatePairDecision', '' ) for d in canvas._duplicate_pair_decisions ],
            'open' : frame.isVisible(),
            'committing' : canvas._CurrentlyCommitting(),
        }

        if media is not None and len( canvas._media_list ) > 1:

            out[ 'other' ] = canvas._media_list.GetNext( media ).GetHash().hex()

        return out

    sizes = {}

    def batch():

        for ( a, b ) in state[ 'canvas' ]._batch_of_pairs_to_process:

            sizes[ a.GetHash().hex() ] = a.GetSize()
            sizes[ b.GetHash().hex() ] = b.GetSize()

        return [ [ a.GetHash().hex(), b.GetHash().hex() ] for ( a, b ) in state[ 'canvas' ]._batch_of_pairs_to_process ]

    def wait_for_pair():

        deadline = time.time() + 60

        while time.time() < deadline:

            time.sleep( 0.3 )

            now = qt( observe )

            if not now[ 'open' ] or ( now[ 'shown' ] is not None and not now[ 'committing' ] and now[ 'num_pairs' ] > 0 ):

                return now

        raise Exception( 'the filter never showed a pair' )

    def command( name ):

        action = getattr( CAC, COMMANDS[ name ] )

        def f():

            return state[ 'canvas' ].ProcessApplicationCommand( CAC.ApplicationCommand.STATICCreateSimpleCommand( action ) )

        qt( f )

    recorded = []

    original_options = { 'duplicate_filter_auto_commit_batch_size' : controller.new_options.GetNoneableInteger( 'duplicate_filter_auto_commit_batch_size' ) }

    for ( name, options, steps ) in [ SCENARIOS[ int( os.environ[ 'SCENARIO' ] ) ] ]:

        asked.clear()
        answers.clear()
        writes.clear()

        qt( make, options )

        first = wait_for_pair()

        sizes.clear()

        scenario = { 'name' : name, 'options' : options, 'batch' : batch() if first[ 'open' ] else [], 'start' : first, 'steps' : [] }

        def run_step( do, step_answers ):

            asked_before = len( asked )
            writes_before = len( writes )

            answers[ : ] = step_answers

            if do == 'close':

                ok = qt( lambda: state[ 'canvas' ].UserOKToClose() )

            else:

                command( do )

                ok = None

            time.sleep( SETTLE )

            observed = qt( observe )

            if observed[ 'committing' ]:

                observed = wait_for_pair()

            entry = { 'do' : do, 'answers' : step_answers, 'after' : observed, 'asked' : list( asked[ asked_before : ] ), 'writes' : list( writes[ writes_before : ] ) }

            if ok is not None:

                entry[ 'ok_to_close' ] = ok

            scenario[ 'steps' ].append( entry )

            return observed

        for step in steps:

            if isinstance( step, str ):

                step = { 'do' : step }

            if 'cycle' in step:

                step_answers = list( step.get( 'answers', [] ) )

                for i in range( MAX_STEPS ):

                    current = qt( observe )

                    if not current[ 'open' ] or current[ 'shown' ] is None:

                        break

                    observed = run_step( step[ 'cycle' ][ i % len( step[ 'cycle' ] ) ], list( step_answers ) )

                    # (only a dialog uses an answer: the batch's end)
                    step_answers = step_answers[ len( [ a for a in scenario[ 'steps' ][ -1 ][ 'asked' ] if 'pressed' in a ] ) : ]

                    if len( step_answers ) == 0 and step.get( 'stop_after_answers' ):

                        break

                    if not observed[ 'open' ]:

                        break

            else:

                run_step( step[ 'do' ], list( step.get( 'answers', [] ) ) )

        time.sleep( SETTLE )

        hashes = sorted( { h for pair in scenario[ 'batch' ] for h in pair } )

        scenario[ 'sizes' ] = sizes

        if len( hashes ) > 0:

            location = ClientLocation.LocationContext.STATICCreateSimple( CC.COMBINED_LOCAL_FILE_DOMAINS_SERVICE_KEY )

            relationships = controller.Read( 'file_relationships_for_api', location, [ bytes.fromhex( h ) for h in hashes ] )

            scenario[ 'relationships' ] = { ( h.hex() if isinstance( h, bytes ) else h ) : r for ( h, r ) in relationships.items() }

        scenario[ 'unused_answers' ] = list( answers )

        recorded.append( scenario )

        def close():

            frame = state[ 'frame' ]

            frame.deleteLater()

        qt( close )

        time.sleep( SETTLE )

    for ( key, value ) in original_options.items():

        controller.new_options.SetNoneableInteger( key, value )

    return { 'scenarios' : recorded }


def child( out ):

    import hydrus_driver
    import record_api

    db_dir = record_api.unpack_fixture( 'auto_resolution' )

    result = hydrus_driver.run_client( db_dir, record )

    with open( out, 'w' ) as f:

        json.dump( result, f, ensure_ascii = False )


def main():

    if len( sys.argv ) > 1 and sys.argv[ 1 ] == '--child':

        child( sys.argv[ 2 ] )

        return


    import tempfile

    import hydrus_driver

    merged = { 'scenarios' : [] }

    # (each scenario on its own copy of the database: commits change it)
    for i in range( len( SCENARIOS ) ):

        with tempfile.TemporaryDirectory() as work:

            path = os.path.join( work, 'duplicate_filter_canvas.json' )

            os.environ[ 'SCENARIO' ] = str( i )

            hydrus_driver.run_in_subprocess( os.path.abspath( __file__ ), '--child', path )

            with open( path ) as f:

                merged[ 'scenarios' ].extend( json.load( f )[ 'scenarios' ] )


    result = merged


    with open( OUT, 'w' ) as f:

        json.dump( result, f, indent = 1, ensure_ascii = False )
        f.write( '\n' )


    print( f'wrote {OUT}' )


if __name__ == '__main__':

    main()

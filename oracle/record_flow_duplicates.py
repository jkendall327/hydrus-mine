#!/usr/bin/env python3
"""Flow 5: decide a few pairs in the duplicate filter.

On the database `record_auto_resolution.py` left (`legacy_db/auto_resolution
.tar.gz`: files with potential pairs between them), with the filter's batch
size set to six pairs and no auto-commit:

1. "launch": a duplicates page (`NewPageDuplicateFilter`) and its sidebar's
   "launch the filter" (`_LaunchFilter`): the canvas shows the first pair of
   the batch.
2. "decide": the first pair is "this is better, delete the other", the
   second "same quality", the third "alternates" (the real application
   commands the shortcuts and the hover buttons send); after each, which
   pair the canvas shows.
3. "close": closing the window with three decisions pending asks to commit
   them; "commit".

After each step: what the canvas showed, the questions it asked, and the
files of the batch (their tags by local tag service, URLs, inbox state and
the local file domains they are in) and each one's duplicate relationships
(its king, its group, potentials, false positives, alternates), and the
popups.

Usage: QT_QPA_PLATFORM=offscreen TZ=UTC ~/pyenv/bin/python oracle/record_flow_duplicates.py
       (writes oracle/fixtures/flow_duplicates.json)
"""

import os
import sys
import time

HERE = os.path.dirname( os.path.abspath( __file__ ) )
sys.path.insert( 0, HERE )

import flows

OUT = os.path.join( HERE, 'fixtures', 'flow_duplicates.json' )

# (the flow has no site of its own, but every flow has a port)
SITE_PORT = 46015

PHASES = [ flows.booru_phase( {} ) ]

BATCH = 6

DECISIONS = [ 'better_delete_other', 'same', 'alternates' ]

COMMANDS = {
    'better_delete_other' : 'SIMPLE_DUPLICATE_FILTER_THIS_IS_BETTER_AND_DELETE_OTHER',
    'same' : 'SIMPLE_DUPLICATE_FILTER_EXACTLY_THE_SAME',
    'alternates' : 'SIMPLE_DUPLICATE_FILTER_ALTERNATES',
}


def record( session, site ):

    controller = session.controller
    gui = controller.gui
    normalise = flows.normaliser( site )

    from qtpy import QtWidgets as QW

    from hydrus.core import HydrusConstants as HC
    from hydrus.client import ClientApplicationCommand as CAC
    from hydrus.client import ClientConstants as CC
    from hydrus.client import ClientLocation
    from hydrus.client.duplicates import ClientPotentialDuplicatesPairFactory
    from hydrus.client.duplicates import ClientPotentialDuplicatesSearchContext
    from hydrus.client.gui import ClientGUIDialogsMessage
    from hydrus.client.gui import ClientGUIDialogsQuick
    from hydrus.client.gui import ClientGUITopLevelWindowsPanels
    from hydrus.client.gui.canvas import ClientGUICanvasDuplicates

    controller.duplicates_auto_resolution_manager._AbleToWorkIdleNormal = lambda: False
    controller.duplicates_auto_resolution_manager._AbleToWorkActiveNormal = lambda: False

    # The mixed batch is a random sample of the matching pairs, sorted. A
    # sample cannot be replayed, so the batch is the first pairs by the sort
    # (which is what hydrus-rs fetches; docs/rust/DIFFERENCES.md), as
    # `record_duplicate_filter_canvas.py` does.
    import sqlite3

    from hydrus.client.gui.canvas import ClientGUICanvasDuplicates  # noqa

    master = sqlite3.connect( 'file:' + os.path.join( controller.db_dir, 'client.master.db' ) + '?mode=ro', uri = True )

    hash_ids = { bytes( h ) : i for ( i, h ) in master.execute( 'SELECT hash_id, hash FROM hashes;' ) }

    master.close()

    from hydrus.client import ClientGlobals as CG

    def do_search_work( factory, *args ):

        search = factory._potential_duplicate_pairs_fragmentary_search

        rows = []

        while not search.SearchDone():

            found = CG.client_controller.Read( 'potential_duplicate_media_result_pairs_and_distances_fragmentary', search, no_more_than = 10 ** 9 )

            rows.extend( list( found.IterateRows() ) )


        rows.sort( key = lambda row: ( hash_ids[ row[ 0 ].GetHash() ], hash_ids[ row[ 1 ].GetHash() ] ) )

        everything = ClientPotentialDuplicatesSearchContext.PotentialDuplicateMediaResultPairsAndDistances( rows )

        everything.Sort( factory._duplicate_pair_sort_type, factory._duplicate_pair_sort_asc )

        chosen = list( everything.IterateRows() )[ : factory._no_more_than ]

        chosen.sort( key = lambda row: ( hash_ids[ row[ 0 ].GetHash() ], hash_ids[ row[ 1 ].GetHash() ] ) )

        for row in chosen:

            factory._fetched_media_result_pairs_and_distances.AppendRow( row )


        return True


    ClientPotentialDuplicatesPairFactory.PotentialDuplicatePairFactoryDBMixed.DoSearchWork = do_search_work

    from hydrus.core import HydrusLists

    HydrusLists.RandomiseListByChunks = lambda items, chunk: list( items )

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

        wanted = answers.pop( 0 )

        # (its question, the buttons as a set; "-or-" between them is decoration)
        asked.append( { 'question' : labels[0], 'buttons' : sorted( buttons ), 'pressed' : wanted } )

        for b in dlg.findChildren( QW.QPushButton ):

            if b.text() == wanted:

                b.click()

                break


        else:

            raise Exception( 'no button {} in {}'.format( wanted, buttons ) )


        return dlg.result()


    ClientGUITopLevelWindowsPanels.DialogCustomButtonQuestion.exec = exec_

    qt = lambda f, *a: controller.CallBlockingToQt( gui, f, *a )

    # (the shutdown signal is handled when Python next runs on the Qt thread)
    def tick():

        from qtpy import QtCore as QC

        timer = QC.QTimer( gui )
        timer.timeout.connect( lambda: None )
        timer.start( 200 )

        state[ 'timer' ] = timer


    state = {}

    qt( tick )

    # the options a user would set in file > options > duplicates
    controller.new_options.SetInteger( 'duplicate_filter_max_batch_size', BATCH )
    controller.new_options.SetNoneableInteger( 'duplicate_filter_auto_commit_batch_size', None )

    steps = []

    batch_hashes = []

    def observe():

        canvas = state[ 'canvas' ]
        frame = state[ 'frame' ]

        media = canvas._current_media

        out = {
            'index' : canvas._GetIndexString(),
            'shown' : None if media is None else media.GetHash().hex(),
            'other' : None,
            'open' : frame.isVisible(),
        }

        if media is not None and len( canvas._media_list ) > 1:

            out[ 'other' ] = canvas._media_list.GetNext( media ).GetHash().hex()


        return out


    def wait_for_pair():

        deadline = time.time() + 60

        while time.time() < deadline:

            time.sleep( 0.3 )

            canvas = state.get( 'canvas' )

            if canvas is None:

                continue


            now = qt( lambda: ( observe(), canvas._CurrentlyCommitting(), len( canvas._batch_of_pairs_to_process ) ) )

            if not now[0][ 'open' ] or ( now[0][ 'shown' ] is not None and not now[1] and now[2] > 0 ):

                return now[0]



        raise Exception( 'the filter never showed a pair' )


    def files_and_relationships():

        hashes = sorted( batch_hashes )

        if len( hashes ) == 0:

            return { 'files' : {}, 'relationships' : {} }


        location = ClientLocation.LocationContext.STATICCreateSimple( CC.COMBINED_LOCAL_FILE_DOMAINS_SERVICE_KEY )

        relationships = controller.Read( 'file_relationships_for_api', location, [ bytes.fromhex( h ) for h in hashes ] )

        return {
            'files' : flows.files_state( session, set( hashes ), normalise ),
            # (the lists are sets: sorted)
            # (not `king_is_local` and `king_is_on_file_domain`, which the
            # reference gives for the file, not its king: a known difference
            # of the Client API, docs/rust/DIFFERENCES.md)
            'relationships' : { ( h.hex() if isinstance( h, bytes ) else h ) : { k : ( sorted( v ) if isinstance( v, list ) else v ) for ( k, v ) in r.items() if not k.startswith( 'king_is_' ) } for ( h, r ) in sorted( relationships.items(), key = lambda kv: kv[0] ) },
        }


    def step( name, seen ):

        steps.append( {
            'step' : name,
            'dialog' : seen,
            'asked' : list( asked ),
            'popups' : flows.popups_shown( controller, normalise ),
            'hits' : site.take_hits(),
        } | files_and_relationships() )

        asked.clear()


    domain_manager = qt( lambda: flows.install( controller, site ) )

    # The pairs are prepared: each file has tags of its own, and some share
    # one, so what the default merge options copy between a pair shows.
    from hydrus.client.metadata import ClientContentUpdates
    from hydrus.core import HydrusExceptions

    prepared = {}

    for ( i, h ) in enumerate( sorted( bytes( h ) for h in hash_ids ) ):

        try:

            media_result = controller.Read( 'media_result', h )

        except HydrusExceptions.DataMissing:

            continue


        if not media_result.GetLocationsManager().IsLocal():

            continue


        tags = [ 'id:' + h.hex()[ : 6 ] ]

        if i % 2 == 0:

            tags.append( 'colour:blue' )


        if i % 3 == 0:

            tags.append( 'meta:three' )


        prepared[ h.hex() ] = tags

        for tag in tags:

            update = ClientContentUpdates.ContentUpdate( HC.CONTENT_TYPE_MAPPINGS, HC.CONTENT_UPDATE_ADD, ( tag, [ h ] ) )

            controller.WriteSynchronous( 'content_updates', ClientContentUpdates.ContentUpdatePackage.STATICCreateFromContentUpdate( CC.DEFAULT_LOCAL_TAG_SERVICE_KEY, update ) )



    # --------------------------------------------------------------- 1. launch

    def new_page():

        location = ClientLocation.LocationContext.STATICCreateSimple( CC.LOCAL_FILE_SERVICE_KEY )

        return gui._notebook.NewPageDuplicateFilter( location_context = location )


    page = qt( new_page )

    sidebar = page.GetSidebar()

    # (before the filter: windows already open are not the filter's)
    before = qt( lambda: set( id( w ) for w in QW.QApplication.topLevelWidgets() ) )

    def launch():

        sidebar._filter_panel._LaunchFilter()

        for w in QW.QApplication.topLevelWidgets():

            if id( w ) in before:

                continue


            found = w.findChildren( ClientGUICanvasDuplicates.CanvasFilterDuplicates )

            if len( found ) > 0:

                state[ 'frame' ] = w
                state[ 'canvas' ] = found[0]

                return



        raise Exception( 'no filter window' )


    qt( launch )

    first = wait_for_pair()

    def batch():

        return [ [ a.GetHash().hex(), b.GetHash().hex() ] for ( a, b ) in state[ 'canvas' ]._batch_of_pairs_to_process ]


    pairs = qt( batch )

    for pair in pairs:

        batch_hashes.extend( pair )


    batch_hashes[ : ] = sorted( set( batch_hashes ) )

    step( 'launch', { 'canvas' : first, 'pairs in the batch' : len( pairs ) } )

    # --------------------------------------------------------------- 2. decide

    seen = []

    for name in DECISIONS:

        action = getattr( CAC, COMMANDS[ name ] )

        qt( lambda: state[ 'canvas' ].ProcessApplicationCommand( CAC.ApplicationCommand.STATICCreateSimpleCommand( action ) ) )

        time.sleep( 1.0 )

        seen.append( { 'do' : name, 'canvas' : wait_for_pair() } )


    step( 'decide', { 'decisions' : seen } )

    # ---------------------------------------------------------------- 3. close

    answers[ : ] = [ 'commit' ]

    ok = qt( lambda: state[ 'canvas' ].UserOKToClose() )

    time.sleep( 2.0 )

    flows.wait_until_idle( controller )

    assert len( answers ) == 0, f'unused answers: {answers}'

    step( 'close', { 'ok to close': ok } )

    return { 'domain_manager' : domain_manager, 'prepared' : prepared, 'files' : batch_hashes, 'steps' : steps }


def child( out ):

    import hydrus_driver
    import record_api

    with flows.Site( SITE_PORT, PHASES ) as site:

        db_dir = record_api.unpack_fixture( 'auto_resolution' )

        result = hydrus_driver.run_client( db_dir, lambda session: record( session, site ), network = True )


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

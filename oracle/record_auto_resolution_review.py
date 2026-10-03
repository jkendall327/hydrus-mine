#!/usr/bin/env python3
"""Record the reference's "review actions" window for a duplicates
auto-resolution rule (`ReviewActionsPanel`, opened by the "edit rules"
dialog's "review actions").

The window has three tabs: "pending actions" (pairs waiting for a human,
with "approve", "deny" and "select all"), "actions taken" (the rule's
log: what was done and when, with "undo") and "actions denied" (pairs a
human denied, when, with "undo"). It starts on "pending actions" for a
semi-automatic rule, else on "actions taken".

On the database `record_auto_resolution.py` left
(`legacy_db/auto_resolution.tar.gz`), with time held still and the
machine's time zone UTC, this opens the window for every rule and records
(`rules`) its starting tab, each tab's label and rows: the pair's two
files by hash and the third column's text. For a pending pair that is the
rule's action summary (`GetActionSummaryOnMatchingPair`, not either way).

Then (`steps`) on "pixel-perfect gifs vs pngs", whose two pairs wait for
approval: approve the first, deny the other, undo the first denied pair
(the newest), and undo the newest action taken by "pixel-perfect pairs";
after each, every tab fetched again, recording what was asked, what each
tab lists, and which of the files concerned are in the trash.

Usage: QT_QPA_PLATFORM=offscreen TZ=UTC python oracle/record_auto_resolution_review.py
       (writes fixtures/auto_resolution_review.json)
"""

import json
import os
import sys
import time

HERE = os.path.dirname( os.path.abspath( __file__ ) )

sys.path.insert( 0, HERE )

OUT = os.path.join( HERE, 'fixtures', 'auto_resolution_review.json' )

# well after the run that made the database
NOW = 1900000000

REVIEWED = 'pixel-perfect gifs vs pngs'
UNDONE = 'pixel-perfect pairs'


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

    asked = []

    def setup():

        from qtpy import QtWidgets as QW

        from hydrus.client.gui import ClientGUIDialogsQuick

        def yes_no( win, message, **kwargs ):

            asked.append( message )

            return QW.QDialog.DialogCode.Accepted


        ClientGUIDialogsQuick.GetYesNo = yes_no


    controller.CallBlockingToQt( gui, setup )

    rules = { rule.GetName() : rule for rule in manager.GetRules() }

    def make( rule ):

        from hydrus.client.gui.duplicates import ClientGUIDuplicatesAutoResolutionRuleReview as R

        return controller.CallBlockingToQt( gui, lambda: R.ReviewActionsPanel( gui, rule ) )


    def rows_of( pair_list ):

        model = pair_list.model()

        out = []

        for row in range( model.rowCount() ):

            ( a, b ) = model.GetMediaResultPair( row )

            text = model.data( model.index( row, 2 ), 0 )

            out.append( [ a.GetHash().hex(), b.GetHash().hex(), text ] )


        return out


    def fetch_all( panel ):

        def f():

            panel._RefetchPendingActionPairs()
            panel._RefetchActionedPairs()
            panel._RefetchDeniedPairs()


        controller.CallBlockingToQt( gui, f )


    def settle( panel ):

        # the fetches, and the pending summaries, are done off the Qt thread
        for _ in range( 100 ):

            time.sleep( 0.2 )

            def busy():

                labels = [ panel._pending_actions_label.text(), panel._actioned_pairs_label.text(), panel._denied_pairs_label.text() ]

                if any( 'fetching' in l for l in labels ):

                    return True


                model = panel._pending_actions_pair_list.model()

                texts = [ model.data( model.index( row, 2 ), 0 ) for row in range( model.rowCount() ) ]

                return any( t.startswith( 'calculating' ) for t in texts ) or not panel._pending_actions_panel.isEnabled()


            if not controller.CallBlockingToQt( gui, busy ):

                return


        raise Exception( 'the panel never settled' )


    def state( panel ):

        def f():

            return {
                'tab' : panel._main_notebook.currentIndex(),
                'tabs' : [ panel._main_notebook.tabText( i ) for i in range( panel._main_notebook.count() ) ],
                'pending' : { 'label' : panel._pending_actions_label.text(), 'rows' : rows_of( panel._pending_actions_pair_list ) },
                'actioned' : { 'label' : panel._actioned_pairs_label.text(), 'rows' : rows_of( panel._actioned_pairs_pair_list ) },
                'denied' : { 'label' : panel._denied_pairs_label.text(), 'rows' : rows_of( panel._denied_pairs_pair_list ) },
            }


        return controller.CallBlockingToQt( gui, f )


    def trashed( hashes ):

        from hydrus.client import ClientConstants as CC

        media_results = controller.Read( 'media_results', [ bytes.fromhex( h ) for h in hashes ] )

        return sorted( mr.GetHash().hex() for mr in media_results if CC.TRASH_SERVICE_KEY in mr.GetLocationsManager().GetCurrent() )


    out_rules = []

    for ( name, rule ) in sorted( rules.items() ):

        panel = make( rule )

        settle( panel )

        # the tab not shown is fetched as it is shown; record it fetched
        fetch_all( panel )
        settle( panel )

        s = state( panel )
        s[ 'name' ] = name

        out_rules.append( s )

        controller.CallBlockingToQt( gui, panel.deleteLater )


    steps = []

    def step( what, panel, act ):

        del asked[:]

        controller.CallBlockingToQt( gui, act )

        settle( panel )
        fetch_all( panel )
        settle( panel )

        s = state( panel )

        hashes = set()

        for tab in ( 'pending', 'actioned', 'denied' ):

            for ( a, b, _ ) in s[ tab ][ 'rows' ]:

                hashes.update( ( a, b ) )



        steps.append( { 'step' : what, 'asked' : list( asked ), 'state' : s, 'trashed' : trashed( sorted( hashes ) ) } )


    panel = make( rules[ REVIEWED ] )

    settle( panel )

    def select( pair_list, row ):

        return lambda: pair_list.selectRow( row )


    def approve():

        panel._pending_actions_pair_list.selectRow( 0 )
        panel._ApproveSelectedPending()


    step( 'approve the first pending pair', panel, approve )

    def deny():

        panel._pending_actions_pair_list.selectRow( 0 )
        panel._DenySelectedPending()


    step( 'deny the first pending pair', panel, deny )

    def undo_denied():

        panel._denied_pairs_pair_list.selectRow( 0 )
        panel._UndoSelectedDenied()


    step( 'undo the first denied pair', panel, undo_denied )

    controller.CallBlockingToQt( gui, panel.deleteLater )

    panel = make( rules[ UNDONE ] )

    settle( panel )

    def undo_actioned():

        panel._actioned_pairs_pair_list.selectRow( 0 )
        panel._UndoSelectedActioned()


    step( 'undo the first action taken', panel, undo_actioned )

    controller.CallBlockingToQt( gui, panel.deleteLater )

    return { 'now' : NOW, 'rules' : out_rules, 'reviewed' : REVIEWED, 'undone' : UNDONE, 'steps' : steps }


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

    os.environ[ 'TZ' ] = 'UTC'

    with tempfile.TemporaryDirectory() as work:

        path = os.path.join( work, 'auto_resolution_review.json' )

        hydrus_driver.run_in_subprocess( os.path.abspath( __file__ ), '--child', path )

        with open( path ) as f:

            result = json.load( f )



    with open( OUT, 'w' ) as f:

        json.dump( result, f, indent = 1, ensure_ascii = False )
        f.write( '\n' )


    print( f'wrote {OUT}: {len( result[ "rules" ] )} rules, {len( result[ "steps" ] )} steps' )


if __name__ == '__main__':

    main()

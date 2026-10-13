#!/usr/bin/env python3
"""Flow 1: add a subscription through the real dialogs, let it run, then run
it again once the site has a new file.

On the `basic` fixture, with the fake booru (`flows.Site`) serving five
"blue eyes" posts (two search pages), and its downloader installed:

1. "add": network > manage subscriptions (`_ManageSubscriptions`, its real
   worker and dialog), "add", the downloader chooser (the booru's search
   picked), the edit subscription dialog: named "blue eyes", a query added
   through its "add" button ("blue_eyes" typed in the query dialog); apply,
   apply. The subscriptions manager runs it; wait until all is idle.
2. "check again": the site gets a sixth post. Manage subscriptions again:
   the subscription selected, "check queries now", apply. Wait until idle.
3. "reopen": manage subscriptions once more, to see its list, then cancel.

After each step: the manage subscriptions dialog's rows as shown when it
opened (or at "apply"), the subscription and its query's logs as saved, the
files the subscription got (their tags, URLs, inbox and domains), the popups
still shown and the site's requests. Texts are normalised by
`flows.normaliser`.

Usage: QT_QPA_PLATFORM=offscreen ~/pyenv/bin/python oracle/record_flow_subscription.py
       (writes oracle/fixtures/flow_subscription.json)
"""

import os
import sys

HERE = os.path.dirname( os.path.abspath( __file__ ) )
sys.path.insert( 0, HERE )

import flows

OUT = os.path.join( HERE, 'fixtures', 'flow_subscription.json' )

SITE_PORT = 46011

PHASES = [
    flows.booru_phase( { 'blue_eyes' : [ 1, 2, 3, 4, 5 ] } ),
    flows.booru_phase( { 'blue_eyes' : [ 1, 2, 3, 4, 5, 6 ] } ),
]


def record( session, site ):

    controller = session.controller
    gui = controller.gui
    normalise = flows.normaliser( site )

    domain_manager = controller.CallBlockingToQt( gui, lambda: flows.install( controller, site ) )

    steps = []

    def rows( panel ):

        listctrl = panel._subscriptions

        out = []

        for subscription in listctrl.GetData():

            display = panel._ConvertSubscriptionToDisplayTuple( subscription )

            out.append( [ normalise( str( d ) ) for d in display ] )


        return out


    def manage( scripts ):
        """network > manage subscriptions, its dialogs scripted; until the
        dialog has closed and its worker finished."""

        done = []

        def opened( dlg, panel ):

            try:

                return scripts( dlg, panel )

            finally:

                done.append( True )



        with flows.scripted_dialogs( { 'EditSubscriptionsPanel' : [ opened ] } | extra_scripts ) as dialogs:

            controller.CallBlockingToQt( gui, gui._ManageSubscriptions )

            # (its worker saves, then lets the subscriptions run again)
            flows.wait_until_idle( controller, also = lambda: dialogs.check() and len( done ) > 0 and not controller.subscriptions_manager.SubscriptionsArePausedForEditing() )



    extra_scripts = {}

    def step( name, seen ):

        subscriptions = flows.subscriptions_state( session, normalise )

        steps.append( {
            'step' : name,
            'dialog' : seen,
            'subscriptions' : subscriptions,
            'files' : flows.files_state( session, flows.hashes_in( subscriptions ), normalise ),
            'popups' : flows.popups_shown( controller, normalise ),
            'hits' : site.take_hits(),
        } )


    # 1. add ---------------------------------------------------------

    seen = {}

    def chooser( dlg, panel ):

        listing = panel._list

        seen[ 'downloaders' ] = [ listing.item( i ).text() for i in range( listing.count() ) ]

        listing.clearSelection()
        listing.item( 0 ).setSelected( True )

        return True


    def query( dlg, panel ):

        seen[ 'query starts as' ] = panel._query_text.text()

        panel._query_text.setText( 'blue_eyes' )

        return True


    def editor( dlg, panel ):

        seen[ 'name starts as' ] = panel._name.text()

        panel._name.setText( 'blue eyes' )

        panel._AddQuery()

        seen[ 'initial file limit' ] = panel._initial_file_limit.value()
        seen[ 'periodic file limit' ] = panel._periodic_file_limit.value()

        return True


    def add( dlg, panel ):

        seen[ 'rows when opened' ] = rows( panel )

        panel.Add()

        seen[ 'rows at apply' ] = rows( panel )

        return True


    extra_scripts = { 'EditSelectFromListPanel' : [ chooser ], 'EditSubscriptionPanel' : [ editor ], 'EditSubscriptionQueryPanel' : [ query ] }

    manage( add )

    step( 'add', seen )

    # 2. check again -------------------------------------------------

    site.phase = 1

    seen = {}

    def check_now( dlg, panel ):

        seen[ 'rows when opened' ] = rows( panel )

        panel._subscriptions.SelectDatas( panel._subscriptions.GetData() )

        panel.CheckNow()

        seen[ 'rows at apply' ] = rows( panel )

        return True


    extra_scripts = {}

    manage( check_now )

    step( 'check again', seen )

    # 3. reopen ------------------------------------------------------

    seen = {}

    def look( dlg, panel ):

        seen[ 'rows when opened' ] = rows( panel )

        return False


    manage( look )

    step( 'reopen', seen )


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

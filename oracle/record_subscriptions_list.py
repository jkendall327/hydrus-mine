#!/usr/bin/env python3
"""Record how the reference's manage subscriptions dialog writes its lists.

network > subscriptions… lists the subscriptions (`EditSubscriptionsPanel.
_ConvertSubscriptionToDisplayTuple`): name, source (the downloader), status
("2 working, 1 paused, 1 dead", "no queries"), last new file time and last
checked (over all its queries, "n/a" if none), error/delay ("delayed--
retrying in 1 hour - because: …", or the bandwidth wait, "" here, no
bandwidth having been used), items ("12/14 - 1Ign1F"), paused ("yes") and
custom import options. Editing one lists its queries
(`EditSubscriptionPanel._ConvertQueryHeaderToDisplayTuple`): name/query
("name (query)" with a display name), paused, status ("ok", "dead"), last
new file time, last check time ("(initial check has not yet occurred)"),
next check time ("in 5 hours", "imminent", "checking on dialog ok",
"dead, so not checking", "paused, but would be …"), file velocity ("3
files in previous 1 day"), recent delays, items and additional tags.

In the running client, on the `basic` fixture, with the time held at
`now`, this makes subscriptions as `SUBSCRIPTIONS` describes them: each
query's file log (each file's status, and when it was found and posted,
in seconds before `now`), last check (seconds before `now`, or never),
check now and paused, synced to its log with the subscription's checker
options as the client does when it loads one (`SyncToQueryLogContainer`,
which may find it dead, and pause it if it has nothing left to do); and
records each subscription's row and its queries' rows, with each query's
state after the sync (checker status, paused, next check time).

Then, on the dialog's list holding those subscriptions, it records
`ACTIONS`: rows selected (by name), a button pressed, the questions it
asked ("Check which?" choices, "Remove all selected?", the select
subscriptions text box) and how they were answered, and after it every
subscription's state (paused, delay, each query's check now, paused, dead
and next check time), which are selected, and what the buttons can do for
the selection (check queries now, reset, scrub delays).

Statuses are `CC.STATUS_*`: 0 unknown, 1 successful and new, 2 already in
db, 3 deleted, 4 failed, 7 ignored, 8 skipped.

Usage: QT_QPA_PLATFORM=offscreen python oracle/record_subscriptions_list.py
       (writes fixtures/subscriptions_list.json)
"""

import json
import os
import sys
import types

HERE = os.path.dirname( os.path.abspath( __file__ ) )

sys.path.insert( 0, HERE )

OUT = os.path.join( HERE, 'fixtures', 'subscriptions_list.json' )

NOW = 1_700_000_000
HOUR = 3600
DAY = 86400

ARTIST = ( 4, DAY, 90 * DAY, ( 1, 180 * DAY ) )
FAST = ( 4, 12 * HOUR, 7 * DAY, ( 1, 90 * DAY ) )


def seeds( n, status, every, first, posted = True ):
    """`n` files of `status`, found `first` seconds ago and every `every`
    seconds before that, posted an hour before they were found."""

    return [ [ status, first + i * every, ( first + i * every + HOUR ) if posted else None ] for i in range( n ) ]


# each query: text, display name, file log, last check (seconds ago, or
# None), check now, paused
SUBSCRIPTIONS = [
    {
        'name' : 'artist one',
        'gug' : 'example tag search',
        'checker' : ARTIST,
        'paused' : False,
        'delay' : None,
        'queries' : [
            { 'text' : 'blue_eyes', 'display' : None, 'seeds' : seeds( 10, 1, 2 * DAY, HOUR ) + seeds( 2, 2, DAY, 3 * DAY ) + seeds( 1, 7, DAY, 5 * DAY ) + seeds( 1, 4, DAY, 6 * DAY ), 'last_check' : HOUR, 'check_now' : False, 'paused' : False },
            { 'text' : 'red_hair', 'display' : 'reds', 'seeds' : seeds( 3, 1, 10 * DAY, 2 * DAY ) + seeds( 2, 0, DAY, 2 * DAY ), 'last_check' : 2 * DAY, 'check_now' : False, 'paused' : True },
            { 'text' : 'quiet artist', 'display' : None, 'seeds' : seeds( 2, 1, 30 * DAY, 400 * DAY ), 'last_check' : 3 * DAY, 'check_now' : False, 'paused' : False },
            { 'text' : 'brand new', 'display' : None, 'seeds' : [], 'last_check' : None, 'check_now' : False, 'paused' : False },
        ],
    },
    {
        'name' : 'Empty sub',
        'gug' : 'example tag search',
        'checker' : ARTIST,
        'paused' : False,
        'delay' : None,
        'queries' : [],
    },
    {
        'name' : 'paused and delayed',
        'gug' : 'other search',
        'checker' : FAST,
        'paused' : True,
        'delay' : ( 2 * HOUR, 'network error: could not connect' ),
        'queries' : [
            { 'text' : 'green', 'display' : None, 'seeds' : seeds( 4, 1, HOUR, 20 * HOUR, posted = False ) + seeds( 3, 8, HOUR, 21 * HOUR ), 'last_check' : 20 * HOUR, 'check_now' : True, 'paused' : False },
            { 'text' : 'yellow', 'display' : None, 'seeds' : seeds( 1, 3, DAY, 2 * DAY ), 'last_check' : 2 * DAY, 'check_now' : False, 'paused' : False },
        ],
    },
    {
        'name' : 'no downloader',
        'gug' : '',
        'checker' : ARTIST,
        'paused' : False,
        'delay' : ( -HOUR, 'old delay' ),
        'queries' : [
            { 'text' : 'lonely', 'display' : None, 'seeds' : seeds( 1, 1, DAY, 10 * DAY ), 'last_check' : 10 * DAY, 'check_now' : False, 'paused' : True },
        ],
    },
]


# ( button, rows selected by name, answers in turn: a choice's index (None
# for closing it), a yes/no, or a text )
ACTIONS = [
    ( 'select', [ 'artist one' ], [] ),
    ( 'check queries now', [ 'artist one' ], [ 1, 1 ] ),
    ( 'select', [ 'paused and delayed' ], [] ),
    ( 'scrub delays', [ 'paused and delayed' ], [] ),
    ( 'check queries now', [ 'paused and delayed', 'no downloader' ], [ 0, 0 ] ),
    ( 'pause/resume', [ 'artist one', 'Empty sub', 'paused and delayed' ], [] ),
    ( 'check queries now', [ 'paused and delayed', 'Empty sub' ], [ 1 ] ),
    ( 'check queries now', [ 'paused and delayed' ], [ None ] ),
    ( 'check queries now', [ 'artist one' ], [ 0, 2, 0 ] ),
    ( 'select subscriptions', [ 'Empty sub' ], [ 'ell' ] ),
    ( 'select subscriptions', [], [ 'E' ] ),
    ( 'select subscriptions', [], [ 'e' ] ),
    ( 'delete', [ 'Empty sub', 'no downloader' ], [ False ] ),
    ( 'delete', [ 'Empty sub', 'no downloader' ], [ True ] ),
]


def checker( t ):

    from hydrus.client.importing.options import CheckerImportOptions

    ( intended, faster, slower, death ) = t

    return CheckerImportOptions.CheckerOptions( intended_files_per_check = intended, never_faster_than = faster, never_slower_than = slower, death_file_velocity = death )


def make_subscription( case ):
    """A subscription as `case` describes it (with the time held at `NOW`):
    the subscription, its query headers, their log containers by name, and
    each query's case with its state after the sync."""

    from hydrus.client.importing import ClientImportFileSeeds
    from hydrus.client.importing import ClientImportSubscriptionQuery
    from hydrus.client.importing import ClientImportSubscriptions

    options = checker( case[ 'checker' ] )

    subscription = ClientImportSubscriptions.Subscription( case[ 'name' ], gug_key_and_name = ( os.urandom( 32 ), case[ 'gug' ] ) )

    subscription.SetCheckerOptions( options )

    headers = []
    containers = {}
    queries = []

    for q in case[ 'queries' ]:

        header = ClientImportSubscriptionQuery.SubscriptionQueryHeader()

        header.SetQueryText( q[ 'text' ] )
        header.SetDisplayName( q[ 'display' ] )

        container = ClientImportSubscriptionQuery.SubscriptionQueryLogContainer( header.GetQueryLogContainerName() )

        cache = ClientImportFileSeeds.FileSeedCache()

        file_seeds = []

        for ( i, ( status, found_ago, posted_ago ) ) in enumerate( q[ 'seeds' ] ):

            file_seed = ClientImportFileSeeds.FileSeed( ClientImportFileSeeds.FILE_SEED_TYPE_URL, f'https://example.com/{q[ "text" ]}/{i}' )

            file_seed.status = status
            file_seed.created = NOW - found_ago
            file_seed.modified = file_seed.created
            file_seed.source_time = None if posted_ago is None else NOW - posted_ago

            file_seeds.append( file_seed )


        cache.AddFileSeeds( file_seeds )

        container.SetFileSeedCache( cache )

        header.SetLastCheckTime( 0 if q[ 'last_check' ] is None else NOW - q[ 'last_check' ] )
        header.SetCheckNow( q[ 'check_now' ] )
        header.SetPaused( q[ 'paused' ] )

        header.SyncToQueryLogContainer( options, container )

        headers.append( header )
        containers[ header.GetQueryLogContainerName() ] = container

        queries.append( {
            'query' : q,
            'after_sync' : {
                'dead' : header.IsDead(),
                'paused' : header.IsPaused(),
                'next_check_time' : header.GetNextCheckTime(),
            },
        } )


    subscription.SetQueryHeaders( headers )
    subscription.SetPaused( case[ 'paused' ] )

    if case[ 'delay' ] is not None:

        ( from_now, reason ) = case[ 'delay' ]

        subscription._no_work_until = NOW + from_now
        subscription._no_work_until_reason = reason


    return ( subscription, headers, containers, queries )


def record( session ):

    controller = session.controller
    gui = controller.gui

    def f():

        from hydrus.core import HydrusTime

        from hydrus.client.gui import ClientGUISubscriptions
        from hydrus.client.importing import ClientImportFileSeeds
        from hydrus.client.importing import ClientImportSubscriptionQuery
        from hydrus.client.importing import ClientImportSubscriptions

        from qtpy import QtWidgets as QW

        from hydrus.core import HydrusExceptions

        from hydrus.client.gui import ClientGUIDialogsQuick

        get_now = HydrusTime.GetNow

        HydrusTime.GetNow = lambda: NOW

        answers = []
        asked = []

        def select_from_list_buttons( win, title, choice_tuples, message = '', **kwargs ):

            answer = answers.pop( 0 )

            asked.append( { 'kind' : 'choice', 'title' : title, 'message' : message, 'choices' : [ t[0] for t in choice_tuples ], 'answer' : answer } )

            if answer is None:

                raise HydrusExceptions.CancelledException()


            return choice_tuples[ answer ][1]


        def get_yes_no( win, message, title = 'Are you sure?', yes_label = 'yes', no_label = 'no', **kwargs ):

            answer = answers.pop( 0 )

            asked.append( { 'kind' : 'yes/no', 'title' : title, 'message' : message, 'answer' : answer } )

            return QW.QDialog.DialogCode.Accepted if answer else QW.QDialog.DialogCode.Rejected


        def enter_text( win, message, **kwargs ):

            answer = answers.pop( 0 )

            asked.append( { 'kind' : 'text', 'message' : message, 'answer' : answer } )

            return answer


        ClientGUIDialogsQuick.SelectFromListButtons = select_from_list_buttons
        ClientGUIDialogsQuick.GetYesNo = get_yes_no
        ClientGUIDialogsQuick.EnterText = enter_text

        try:

            recorded = []
            made = []

            for case in SUBSCRIPTIONS:

                ( subscription, headers, containers, queries ) = make_subscription( case )

                row = ClientGUISubscriptions.EditSubscriptionsPanel._ConvertSubscriptionToDisplayTuple( None, subscription )

                stub = types.SimpleNamespace( _original_subscription = subscription )

                for ( query, header ) in zip( queries, headers ):

                    query[ 'row' ] = list( ClientGUISubscriptions.EditSubscriptionPanel._ConvertQueryHeaderToDisplayTuple( stub, header ) )


                recorded.append( {
                    'subscription' : case,
                    'row' : list( row ),
                    'queries' : queries,
                } )

                made.append( subscription )


            # the dialog's list, and its buttons
            panel = ClientGUISubscriptions.EditSubscriptionsPanel( gui, made )

            listctrl = panel._subscriptions

            def by_name( names ):

                return [ s for s in listctrl.GetData() if s.GetName() in names ]


            def state( do, names ):

                subs = sorted( listctrl.GetData(), key = lambda s: [ c[ 'name' ] for c in SUBSCRIPTIONS ].index( s.GetName() ) )

                return {
                    'do' : do,
                    'rows' : names,
                    'asked' : list( asked ),
                    'selected' : sorted( s.GetName() for s in listctrl.GetData( only_selected = True ) ),
                    'can_check_now' : panel._CanCheckNow(),
                    'can_reset' : panel._CanReset(),
                    'can_scrub_delays' : panel._CanScrubDelays(),
                    'subscriptions' : [
                        {
                            'name' : s.GetName(),
                            'paused' : s.IsPaused(),
                            'no_work_until' : s.ToTuple()[2],
                            'queries' : [
                                {
                                    'text' : h.GetQueryText(),
                                    'check_now' : h.IsCheckingNow(),
                                    'paused' : h.IsPaused(),
                                    'dead' : h.IsDead(),
                                    'next_check_time' : h.GetNextCheckTime(),
                                }
                                for h in s.GetQueryHeaders()
                            ],
                        }
                        for s in subs
                    ],
                }


            actions = []

            for ( do, names, step_answers ) in ACTIONS:

                answers[:] = list( step_answers )
                asked.clear()

                listctrl.SelectDatas( by_name( names ), deselect_others = True )

                if do == 'check queries now':

                    panel.CheckNow()

                elif do == 'scrub delays':

                    panel.ScrubDelays()

                elif do == 'pause/resume':

                    panel.PauseResume()

                elif do == 'select subscriptions':

                    panel.SelectSubscriptions()

                elif do == 'delete':

                    listctrl.ShowDeleteSelectedDialog()


                if answers:

                    raise Exception( f'unasked answers left at {do}: {answers}' )


                actions.append( state( do, names ) )


        finally:

            HydrusTime.GetNow = get_now


        return { 'now' : NOW, 'subscriptions' : recorded, 'actions' : actions }


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

        path = os.path.join( work, 'subscriptions_list.json' )

        hydrus_driver.run_in_subprocess( os.path.abspath( __file__ ), '--child', path )

        with open( path ) as f:

            result = json.load( f )



    with open( OUT, 'w' ) as f:

        json.dump( result, f, indent = 1, ensure_ascii = False )
        f.write( '\n' )


    print( f'wrote {OUT}: {len( result[ "subscriptions" ] )} subscriptions' )


if __name__ == '__main__':

    main()

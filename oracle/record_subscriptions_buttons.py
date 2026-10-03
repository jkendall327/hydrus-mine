#!/usr/bin/env python3
"""Record the reference's manage subscriptions dialog's other buttons.

On `EditSubscriptionsPanel` holding `record_subscriptions_list.py`'s
`SUBSCRIPTIONS` and `EXTRA` (the time held at its `NOW`), with every
query's log already loaded (as the dialog loads them before a reset or a
retry), this records `ACTIONS`: rows selected (by name), a button pressed,
what it asked and how it was answered, and after it every subscription in
the list's order by name: its name, downloader, paused, checker options
and each query's text, display name, paused, check now, dead, last and
next check times, and its file log's counts by status; with what the
buttons can do for the selection (lowercase, merge, separate, reset,
retry failed, retry ignored).

The buttons: "retry ignored" (asking which), "retry failed", "reset"
(asking first), "lowercase" (asking first), "overwrite checker options"
(the checker options editor answered with new options), "overwrite
downloader" (the downloader chooser answered with a name), "separate"
(asking how, and the new name) and "merge" (asking first, which is
primary, and the merged name).

Usage: QT_QPA_PLATFORM=offscreen python oracle/record_subscriptions_buttons.py
       (writes fixtures/subscriptions_buttons.json)
"""

import json
import os
import sys

HERE = os.path.dirname( os.path.abspath( __file__ ) )

sys.path.insert( 0, HERE )

import record_subscriptions_list as subs

OUT = os.path.join( HERE, 'fixtures', 'subscriptions_buttons.json' )

HOUR = subs.HOUR
DAY = subs.DAY

EXTRA = [
    {
        'name' : 'Mixed Case',
        'gug' : 'example tag search',
        'checker' : subs.ARTIST,
        'paused' : False,
        'delay' : None,
        'queries' : [
            { 'text' : 'Blue_Sky', 'display' : None, 'seeds' : subs.seeds( 2, 1, DAY, DAY ), 'last_check' : DAY, 'check_now' : False, 'paused' : False },
            { 'text' : 'green', 'display' : 'Greens', 'seeds' : subs.seeds( 1, 4, DAY, 2 * DAY ) + subs.seeds( 1, 7, DAY, 3 * DAY ), 'last_check' : 2 * DAY, 'check_now' : False, 'paused' : False },
        ],
    },
]

NEW_CHECKER = ( 2, 6 * HOUR, 3 * DAY, ( 1, 30 * DAY ) )

# ( button, rows selected by name, answers in turn: a choice's index (None
# to close it), a yes/no, a text, or for the editors what they give back )
ACTIONS = [
    ( 'retry ignored', [ 'artist one', 'Mixed Case' ], [ 0 ] ),
    ( 'retry failed', [ 'artist one', 'paused and delayed' ], [] ),
    ( 'reset', [ 'artist one' ], [ False ] ),
    ( 'reset', [ 'paused and delayed' ], [ True ] ),
    ( 'lowercase', [ 'Mixed Case', 'artist one' ], [ True ] ),
    ( 'overwrite checker options', [ 'artist one', 'no downloader' ], [ NEW_CHECKER ] ),
    ( 'overwrite downloader', [ 'no downloader', 'Mixed Case' ], [ 'other search' ] ),
    ( 'separate', [ 'artist one' ], [ 1, 'split' ] ),
    ( 'merge', [ 'split: blue_eyes', 'split: reds', 'split: brand new', 'Empty sub' ], [ True, 1, 'merged' ] ),
    ( 'separate', [ 'merged' ], [ 0 ] ),
    ( 'separate', [ 'paused and delayed' ], [ 'pd' ] ),
    ( 'merge', [ 'pd: green', 'pd: yellow', 'Mixed Case' ], [ True, 0, None ] ),
]


def record( session ):

    controller = session.controller
    gui = controller.gui

    def f():

        from qtpy import QtWidgets as QW

        from hydrus.core import HydrusExceptions
        from hydrus.core import HydrusTime

        from hydrus.client.gui import ClientGUIDialogsMessage
        from hydrus.client.gui import ClientGUIDialogsQuick
        from hydrus.client.gui import ClientGUISubscriptions
        from hydrus.client.gui.importing import ClientGUIImport

        get_now = HydrusTime.GetNow

        HydrusTime.GetNow = lambda: subs.NOW

        answers = []
        asked = []

        def pop():

            return answers.pop( 0 )


        def select_from_list_buttons( win, title, choice_tuples, message = '', **kwargs ):

            answer = pop()

            asked.append( { 'kind' : 'choice', 'title' : title, 'message' : message, 'choices' : [ t[0] for t in choice_tuples ], 'answer' : answer } )

            if answer is None:

                raise HydrusExceptions.CancelledException()


            return choice_tuples[ answer ][1]


        def select_from_list( win, title, choice_tuples, **kwargs ):

            answer = pop()

            asked.append( { 'kind' : 'list', 'title' : title, 'choices' : [ t[0] for t in choice_tuples ], 'answer' : answer } )

            if answer is None:

                raise HydrusExceptions.CancelledException()


            return choice_tuples[ answer ][1]


        def get_yes_no( win, message, title = 'Are you sure?', yes_label = 'yes', no_label = 'no', **kwargs ):

            answer = pop()

            asked.append( { 'kind' : 'yes/no', 'title' : title, 'message' : message, 'answer' : answer } )

            return QW.QDialog.DialogCode.Accepted if answer else QW.QDialog.DialogCode.Rejected


        def get_yes_yes_no( win, message, title = 'Are you sure?', yes_tuples = None, no_label = 'no' ):

            answer = pop()

            asked.append( { 'kind' : 'yes/yes/no', 'title' : title, 'message' : message, 'yeses' : [ t[0] for t in yes_tuples ], 'no' : no_label, 'answer' : answer } )

            if answer is None:

                raise HydrusExceptions.CancelledException()


            return yes_tuples[ answer ][1]


        def enter_text( win, message, default = '', **kwargs ):

            answer = pop()

            asked.append( { 'kind' : 'text', 'message' : message, 'default' : default, 'answer' : answer } )

            if answer is None:

                raise HydrusExceptions.CancelledException()


            return answer


        def show( kind ):

            def shower( win, message, **kwargs ):

                asked.append( { 'kind' : kind, 'message' : message } )


            return shower


        def select_gug( win, gug_key_and_name, for_new_sub = False ):

            name = pop()

            asked.append( { 'kind' : 'downloader', 'current' : gug_key_and_name[1], 'answer' : name } )

            return ( b'\x00' * 32, name )


        ClientGUIDialogsQuick.SelectFromListButtons = select_from_list_buttons
        ClientGUIDialogsQuick.SelectFromList = select_from_list
        ClientGUIDialogsQuick.GetYesNo = get_yes_no
        ClientGUIDialogsQuick.GetYesYesNo = get_yes_yes_no
        ClientGUIDialogsQuick.EnterText = enter_text
        ClientGUIDialogsMessage.ShowInformation = show( 'information' )
        ClientGUIDialogsMessage.ShowWarning = show( 'warning' )
        ClientGUIImport.SelectGUGKeyAndName = select_gug

        try:

            made = []
            containers = {}

            for case in subs.SUBSCRIPTIONS + EXTRA:

                ( subscription, headers, case_containers, queries ) = subs.make_subscription( case )

                made.append( subscription )
                containers.update( case_containers )


            panel = ClientGUISubscriptions.EditSubscriptionsPanel( gui, made )

            # (every log loaded already, so its jobs run at once)
            panel._names_to_edited_query_log_containers.update( containers )

            listctrl = panel._subscriptions

            def by_name( names ):

                return [ s for s in listctrl.GetData() if s.GetName() in names ]


            def files( header ):

                container = panel._names_to_edited_query_log_containers[ header.GetQueryLogContainerName() ]

                counts = {}

                for file_seed in container.GetFileSeedCache().GetFileSeeds():

                    counts[ str( file_seed.status ) ] = counts.get( str( file_seed.status ), 0 ) + 1


                return counts


            def checker( c ):

                return [ c._intended_files_per_check, c._never_faster_than, c._never_slower_than, list( c._death_file_velocity ) ]


            def state( do, names ):

                return {
                    'do' : do,
                    'rows' : names,
                    'asked' : list( asked ),
                    'selected' : sorted( s.GetName() for s in listctrl.GetData( only_selected = True ) ),
                    'can' : {
                        'lowercase' : panel._CanLowerCaseQueries(),
                        'merge' : panel._CanMerge(),
                        'separate' : panel._CanSeparate(),
                        'reset' : panel._CanReset(),
                        'retry_failed' : panel._CanRetryFailed(),
                        'retry_ignored' : panel._CanRetryIgnored(),
                    },
                    'subscriptions' : [
                        {
                            'name' : s.GetName(),
                            'gug' : s.GetGUGKeyAndName()[1],
                            'paused' : s.IsPaused(),
                            'checker' : checker( s.GetCheckerOptions() ),
                            'queries' : [
                                {
                                    'text' : h.GetQueryText(),
                                    'display' : h.GetDisplayName(),
                                    'paused' : h.IsPaused(),
                                    'check_now' : h.IsCheckingNow(),
                                    'dead' : h.IsDead(),
                                    'last_check_time' : h.GetLastCheckTime(),
                                    'next_check_time' : h.GetNextCheckTime(),
                                    'files' : files( h ),
                                }
                                for h in s.GetQueryHeaders()
                            ],
                        }
                        for s in sorted( listctrl.GetData(), key = lambda s: s.GetName() )
                    ],
                }


            start = state( 'start', [] )

            actions = []

            for ( do, names, step_answers ) in ACTIONS:

                answers[:] = list( step_answers )
                asked.clear()

                listctrl.SelectDatas( by_name( names ), deselect_others = True )

                if do == 'retry ignored':

                    panel._STARTRetryIgnored()

                elif do == 'retry failed':

                    panel._STARTRetryFailed()

                elif do == 'reset':

                    panel._STARTReset()

                elif do == 'lowercase':

                    panel.LowerCaseQueries()

                elif do == 'overwrite checker options':

                    # (what the editor gives back, as `SetCheckerOptions` does with it)
                    ( intended, faster, slower, death ) = pop()

                    new = subs.checker( ( intended, faster, slower, death ) )

                    asked.append( { 'kind' : 'checker options', 'answer' : checker( new ) } )

                    for subscription in listctrl.GetData( only_selected = True ):

                        subscription.SetCheckerOptions( new, names_to_query_log_containers = panel._names_to_edited_query_log_containers )


                    listctrl.UpdateDatas()

                elif do == 'overwrite downloader':

                    panel.SetDownloader()

                elif do == 'separate':

                    panel.Separate()

                elif do == 'merge':

                    panel.Merge()


                if answers:

                    raise Exception( f'unasked answers left at {do}: {answers}' )


                actions.append( state( do, names ) )


        finally:

            HydrusTime.GetNow = get_now


        return { 'now' : subs.NOW, 'extra' : EXTRA, 'start' : start, 'actions' : actions }


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

        path = os.path.join( work, 'subscriptions_buttons.json' )

        hydrus_driver.run_in_subprocess( os.path.abspath( __file__ ), '--child', path )

        with open( path ) as f:

            result = json.load( f )



    with open( OUT, 'w' ) as f:

        json.dump( result, f, indent = 1, ensure_ascii = False )
        f.write( '\n' )


    print( f'wrote {OUT}: {len( result[ "actions" ] )} actions' )


if __name__ == '__main__':

    main()

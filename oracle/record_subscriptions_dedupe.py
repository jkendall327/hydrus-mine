#!/usr/bin/env python3
"""Record the reference's manage subscriptions dialog's "deduplicate".

"deduplicate" (`EditSubscriptionsPanel.DedupeAll`) finds queries with
the same text on the same downloader, in one subscription or several,
asks whether to match case ("Caseless or cased?", or a yes/no when only
caseless duplicates exist), which downloader (if several have
duplicates), which texts (one asks yes/no; several: all, or a list to
pick from), and which subscription keeps them (if several have them;
one asks yes/no). That one keeps one query per text (its biggest file
log), the others lose theirs, and it offers to go on with the texts
left.

On the dialog holding `SUBSCRIPTIONS` (the time held at
`record_subscriptions_list.py`'s `NOW`), this records `ACTIONS`: each a
"deduplicate" with its answers in turn (a label to choose, its start
enough; a yes/no; a list of texts; None to cancel), what it asked, and
after it whether "deduplicate" is enabled and every subscription in the
list's order by name: its name, downloader and each query's text and
file count.

Usage: QT_QPA_PLATFORM=offscreen python oracle/record_subscriptions_dedupe.py
       (writes fixtures/subscriptions_dedupe.json)
"""

import json
import os
import sys

HERE = os.path.dirname( os.path.abspath( __file__ ) )

sys.path.insert( 0, HERE )

import record_subscriptions_list as subs

OUT = os.path.join( HERE, 'fixtures', 'subscriptions_dedupe.json' )

DAY = subs.DAY


def query( text, n ):

    return { 'text' : text, 'display' : None, 'seeds' : subs.seeds( n, 1, DAY, DAY ), 'last_check' : DAY, 'check_now' : False, 'paused' : False }


def subscription( name, gug, queries ):

    return { 'name' : name, 'gug' : gug, 'checker' : subs.ARTIST, 'paused' : False, 'delay' : None, 'queries' : queries }


SUBSCRIPTIONS = [
    subscription( 'sub a', 'example tag search', [ query( 'blue', 1 ), query( 'Red', 5 ), query( 'green', 2 ), query( 'only a', 1 ) ] ),
    subscription( 'sub b', 'example tag search', [ query( 'blue', 3 ), query( 'red', 1 ), query( 'yellow', 2 ) ] ),
    subscription( 'sub c', 'example tag search', [ query( 'Blue', 4 ), query( 'green', 6 ), query( 'yellow', 1 ) ] ),
    subscription( 'cats', 'other search', [ query( 'cat', 1 ), query( 'dog', 2 ), query( 'cat', 3 ), query( 'Cat', 2 ) ] ),
    subscription( 'dogs', 'other search', [ query( 'Dog', 1 ) ] ),
]

ACTIONS = [
    # both kinds; which downloader; pick texts; keep in "sub a" (which has
    # two of them), then the rest in "sub c"
    [ 'do a normal', 'example tag search', 'select which', [ 'blue', 'red', 'yellow' ], 'sub a', True, 'sub c' ],
    # exact text only, on the other downloader: "cats" keeps "cat"
    [ 'only do exact', 'other search', True, True ],
    # cancelled at the first question
    [ None ],
    # what is left, ignoring case: all of "other search"'s, keep in "cats"
    [ 'do a normal', 'other search', 'do them all', 'cats' ],
    # and the rest
    [ 'do a normal', 'do them all', 'sub a', True ],
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

        HydrusTime.GetNow = lambda: subs.NOW

        answers = []
        asked = []

        def pop():

            return answers.pop( 0 ) if len( answers ) > 0 else None


        def pick( labels, answer ):

            if answer is None:

                raise HydrusExceptions.CancelledException()


            return next( i for ( i, label ) in enumerate( labels ) if label.startswith( answer ) )


        def select_from_list_buttons( win, title, choice_tuples, message = '', **kwargs ):

            answer = pop()

            labels = [ t[0] for t in choice_tuples ]

            asked.append( { 'kind' : 'choice', 'title' : title, 'message' : message, 'choices' : labels, 'answer' : answer } )

            return choice_tuples[ pick( labels, answer ) ][1]


        def select_multiple( win, title, choice_tuples, **kwargs ):

            answer = pop()

            asked.append( { 'kind' : 'multiple', 'title' : title, 'choices' : sorted( t[0] for t in choice_tuples ), 'checked' : sorted( t[0] for t in choice_tuples if t[2] ), 'answer' : answer } )

            if answer is None:

                raise HydrusExceptions.CancelledException()


            return [ t[1] for t in choice_tuples if t[0] in answer ]


        def get_yes_no( win, message, title = 'Are you sure?', **kwargs ):

            answer = pop()

            asked.append( { 'kind' : 'yes/no', 'title' : title, 'message' : message, 'answer' : answer } )

            return QW.QDialog.DialogCode.Accepted if answer else QW.QDialog.DialogCode.Rejected


        def get_yes_yes_no( win, message, title = 'Are you sure?', yes_tuples = None, no_label = 'no' ):

            answer = pop()

            labels = [ t[0] for t in yes_tuples ]

            asked.append( { 'kind' : 'yes/yes/no', 'title' : title, 'message' : message, 'yeses' : labels, 'no' : no_label, 'answer' : answer } )

            return yes_tuples[ pick( labels, answer ) ][1]


        def show( kind ):

            def shower( win, message, **kwargs ):

                asked.append( { 'kind' : kind, 'message' : message } )


            return shower


        ClientGUIDialogsQuick.SelectFromListButtons = select_from_list_buttons
        ClientGUIDialogsQuick.SelectMultipleFromList = select_multiple
        ClientGUIDialogsQuick.GetYesNo = get_yes_no
        ClientGUIDialogsQuick.GetYesYesNo = get_yes_yes_no
        ClientGUIDialogsMessage.ShowWarning = show( 'warning' )

        made = []
        containers = {}

        for case in SUBSCRIPTIONS:

            ( subscription, headers, case_containers, queries ) = subs.make_subscription( case )

            made.append( subscription )
            containers.update( case_containers )


        panel = ClientGUISubscriptions.EditSubscriptionsPanel( gui, made )

        panel._names_to_edited_query_log_containers.update( containers )

        listctrl = panel._subscriptions

        def state( answers_given ):

            return {
                'answers' : answers_given,
                'asked' : list( asked ),
                'can_dedupe' : panel._CanDedupeAll(),
                'subscriptions' : [
                    {
                        'name' : s.GetName(),
                        'gug' : s.GetGUGKeyAndName()[1],
                        'queries' : [ [ h.GetQueryText(), h.GetFileSeedCacheStatus().GetFileSeedCount() ] for h in s.GetQueryHeaders() ],
                    }
                    for s in sorted( listctrl.GetData(), key = lambda s: s.GetName() )
                ],
            }


        start = state( [] )

        actions = []

        for step_answers in ACTIONS:

            answers[:] = list( step_answers )
            asked.clear()

            panel.DedupeAll()

            actions.append( state( step_answers ) )


        return { 'now' : subs.NOW, 'subscriptions' : SUBSCRIPTIONS, 'start' : start, 'actions' : actions }


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

    # (the reference lists some choices in a set's order)
    os.environ[ 'PYTHONHASHSEED' ] = '0'

    with tempfile.TemporaryDirectory() as work:

        path = os.path.join( work, 'subscriptions_dedupe.json' )

        hydrus_driver.run_in_subprocess( os.path.abspath( __file__ ), '--child', path )

        with open( path ) as f:

            result = json.load( f )



    with open( OUT, 'w' ) as f:

        json.dump( result, f, indent = 1, ensure_ascii = False )
        f.write( '\n' )


    print( f'wrote {OUT}: {len( result[ "actions" ] )} actions' )


if __name__ == '__main__':

    main()

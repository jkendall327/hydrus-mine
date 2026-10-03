#!/usr/bin/env python3
"""Record the reference's "edit subscription" dialog (`EditSubscriptionPanel`).

Opened on a subscription from `record_subscriptions_list.py`'s
`SUBSCRIPTIONS` (the time held at its `NOW`), this records:

* `opened`: each subscription's dialog as it opens: its name, the delay
  line ("no recent errors", "delayed--retrying in 2 hours because: …"),
  the downloader button's label, the file limits, "do not worry about
  subscription gaps", "currently paused", and the publication options
  (popup, popup button, page, the label override or none, merged);
* `steps`: on "artist one"'s dialog, a run of the queries list's buttons
  (`STEPS`): rows selected (by query text), a button pressed, what it
  asked (the "Check which?" choices, "Remove all selected?", pasting
  queries' messages and questions, with the clipboard's text) and how it
  was answered, and after it every query's text, paused, check now and
  dead, and the delay line;
* `value`: what the dialog gives back on "apply" after its fields are
  changed (`EDITS`): the subscription's name, paused, file limits, random
  sample, publication options, delay, and its queries in order.

Usage: QT_QPA_PLATFORM=offscreen python oracle/record_edit_subscription.py
       (writes fixtures/edit_subscription.json)
"""

import json
import os
import sys

HERE = os.path.dirname( os.path.abspath( __file__ ) )

sys.path.insert( 0, HERE )

import record_subscriptions_list as subs

OUT = os.path.join( HERE, 'fixtures', 'edit_subscription.json' )

# ( button, query texts selected, answers in turn (a choice's index or None
# to close it, a yes/no, or for "paste queries" the clipboard's text then
# the answers) )
STEPS = [
    ( 'paste queries', [], [ 'Blue_Eyes\nfresh query\nFresh Query\n\n  quiet artist  \n', 1 ] ),
    ( 'paste queries', [], [ 'blue_eyes\nred_hair' ] ),
    ( 'paste queries', [], [ '\n  \n' ] ),
    ( 'paste queries', [], [ 'another one', False ] ),
    ( 'paste queries', [], [ 'another one', True ] ),
    ( 'check now', [ 'quiet artist' ], [ 0, 0 ] ),
    ( 'check now', [ 'blue_eyes', 'red_hair', 'quiet artist' ], [ 1 ] ),
    ( 'pause/play', [ 'blue_eyes', 'red_hair' ], [] ),
    ( 'check now', [ 'blue_eyes', 'red_hair' ], [ None ] ),
    ( 'paste queries', [], [ 'quiet artist\nyet another', True ] ),
    ( 'delete', [ 'fresh query', 'Fresh Query' ], [ False ] ),
    ( 'delete', [ 'fresh query', 'Fresh Query' ], [ True ] ),
]

EDITS = {
    'name' : 'artist one, renamed',
    'paused' : True,
    'initial_file_limit' : 50,
    'periodic_file_limit' : 25,
    'random_sample' : True,
    'show_a_popup_while_working' : False,
    'publish_files_to_popup_button' : False,
    'publish_files_to_page' : True,
    'publish_label_override' : 'my label',
    'merge_query_publish_events' : False,
}


def record( session ):

    controller = session.controller
    gui = controller.gui

    def f():

        from qtpy import QtWidgets as QW

        from hydrus.core import HydrusExceptions
        from hydrus.core import HydrusTime

        from hydrus.client import ClientGlobals as CG
        from hydrus.client.gui import ClientGUIDialogsMessage
        from hydrus.client.gui import ClientGUIDialogsQuick
        from hydrus.client.gui import ClientGUISubscriptions

        get_now = HydrusTime.GetNow

        HydrusTime.GetNow = lambda: subs.NOW

        answers = []
        asked = []
        clipboard = [ '' ]

        def select_from_list_buttons( win, title, choice_tuples, message = '', **kwargs ):

            answer = answers.pop( 0 )

            asked.append( { 'kind' : 'choice', 'title' : title, 'message' : message, 'choices' : [ t[0] for t in choice_tuples ], 'answer' : answer } )

            if answer is None:

                raise HydrusExceptions.CancelledException()


            return choice_tuples[ answer ][1]


        def get_yes_no( win, message, title = 'Are you sure?', yes_label = 'yes', no_label = 'no', **kwargs ):

            answer = answers.pop( 0 )

            asked.append( { 'kind' : 'yes/no', 'title' : title, 'message' : message, 'yes' : yes_label, 'no' : no_label, 'answer' : answer } )

            return QW.QDialog.DialogCode.Accepted if answer else QW.QDialog.DialogCode.Rejected


        def get_yes_yes_no( win, message, title = 'Are you sure?', yes_tuples = None, no_label = 'no' ):

            answer = answers.pop( 0 )

            asked.append( { 'kind' : 'yes/yes/no', 'title' : title, 'message' : message, 'yeses' : [ t[0] for t in yes_tuples ], 'no' : no_label, 'answer' : answer } )

            if answer is None:

                raise HydrusExceptions.CancelledException()


            return yes_tuples[ answer ][1]


        def show( kind ):

            def shower( win, message, **kwargs ):

                asked.append( { 'kind' : kind, 'message' : message } )


            return shower


        ClientGUIDialogsQuick.SelectFromListButtons = select_from_list_buttons
        ClientGUIDialogsQuick.GetYesNo = get_yes_no
        ClientGUIDialogsQuick.GetYesYesNo = get_yes_yes_no
        ClientGUIDialogsMessage.ShowInformation = show( 'information' )
        ClientGUIDialogsMessage.ShowWarning = show( 'warning' )

        controller.GetClipboardText = lambda: clipboard[0]

        def panel_for( case ):

            ( subscription, headers, containers, queries ) = subs.make_subscription( case )

            return ClientGUISubscriptions.EditSubscriptionPanel( gui, subscription, containers, set( containers.keys() ) )


        def fields( panel ):

            label_override = panel._publish_label_override.GetValue()

            return {
                'name' : panel._name.text(),
                'delay' : panel._delay_st.text(),
                'downloader' : panel._gug_key_and_name.text(),
                'initial_file_limit' : panel._initial_file_limit.value(),
                'periodic_file_limit' : panel._periodic_file_limit.value(),
                'random_sample' : panel._this_is_a_random_sample_sub.isChecked(),
                'paused' : panel._paused.isChecked(),
                'show_a_popup_while_working' : panel._show_a_popup_while_working.isChecked(),
                'publish_files_to_popup_button' : panel._publish_files_to_popup_button.isChecked(),
                'publish_files_to_page' : panel._publish_files_to_page.isChecked(),
                'publish_label_override' : label_override,
                'merge_query_publish_events' : panel._merge_query_publish_events.isChecked(),
            }


        def queries_state( panel ):

            return [
                {
                    'text' : h.GetQueryText(),
                    'paused' : h.IsPaused(),
                    'check_now' : h.IsCheckingNow(),
                    'dead' : h.IsDead(),
                }
                for h in panel._query_headers.GetData()
            ]


        try:

            opened = [ fields( panel_for( case ) ) for case in subs.SUBSCRIPTIONS ]

            case = next( c for c in subs.SUBSCRIPTIONS if c[ 'name' ] == 'artist one' )

            panel = panel_for( case )

            listctrl = panel._query_headers

            steps = []

            for ( do, texts, step_answers ) in STEPS:

                answers[:] = list( step_answers )
                asked.clear()

                listctrl.SelectDatas( [ h for h in listctrl.GetData() if h.GetQueryText() in texts ], deselect_others = True )

                if do == 'paste queries':

                    clipboard[0] = answers.pop( 0 )

                    panel._PasteQueries()

                elif do == 'check now':

                    panel._CheckNow()

                elif do == 'pause/play':

                    panel._PausePlay()

                elif do == 'delete':

                    listctrl.ShowDeleteSelectedDialog()


                if answers:

                    raise Exception( f'unasked answers left at {do}: {answers}' )


                steps.append( {
                    'do' : do,
                    'rows' : texts,
                    'clipboard' : clipboard[0] if do == 'paste queries' else None,
                    'asked' : list( asked ),
                    'queries' : queries_state( panel ),
                    'delay' : panel._delay_st.text(),
                } )


            # the fields changed, then "apply"
            panel._name.setText( EDITS[ 'name' ] )
            panel._paused.setChecked( EDITS[ 'paused' ] )
            panel._initial_file_limit.setValue( EDITS[ 'initial_file_limit' ] )
            panel._periodic_file_limit.setValue( EDITS[ 'periodic_file_limit' ] )
            panel._this_is_a_random_sample_sub.setChecked( EDITS[ 'random_sample' ] )
            panel._show_a_popup_while_working.setChecked( EDITS[ 'show_a_popup_while_working' ] )
            panel._publish_files_to_popup_button.setChecked( EDITS[ 'publish_files_to_popup_button' ] )
            panel._publish_files_to_page.setChecked( EDITS[ 'publish_files_to_page' ] )
            panel._publish_label_override.SetValue( EDITS[ 'publish_label_override' ] )
            panel._merge_query_publish_events.setChecked( EDITS[ 'merge_query_publish_events' ] )

            ( subscription, edited_containers ) = panel.GetValue()

            ( initial_file_limit, periodic_file_limit, no_work_until, no_work_until_reason ) = subscription.ToTuple()

            ( show_a_popup_while_working, publish_files_to_popup_button, publish_files_to_page, publish_label_override, merge_query_publish_events ) = subscription.GetPresentationOptions()

            value = {
                'name' : subscription.GetName(),
                'paused' : subscription.IsPaused(),
                'initial_file_limit' : initial_file_limit,
                'periodic_file_limit' : periodic_file_limit,
                'random_sample' : subscription.ThisIsARandomSampleSubscription(),
                'show_a_popup_while_working' : show_a_popup_while_working,
                'publish_files_to_popup_button' : publish_files_to_popup_button,
                'publish_files_to_page' : publish_files_to_page,
                'publish_label_override' : publish_label_override,
                'merge_query_publish_events' : merge_query_publish_events,
                'no_work_until' : no_work_until,
                'no_work_until_reason' : no_work_until_reason,
                'queries' : [
                    {
                        'text' : h.GetQueryText(),
                        'paused' : h.IsPaused(),
                        'check_now' : h.IsCheckingNow(),
                        'dead' : h.IsDead(),
                    }
                    for h in subscription.GetQueryHeaders()
                ],
            }

        finally:

            HydrusTime.GetNow = get_now


        return { 'now' : subs.NOW, 'opened' : opened, 'steps' : steps, 'edits' : EDITS, 'value' : value }


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

        path = os.path.join( work, 'edit_subscription.json' )

        hydrus_driver.run_in_subprocess( os.path.abspath( __file__ ), '--child', path )

        with open( path ) as f:

            result = json.load( f )



    with open( OUT, 'w' ) as f:

        json.dump( result, f, indent = 1, ensure_ascii = False )
        f.write( '\n' )


    print( f'wrote {OUT}: {len( result[ "steps" ] )} steps' )


if __name__ == '__main__':

    main()

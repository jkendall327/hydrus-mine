#!/usr/bin/env python3
"""Record the popup message manager's cards as the user sees them: the title,
two texts and two gauges (and what pausing hides), the cancel button, the
traceback buttons and the complete diagnostic text they copy, the yes/no
question's buttons, and the summary bar's count and collapse arrow.

In the running client, on the `basic` fixture, job statuses are added to the
real `PopupMessageManager`, and its widgets are read (each shown/hidden, text,
gauge range and value) and clicked (`QPushButton.click()`, as the user's
press does):

* `content`: jobs with every field, with only some, a paused one, and one
  whose second text is past the 1024-character cutoff;
* `cancel`: a cancellable job's stop button clicked, then the job and card;
* `traceback`: a job with a traceback: "show traceback" clicked, then "hide
  traceback", then "copy traceback information" (the clipboard read back);
* `question`: a yes/no question, answered yes and answered no;
* `summary`: twelve jobs (the cards shown, the count), the collapse arrow
  clicked and clicked again.

Usage: QT_QPA_PLATFORM=offscreen ~/pyenv/bin/python oracle/record_popup_cards.py
       (writes fixtures/popup_cards.json)
"""

import json
import sys
import tempfile
import time
from pathlib import Path

HERE = Path( __file__ ).resolve().parent

sys.path.insert( 0, str( HERE ) )


def record( session ):

    # (the manager's own "initialising" message is gone after a second)
    time.sleep( 2 )

    def drive():

        from hydrus.client import ClientThreading
        from hydrus.client.gui import ClientGUIPopupMessages
        from qtpy import QtWidgets as QW

        gui = session.controller.gui

        manager = gui._message_manager

        def cards():

            vbox = manager._message_vbox

            windows = [ vbox.itemAt( i ).widget() for i in range( vbox.count() ) ]

            return [ w for w in windows if w is not None ]


        def gauge( g ):

            ( value, range ) = g.GetValueRange()

            return { 'shown' : not g.isHidden(), 'value' : value, 'range' : range, 'pulsing' : g._is_pulsing }


        def text( t ):

            return { 'shown' : not t.isHidden(), 'text' : t.text() }


        def card_state( w ):

            return {
                'title' : text( w._title ),
                'text_1' : text( w._text_1 ),
                'gauge_1' : gauge( w._gauge_1 ),
                'text_2' : text( w._text_2 ),
                'gauge_2' : gauge( w._gauge_2 ),
                'question' : text( w._text_yes_no ),
                'yes_shown' : not w._yes.isHidden(),
                'no_shown' : not w._no.isHidden(),
                'show_tb' : { 'shown' : not w._show_tb_button.isHidden(), 'text' : w._show_tb_button.text() },
                'tb_text' : text( w._tb_text ),
                'copy_tb_shown' : not w._copy_tb_button.isHidden(),
                'pause_shown' : not w._pause_button.isHidden(),
                'cancel_shown' : not w._cancel_button.isHidden(),
                'cancel_enabled' : w._cancel_button.isEnabled(),
                'job' : {
                    'done' : w._job_status.IsDone(),
                    'dismissed' : w._job_status.IsDismissed(),
                    'cancelled' : w._job_status.IsCancelled(),
                    'cancellable' : w._job_status.IsCancellable(),
                    'pausable' : w._job_status.IsPausable(),
                    'paused' : w._job_status.IsPaused(),
                    'answer' : w._job_status.GetIfHasVariable( 'popup_yes_no_answer' ),
                },
            }


        def refresh():

            for w in cards():

                w.UpdateMessage()


            QW.QApplication.processEvents()


        def clear():

            for job in list( manager._job_status_queue.GetJobStatuses() ):

                job.FinishAndDismiss()


            manager.DismissAll()

            QW.QApplication.processEvents()


        def show( jobs ):

            for job in jobs:

                manager.AddMessage( job )


            manager._CheckPending()

            refresh()


        def summary():

            return {
                'summary' : manager._summary_bar._text.text(),
                'panel_shown' : not manager._message_panel.isHidden(),
                'texts' : [ w._job_status.GetStatusText() for w in cards() ],
            }


        result = {}

        # content

        full = ClientThreading.JobStatus( pausable = True )
        full.SetStatusTitle( 'a title' )
        full.SetStatusText( 'first' )
        full.SetStatusText( 'second', 2 )
        full.SetGauge( 1, 4 )
        full.SetVariable( 'popup_gauge_2', ( 0, 0 ) )

        only_text = ClientThreading.JobStatus()
        only_text.SetStatusText( 'plain' )

        bare = ClientThreading.JobStatus()

        long = ClientThreading.JobStatus()
        long.SetStatusText( 'long' )
        long.SetStatusText( 'y' * 1500, 2 )

        show( [ full, only_text, bare, long ] )

        content = [ { 'do' : 'shown', 'cards' : [ card_state( w ) for w in cards() ] } ]

        # (the card of the full job, paused: the button is clicked)
        card = next( w for w in cards() if w._job_status is full )

        card._pause_button.click()

        QW.QApplication.processEvents()

        refresh()

        content.append( { 'do' : 'pause', 'cards' : [ card_state( w ) for w in cards() ] } )

        card._pause_button.click()

        QW.QApplication.processEvents()

        refresh()

        content.append( { 'do' : 'play', 'cards' : [ card_state( w ) for w in cards() ] } )

        result[ 'content' ] = content

        clear()

        # cancel

        stoppable = ClientThreading.JobStatus( pausable = True, cancellable = True )
        stoppable.SetStatusText( 'stoppable' )

        fixed = ClientThreading.JobStatus( cancellable = False )
        fixed.SetStatusText( 'cannot stop' )

        show( [ stoppable, fixed ] )

        cancel = [ { 'do' : 'shown', 'cards' : [ card_state( w ) for w in cards() ] } ]

        card = next( w for w in cards() if w._job_status is stoppable )

        card._cancel_button.click()

        refresh()

        cancel.append( { 'do' : 'cancel', 'cards' : [ card_state( w ) for w in cards() ] } )

        result[ 'cancel' ] = cancel

        clear()

        # traceback

        broken = ClientThreading.JobStatus()
        broken.SetStatusTitle( 'oh no' )
        broken.SetStatusText( 'it broke' )
        broken.SetVariable( 'traceback', 'trace line 1\ntrace line 2\n' + 'z' * 2000 )

        show( [ broken ] )

        card = cards()[0]

        traceback = [ { 'do' : 'shown', 'card' : card_state( card ) } ]

        card._show_tb_button.click()

        QW.QApplication.processEvents()

        traceback.append( { 'do' : 'show traceback', 'card' : card_state( card ) } )

        card._show_tb_button.click()

        QW.QApplication.processEvents()

        traceback.append( { 'do' : 'hide traceback', 'card' : card_state( card ) } )

        QW.QApplication.clipboard().clear()

        card._copy_tb_button.click()

        time.sleep( 0.5 )

        QW.QApplication.processEvents()

        copied = QW.QApplication.clipboard().text()

        traceback.append( { 'do' : 'copy', 'card' : card_state( card ), 'clipboard' : copied, 'job_string' : broken.ToString() } )

        result[ 'traceback' ] = traceback

        clear()

        # question

        question = []

        for answer in ( True, False ):

            asking = ClientThreading.JobStatus()
            asking.SetStatusText( 'asking' )
            asking.SetVariable( 'popup_yes_no_question', 'Accept this?' )

            show( [ asking ] )

            card = cards()[0]

            before = card_state( card )

            ( card._yes if answer else card._no ).click()

            QW.QApplication.processEvents()

            question.append( { 'answer' : answer, 'before' : before, 'after' : card_state( card ) } )

            clear()


        result[ 'question' ] = question

        # summary

        jobs = []

        for i in range( 12 ):

            job = ClientThreading.JobStatus()
            job.SetStatusText( f'm{i}' )

            jobs.append( job )


        show( jobs )

        steps = [ { 'do' : 'shown', 'state' : summary() } ]

        arrow = manager._summary_bar._expand_collapse

        arrow.click()

        QW.QApplication.processEvents()

        steps.append( { 'do' : 'collapse', 'state' : summary() } )

        arrow.click()

        QW.QApplication.processEvents()

        steps.append( { 'do' : 'expand', 'state' : summary() } )

        result[ 'summary' ] = steps

        for job in jobs:

            job.FinishAndDismiss()

        return result


    return session.controller.CallBlockingToQt( session.controller.gui, drive )


def main():

    import hydrus_driver
    import record_api

    if len( sys.argv ) > 1 and sys.argv[1] == '--child':

        Path( sys.argv[2] ).write_text( json.dumps( hydrus_driver.run_client( record_api.unpack_fixture( 'basic' ), record ) ) )

        return


    with tempfile.TemporaryDirectory() as directory:

        path = Path( directory ) / 'result.json'

        hydrus_driver.run_in_subprocess( str( Path( __file__ ).resolve() ), '--child', str( path ) )

        result = json.loads( path.read_text() )


    ( HERE / 'fixtures' / 'popup_cards.json' ).write_text( json.dumps( result, indent = 2 ) + '\n' )

    print( 'wrote popup_cards.json' )


if __name__ == '__main__':

    main()

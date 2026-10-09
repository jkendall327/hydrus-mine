#!/usr/bin/env python3
"""Record the popup message manager's dismissals: three jobs shown (one
still working, two done), each card right-clicked in turn (a real right
mouse press on the card, `EventDismiss` -> `TryToDismiss`), then the summary
bar's "dismiss all" button clicked. After each step: the cards shown (their
text, whether the job is done or dismissed), and the summary bar's text.

Usage: QT_QPA_PLATFORM=offscreen ~/pyenv/bin/python oracle/record_popup_dismiss.py
       (writes fixtures/popup_dismiss.json)
"""

import json
import sys
import tempfile
import time
from pathlib import Path

HERE = Path( __file__ ).resolve().parent

sys.path.insert( 0, str( HERE ) )

STEPS = [
    [ 'right_click', 'working' ],
    [ 'right_click', 'done' ],
    [ 'dismiss_all' ],
]


def record( session ):

    # (the manager's own "initialising" message is gone after a second)
    time.sleep( 2 )

    def drive():

        from hydrus.client import ClientThreading
        from qtpy import QtCore as QC
        from qtpy import QtGui as QG
        from qtpy import QtWidgets as QW

        manager = session.controller.gui._message_manager

        def cards():

            vbox = manager._message_vbox

            windows = [ vbox.itemAt( i ).widget() for i in range( vbox.count() ) ]

            return [ w for w in windows if w is not None ]


        def state():

            return {
                'cards' : [ { 'text' : w._job_status.GetStatusText(), 'done' : w._job_status.IsDone(), 'dismissed' : w._job_status.IsDismissed() } for w in cards() ],
                'summary' : manager._summary_bar._text.text()
            }


        working = ClientThreading.JobStatus( pausable = True, cancellable = True )
        working.SetStatusText( 'working' )

        done = ClientThreading.JobStatus()
        done.SetStatusText( 'done' )
        done.Finish()

        also = ClientThreading.JobStatus()
        also.SetStatusText( 'also done' )
        also.Finish()

        for job in ( working, done, also ):

            manager.AddMessage( job )


        manager._CheckPending()
        QW.QApplication.processEvents()

        steps = [ { 'state' : state() } ]

        for step in STEPS:

            if step[0] == 'right_click':

                card = next( w for w in cards() if w._job_status.GetStatusText() == step[1] )

                point = QC.QPointF( 5, 5 )

                event = QG.QMouseEvent( QC.QEvent.Type.MouseButtonPress, point, card.mapToGlobal( point ), QC.Qt.MouseButton.RightButton, QC.Qt.MouseButton.RightButton, QC.Qt.KeyboardModifier.NoModifier )

                QW.QApplication.sendEvent( card, event )

            else:

                buttons = [ b for b in manager._summary_bar.findChildren( QW.QPushButton ) if b.text() == 'dismiss all' ]

                buttons[0].click()


            QW.QApplication.processEvents()

            steps.append( { 'do' : step, 'state' : state() } )


        for job in ( working, done, also ):

            job.FinishAndDismiss()


        return { 'steps' : steps }


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


    ( HERE / 'fixtures' / 'popup_dismiss.json' ).write_text( json.dumps( result, indent = 2 ) + '\n' )

    print( 'wrote popup_dismiss.json' )


if __name__ == '__main__':

    main()

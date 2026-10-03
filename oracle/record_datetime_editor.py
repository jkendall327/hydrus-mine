#!/usr/bin/env python3
"""Record the reference's date-time editor (`ClientGUITime.DateTimesCtrl`),
as "manage times" opens it to edit a time over some files.

In the running client, in UTC with the time held still (`NOW`), for each
case the real control (seconds and milliseconds allowed, no "no time",
only past dates) is given a time over some files (`files`: each file's
time in milliseconds, or null for none; `step`, the range's step in
milliseconds) and driven step by step:

* `["date", "yyyy-MM-dd"]` / `["time", "hh:mm:ss.zzz"]`: the calendar's
  date or the time box set;
* `["step", seconds]`: the cascading step set;
* `["now"]`: the "now" button;
* `["copy"]`: copy (recording the text);
* `["paste", text]`: paste that text.

After each step `state` has the label over it (null if hidden), whether
the step box shows, the date, time and step shown, the value it gives
(as the time range's text, `ToString`), and whether that is a change.
`said` has the notices, warnings and errors shown; `copied` what was
copied.

Usage: QT_QPA_PLATFORM=offscreen python oracle/record_datetime_editor.py
       (writes fixtures/datetime_editor.json)
"""

import json
import os
import sys
import time

HERE = os.path.dirname( os.path.abspath( __file__ ) )

sys.path.insert( 0, HERE )

OUT = os.path.join( HERE, 'fixtures', 'datetime_editor.json' )

os.environ[ 'TZ' ] = 'UTC'
time.tzset()

NOW = 1700000000

T = NOW * 1000

DAY = 86400 * 1000

CASES = [
    { 'name' : 'one file', 'files' : [ T - 10 * DAY + 123 ], 'step' : 0, 'steps' : [
        [ 'copy' ],
        [ 'date', '2023-01-02' ],
        [ 'time', '03:04:05.678' ],
        [ 'date', '2030-01-01' ],
        [ 'now' ],
        [ 'paste', '1600000000' ],
        [ 'paste', '1600000000.25' ],
        [ 'paste', '"1500000000"' ],
        [ 'paste', 'null' ],
        [ 'paste', '"soon"' ],
        [ 'paste', 'nonsense words' ],
        [ 'paste', '[1, 2]' ],
        [ 'paste', '2023-11-01 12:30:00' ],
        [ 'paste', '2023-10-05T01:02:03.456' ],
        [ 'paste', '2023-09-01' ],
        [ 'copy' ],
    ] },
    { 'name' : 'three files', 'files' : [ T - 10 * DAY, T - 5 * DAY, None ], 'step' : 0, 'steps' : [
        [ 'step', 1.5 ],
        [ 'step', -0.005 ],
        [ 'date', '2023-11-01' ],
        [ 'step', 0 ],
    ] },
    { 'name' : 'two files the same, stepped', 'files' : [ T - DAY, T - DAY ], 'step' : 250, 'steps' : [
        [ 'time', '00:00:00.000' ],
    ] },
]


def record( session ):

    controller = session.controller
    gui = controller.gui

    from hydrus.core import HydrusTime

    HydrusTime.GetNow = lambda: NOW

    def f():

        import types

        from qtpy import QtCore as QC

        from hydrus.client.gui import ClientGUIDialogsMessage
        from hydrus.client.gui import ClientGUIFunctions
        from hydrus.client.gui.metadata import ClientGUITime as M

        said = []
        clipboard = []
        pasted = []

        real_pub = controller.pub

        def pub( topic, *args, **kwargs ):

            if topic == 'clipboard':

                clipboard.append( args[ 1 ] )

                return


            real_pub( topic, *args, **kwargs )


        controller.pub = pub
        controller.GetClipboardText = lambda: pasted[ -1 ]

        ClientGUIDialogsMessage.ShowWarning = lambda win, message: said.append( { 'warning' : message } )
        ClientGUIDialogsMessage.ShowCritical = lambda win, title, message: said.append( { 'critical' : title, 'message' : message } )
        ClientGUIFunctions.ShowMicroNotification = lambda win, text: said.append( { 'notice' : text } )

        held = types.SimpleNamespace( **{ k : getattr( QC, k ) for k in dir( QC ) if not k.startswith( '__' ) } )

        class HeldDateTime( QC.QDateTime ):

            currentDateTime = staticmethod( lambda: QC.QDateTime.fromMSecsSinceEpoch( NOW * 1000, QC.QTimeZone.systemTimeZone() ) )


        held.QDateTime = HeldDateTime
        M.QC = held

        out = []

        for case in CASES:

            value = M.DateTimeWidgetValueRange()

            for ms in case[ 'files' ]:

                value.AddValueTimestampMS( ms )


            value.SetStepMS( case[ 'step' ] )

            ctrl = M.DateTimesCtrl( gui, seconds_allowed = True, milliseconds_allowed = True, none_allowed = False, only_past_dates = True )

            ctrl.SetValue( value )

            ctrl.show()

            def state():

                return {
                    'label' : ctrl._label.text() if ctrl._label.isVisibleTo( ctrl ) else None,
                    'step_shown' : ctrl._step_box.isVisibleTo( ctrl ),
                    'date' : ctrl._date.selectedDate().toString( 'yyyy-MM-dd' ),
                    'time' : ctrl._time.time().toString( 'hh:mm:ss.zzz' ),
                    'step' : ctrl._step.GetValue(),
                    'value' : ctrl.GetValue().ToString(),
                    'changed' : ctrl.HasChanges(),
                }


            steps = [ { 'state' : state() } ]

            for step in case[ 'steps' ]:

                del said[:]
                del clipboard[:]

                kind = step[ 0 ]

                if kind == 'date':

                    ctrl._date.setSelectedDate( QC.QDate.fromString( step[ 1 ], 'yyyy-MM-dd' ) )

                elif kind == 'time':

                    ctrl._time.setTime( QC.QTime.fromString( step[ 1 ], 'hh:mm:ss.zzz' ) )

                elif kind == 'step':

                    ctrl._step.SetValue( step[ 1 ] )

                elif kind == 'now':

                    ctrl._SetNow()

                elif kind == 'copy':

                    ctrl._Copy()

                elif kind == 'paste':

                    pasted.append( step[ 1 ] )

                    ctrl._Paste()


                steps.append( { 'do' : step, 'said' : list( said ), 'copied' : list( clipboard ), 'state' : state() } )


            ctrl.hide()
            ctrl.deleteLater()

            out.append( { 'name' : case[ 'name' ], 'files' : case[ 'files' ], 'step' : case[ 'step' ], 'steps' : steps } )


        return { 'now' : NOW, 'cases' : out }


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

        path = os.path.join( work, 'datetime_editor.json' )

        hydrus_driver.run_in_subprocess( os.path.abspath( __file__ ), '--child', path )

        with open( path ) as f:

            result = json.load( f )



    with open( OUT, 'w' ) as f:

        json.dump( result, f, indent = 1, ensure_ascii = False )
        f.write( '\n' )


    print( f'wrote {OUT}: {len( result[ "cases" ] )} cases' )


if __name__ == '__main__':

    main()

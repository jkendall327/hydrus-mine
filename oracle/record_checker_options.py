#!/usr/bin/env python3
"""Record the reference's checker options editor (`EditCheckerOptions`).

A checker options button ("checker options", `CheckerOptionsButton`)
opens it, as the options dialog's downloading page does for the default
subscription and watcher checker options. It has five "reasonable
defaults" buttons, the file velocity below which checking stops, whether
to check at a static interval, and either the reactive checking box
(intended new files per check, never faster than, never slower than) or
the static one (the check period). Its times have smaller minimums in
advanced mode. A "never faster than" above "never slower than" moves the
latter up (`_UpdateTimeDeltas`, on every change of either's fields); and
"ok" with the two the same asks first (`UserIsOKToOK`).

In the running client, on the `basic` fixture, this records:

* `presets`: each defaults button's label and the checker options it
  sets (`[intended files per check, never faster than, never slower than,
  [files, seconds]]`, as `ToTuple`);
* `panels`: the editor opened on given checker options, in normal and
  advanced mode: its layout (as `record_options_dialog.py` records
  controls, hidden ones `"hidden"`) and its value (`GetValue`);
* `steps`: from given checker options, edits in turn (a defaults button
  clicked, a time's field or the intended files or the velocity set as
  typed, the static checkbox clicked), with the value after each, which
  of the two boxes is hidden, and the times' fields as shown;
* `ok`: "ok" pressed on given checker options: the question asked (if
  any), answered yes or no, and whether the editor closes;
* `summaries`: given checker options' `GetSummary` (the button's
  tooltip);
* `downloading`: the options dialog's downloading page's two buttons,
  their checker options, and those the options get when changed there and
  applied.

Usage: QT_QPA_PLATFORM=offscreen python oracle/record_checker_options.py
       (writes fixtures/checker_options.json)
"""

import json
import os
import sys

HERE = os.path.dirname( os.path.abspath( __file__ ) )

sys.path.insert( 0, HERE )

OUT = os.path.join( HERE, 'fixtures', 'checker_options.json' )

DAY = 86400

# ( intended, never faster than, never slower than, ( files, seconds ) )
THREAD = ( 4, 300, DAY, ( 1, 3 * DAY ) )
ARTIST = ( 4, DAY, 90 * DAY, ( 1, 180 * DAY ) )
STATIC = ( 2.5, 3 * 3600, 3 * 3600, ( 0, 7 * DAY ) )
ODD = ( 1.33, 59, 61, ( 3, 90 ) )

PANELS = [ THREAD, STATIC, ODD ]

# ( given, [ edits ] ): ( 'preset', label ); ( 'field', time, field, value )
# with time 'faster', 'slower' or 'flat' and field its spin box ('days',
# 'hours', 'minutes', 'seconds'); ( 'intended', value ); ( 'velocity',
# files, field, value ); ( 'flat', ) clicks the static checkbox
STEPS = [
    # each preset from a subscription's, and back
    ( ARTIST, [ ( 'preset', 'thread' ), ( 'preset', 'slow thread' ), ( 'preset', 'faster tag subscription' ), ( 'preset', 'medium tag/artist subscription' ), ( 'preset', 'slower tag subscription' ), ( 'preset', 'thread' ) ] ),
    ( THREAD, [ ( 'preset', 'slower tag subscription' ), ( 'preset', 'thread' ) ] ),
    ( STATIC, [ ( 'preset', 'thread' ) ] ),
    # never faster than raised past never slower than, a field at a time
    ( THREAD, [ ( 'field', 'faster', 'days', 2 ), ( 'field', 'faster', 'hours', 5 ), ( 'field', 'slower', 'days', 1 ), ( 'field', 'slower', 'days', 3 ) ] ),
    # never slower than lowered below never faster than
    ( ARTIST, [ ( 'field', 'slower', 'days', 0 ), ( 'field', 'slower', 'hours', 2 ), ( 'field', 'faster', 'days', 0 ), ( 'field', 'faster', 'minutes', 30 ) ] ),
    # static on, its period changed, off again, and on again
    ( THREAD, [ ( 'flat', ), ( 'field', 'flat', 'hours', 6 ), ( 'flat', ), ( 'flat', ) ] ),
    ( STATIC, [ ( 'flat', ), ( 'field', 'faster', 'hours', 1 ), ( 'flat', ) ] ),
    # the intended files and the velocity
    ( THREAD, [ ( 'intended', 2.345 ), ( 'intended', 0.1 ), ( 'intended', 5000 ), ( 'velocity', 0, None, None ), ( 'velocity', 7, 'hours', 5 ), ( 'velocity', 5000, 'days', 0 ) ] ),
    # below the minimums (as typed; they are kept until focus leaves)
    ( THREAD, [ ( 'field', 'faster', 'minutes', 0 ), ( 'field', 'slower', 'days', 0 ) ] ),
]

# ( given, [ edits ] ), as STEPS'
OKS = [
    ( THREAD, [] ),
    # (the same times open as static checking)
    ( ( 4, 600, 600, ( 1, DAY ) ), [] ),
    # the same times, as reactive checking
    ( THREAD, [ ( 'field', 'slower', 'days', 0 ) ] ),
    ( ( 4, 600, 600, ( 1, DAY ) ), [ ( 'flat', ) ] ),
]

SUMMARIES = [ THREAD, ARTIST, STATIC, ODD, ( 10, 43200, 30 * DAY, ( 1, 90 * DAY ) ), ( 1000, 60, 2 * DAY + 3 * 3600 + 7, ( 1234, 3600 ) ) ]


def checker( t ):

    from hydrus.client.importing.options import CheckerImportOptions

    ( intended, faster, slower, death ) = t

    return CheckerImportOptions.CheckerOptions( intended_files_per_check = intended, never_faster_than = faster, never_slower_than = slower, death_file_velocity = death )


def as_list( checker_options ):

    ( intended, faster, slower, ( files, seconds ) ) = checker_options.ToTuple()

    return [ intended, faster, slower, [ files, seconds ] ]


def fields( w ):

    names = [ 'days', 'hours', 'minutes', 'seconds' ]

    return { name : getattr( w, '_' + name ).value() for name in names if getattr( w, '_show_' + name, False ) }


def state( panel ):

    return {
        'value' : as_list( panel.GetValue() ),
        'flat' : panel._flat_check_period_checkbox.isChecked(),
        'reactive_hidden' : panel._reactive_check_panel.isHidden(),
        'static_hidden' : panel._static_check_panel.isHidden(),
        'faster' : fields( panel._never_faster_than ),
        'slower' : fields( panel._never_slower_than ),
        'flat_period' : fields( panel._flat_check_period ),
        'intended' : panel._intended_files_per_check.value(),
        'velocity' : [ panel._death_file_velocity._num.value(), fields( panel._death_file_velocity._times ) ],
    }


def do( panel, edit ):

    from qtpy import QtWidgets as QW

    if edit[ 0 ] == 'preset':

        [ b ] = [ b for b in panel.findChildren( QW.QPushButton ) if b.text() == edit[ 1 ] ]

        b.click()

    elif edit[ 0 ] == 'field':

        ( _, which, field, value ) = edit

        w = { 'faster' : panel._never_faster_than, 'slower' : panel._never_slower_than, 'flat' : panel._flat_check_period }[ which ]

        getattr( w, '_' + field ).setValue( value )

    elif edit[ 0 ] == 'intended':

        panel._intended_files_per_check.setValue( edit[ 1 ] )

    elif edit[ 0 ] == 'velocity':

        ( _, files, field, value ) = edit

        panel._death_file_velocity._num.setValue( files )

        if field is not None:

            getattr( panel._death_file_velocity._times, '_' + field ).setValue( value )


    elif edit[ 0 ] == 'flat':

        panel._flat_check_period_checkbox.click()



def record( session ):

    controller = session.controller
    gui = controller.gui

    def f():

        from qtpy import QtWidgets as QW

        from hydrus.client.gui import ClientGUIDialogsQuick
        from hydrus.client.gui import ClientGUITopLevelWindowsPanels
        from hydrus.client.gui.metadata import ClientGUITime

        import record_options_dialog

        new_options = controller.new_options

        advanced_before = new_options.GetBoolean( 'advanced_mode' )

        def editor( given, advanced ):

            new_options.SetBoolean( 'advanced_mode', advanced )

            dlg = ClientGUITopLevelWindowsPanels.DialogEdit( gui, 'edit checker options' )

            panel = ClientGUITime.EditCheckerOptions( dlg, checker( given ) )

            dlg.SetPanel( panel )

            return ( dlg, panel )


        result = {}

        # the presets, by their buttons
        ( dlg, panel ) = editor( ARTIST, False )

        from hydrus.client.gui.widgets import ClientGUICommon

        [ defaults_box ] = [ b for b in panel.findChildren( ClientGUICommon.StaticBox ) if b._title_st.text() == 'reasonable defaults' ]

        # (less the box's own arrow, which folds it)
        buttons = [ b for b in defaults_box.findChildren( QW.QPushButton ) if not isinstance( b, ClientGUICommon.ExpandCollapseArrowButton ) ]

        presets = []

        for b in buttons:

            b.click()

            presets.append( { 'label' : b.text(), 'value' : as_list( panel.GetValue() ) } )


        result[ 'presets' ] = presets

        dlg.deleteLater()

        # the layouts
        panels = []

        for advanced in ( False, True ):

            for given in PANELS:

                ( dlg, panel ) = editor( given, advanced )

                panels.append( {
                    'advanced' : advanced,
                    'given' : as_list( checker( given ) ),
                    'layout' : record_options_dialog.walk( panel.widget().layout() ),
                    'state' : state( panel ),
                } )

                dlg.deleteLater()



        result[ 'panels' ] = panels

        # the steps
        steps = []

        for ( given, edits ) in STEPS:

            ( dlg, panel ) = editor( given, False )

            after = []

            for edit in edits:

                do( panel, edit )

                after.append( { 'edit' : list( edit ), **state( panel ) } )


            steps.append( { 'given' : as_list( checker( given ) ), 'steps' : after } )

            dlg.deleteLater()


        result[ 'steps' ] = steps

        # "ok"
        oks = []

        for ( given, edits ) in OKS:

            for answer in ( True, False ):

                asked = []

                def get_yes_no( win, message, *args, **kwargs ):

                    asked.append( message )

                    return QW.QDialog.DialogCode.Accepted if answer else QW.QDialog.DialogCode.Rejected


                original = ClientGUIDialogsQuick.GetYesNo

                ClientGUIDialogsQuick.GetYesNo = get_yes_no

                try:

                    ( dlg, panel ) = editor( given, False )

                    for edit in edits:

                        do( panel, edit )


                    ok = panel.UserIsOKToOK()

                finally:

                    ClientGUIDialogsQuick.GetYesNo = original


                oks.append( { 'given' : as_list( checker( given ) ), 'edits' : [ list( e ) for e in edits ], 'value' : as_list( panel.GetValue() ), 'answer' : answer, 'asked' : asked, 'ok' : ok } )

                dlg.deleteLater()



        result[ 'ok' ] = oks

        result[ 'summaries' ] = [ { 'given' : as_list( checker( given ) ), 'summary' : checker( given ).GetSummary() } for given in SUMMARIES ]



        # the downloading page's buttons, and applied
        new_options.SetBoolean( 'advanced_mode', False )

        from hydrus.client.gui.panels.options import ClientGUIManageOptionsPanel

        options_panel = ClientGUIManageOptionsPanel.ManageOptionsPanel( gui )

        book = options_panel._listbook

        [ downloading ] = [ book.widget( i ) for i in range( book.count() ) if book.tabText( i ) == 'downloading' ]

        before = {
            'subscriptions' : as_list( downloading._subscription_checker_options.GetValue() ),
            'watchers' : as_list( downloading._watcher_checker_options.GetValue() ),
            'subscriptions_tooltip' : downloading._subscription_checker_options.toolTip(),
        }

        downloading._subscription_checker_options.SetValue( checker( STATIC ) )
        downloading._watcher_checker_options.SetValue( checker( ODD ) )

        stored_before = ( as_list( new_options.GetDefaultSubscriptionCheckerOptions() ), as_list( new_options.GetDefaultWatcherCheckerOptions() ) )

        downloading.UpdateOptions()

        applied = {
            'subscriptions' : as_list( new_options.GetDefaultSubscriptionCheckerOptions() ),
            'watchers' : as_list( new_options.GetDefaultWatcherCheckerOptions() ),
        }

        # (back as they were)
        new_options.SetDefaultSubscriptionCheckerOptions( checker( tuple( stored_before[ 0 ][ :3 ] ) + ( tuple( stored_before[ 0 ][ 3 ] ), ) ) )
        new_options.SetDefaultWatcherCheckerOptions( checker( tuple( stored_before[ 1 ][ :3 ] ) + ( tuple( stored_before[ 1 ][ 3 ] ), ) ) )

        options_panel.deleteLater()

        result[ 'downloading' ] = { 'before' : before, 'set' : { 'subscriptions' : as_list( checker( STATIC ) ), 'watchers' : as_list( checker( ODD ) ) }, 'applied' : applied }

        new_options.SetBoolean( 'advanced_mode', advanced_before )

        return result


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

        path = os.path.join( work, 'checker.json' )

        hydrus_driver.run_in_subprocess( os.path.abspath( __file__ ), '--child', path )

        with open( path ) as f:

            result = json.load( f )



    with open( OUT, 'w' ) as f:

        json.dump( result, f, indent = 1, ensure_ascii = False )
        f.write( '\n' )


    print( f'wrote {OUT}: {len( result[ "presets" ] )} presets, {len( result[ "steps" ] )} step runs' )


if __name__ == '__main__':

    main()

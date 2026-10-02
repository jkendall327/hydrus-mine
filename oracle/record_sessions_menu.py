#!/usr/bin/env python3
"""Record the reference's pages > sessions entries that save and load.

pages > sessions > save > "as new session…" asks a name
(`ProposeSaveGUISession`): a reserved name ("last session", "exit
session", "just a blank page", none) is refused with a warning and asked
again; an existing one asks whether to overwrite it ("no, choose another
name" asks again). save > a session's name asks whether to overwrite it.
"clear and load" > a session's name, with pages open, asks whether to
close them (`LoadGUISession`), then loads the session in their place, its
pages at the top (not in a page of pages, as "append" puts them).

In the running client, on the `basic` fixture, with its dialogs answered
in turn, this records each step: what it did (`do`), what was asked and
answered (`asked`: each dialog's kind, message, title and button labels
as the reference gives them, and the answer), and after it the saved
sessions (`sessions`: each name and how many times it has been saved)
and the open pages' names (`pages`, top level).

Usage: QT_QPA_PLATFORM=offscreen python oracle/record_sessions_menu.py
       (writes fixtures/sessions_menu.json)
"""

import json
import os
import sys
import time

HERE = os.path.dirname( os.path.abspath( __file__ ) )

sys.path.insert( 0, HERE )

OUT = os.path.join( HERE, 'fixtures', 'sessions_menu.json' )

# ( what, argument, answers ): answers in turn to the dialogs asked: a
# text for a name (or None to cancel), True/False for yes/no ('closed'
# for closing the question)
STEPS = [
    ( 'save new', None, [ 'last session', 'just a blank page', 'my session' ] ),
    ( 'save new', None, [ 'my session', False, 'second session' ] ),
    ( 'save new', None, [ 'my session', 'closed' ] ),
    ( 'save new', None, [ 'my session', True ] ),
    ( 'save new', None, [ None ] ),
    ( 'save', 'second session', [ False ] ),
    ( 'save', 'second session', [ True ] ),
    ( 'new page', None, [] ),
    ( 'save', 'second session', [ True ] ),
    ( 'clear and load', 'my session', [ False ] ),
    ( 'clear and load', 'my session', [ True ] ),
    ( 'clear and load', 'second session', [ True ] ),
    # pages that say they can't close quietly (two, the same reason; then
    # one): asked about after the load is
    ( 'veto', 'This page is still importing.', [] ),
    ( 'clear and load', 'my session', [ True, False ] ),
    ( 'clear and load', 'my session', [ True, True ] ),
    ( 'veto', '2 watchers are still importing.', [] ),
    ( 'clear and load', 'second session', [ True, False ] ),
]


def record( session ):

    controller = session.controller
    gui = controller.gui

    from qtpy import QtWidgets as QW

    from hydrus.core import HydrusExceptions
    from hydrus.core import HydrusSerialisable

    from hydrus.client.gui import ClientGUIDialogsMessage
    from hydrus.client.gui import ClientGUIDialogsQuick

    answers = []
    asked = []

    def enter_text( win, message, default = '', **kwargs ):

        answer = answers.pop( 0 )

        asked.append( { 'kind' : 'text', 'message' : message, 'default' : default, 'answer' : answer } )

        if answer is None:

            raise HydrusExceptions.CancelledException()


        return answer


    def get_yes_no( win, message, title = 'Are you sure?', yes_label = 'yes', no_label = 'no', check_for_cancelled = False, **kwargs ):

        answer = answers.pop( 0 )

        asked.append( { 'kind' : 'yes/no', 'message' : message, 'title' : title, 'yes' : yes_label, 'no' : no_label, 'answer' : answer } )

        result = QW.QDialog.DialogCode.Accepted if answer is True else QW.QDialog.DialogCode.Rejected

        if check_for_cancelled:

            return ( result, answer == 'closed' )


        return result


    def get_yes_no_no( win, message, title = 'Are you sure?', yes_label = 'yes', no_tuples = None, auto_yes_time = None, disable_yes_initially = False ):

        answer = answers.pop( 0 )

        asked.append( { 'kind' : 'yes/no/no', 'message' : message, 'title' : title, 'yes' : yes_label, 'nos' : [ list( t ) for t in ( no_tuples or [] ) ], 'yes_disabled_at_first' : disable_yes_initially, 'answer' : answer } )

        if answer is True:

            return ( QW.QDialog.DialogCode.Accepted, None )


        return ( QW.QDialog.DialogCode.Rejected, 'no' )


    def show_warning( win, message, **kwargs ):

        asked.append( { 'kind' : 'warning', 'message' : message } )


    ClientGUIDialogsQuick.EnterText = enter_text
    ClientGUIDialogsQuick.GetYesNo = get_yes_no
    ClientGUIDialogsQuick.GetYesNoNo = get_yes_no_no
    ClientGUIDialogsMessage.ShowWarning = show_warning

    def sessions():

        names = controller.Read( 'serialisable_names', HydrusSerialisable.SERIALISABLE_TYPE_GUI_SESSION_CONTAINER )

        backups = controller.Read( 'serialisable_names_to_backup_timestamps_ms', HydrusSerialisable.SERIALISABLE_TYPE_GUI_SESSION_CONTAINER )

        return { name : len( backups.get( name, [] ) ) for name in sorted( names ) }


    def pages():

        notebook = gui._notebook

        return [ notebook.widget( i ).GetName() for i in range( notebook.count() ) ]


    steps = [ { 'do' : 'start', 'sessions' : sessions(), 'pages' : controller.CallBlockingToQt( gui, pages ) } ]

    for ( what, name, step_answers ) in STEPS:

        answers[:] = list( step_answers )
        asked.clear()

        def f():

            if what == 'save new':

                gui.ProposeSaveGUISession()

            elif what == 'save':

                gui.ProposeSaveGUISession( name )

            elif what == 'new page':

                from hydrus.client import ClientConstants as CC
                from hydrus.client import ClientLocation

                gui._notebook.NewPageQuery( ClientLocation.LocationContext.STATICCreateSimple( CC.LOCAL_FILE_SERVICE_KEY ) )

            elif what == 'veto':

                def veto( for_session_close = False, reason = name ):

                    raise HydrusExceptions.VetoException( reason )


                for page in gui._notebook.GetMediaPages():

                    page.CheckAbleToClose = veto


            elif what == 'clear and load':

                gui._notebook.LoadGUISession( name )



        controller.CallBlockingToQt( gui, f )

        # (saves are written on another thread, and a load waits a second)
        time.sleep( 3 )

        steps.append( {
            'do' : what,
            'name' : name,
            'asked' : list( asked ),
            'unanswered' : list( answers ),
            'sessions' : sessions(),
            'pages' : controller.CallBlockingToQt( gui, pages ),
        } )


    return { 'steps' : steps }


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

        path = os.path.join( work, 'sessions_menu.json' )

        hydrus_driver.run_in_subprocess( os.path.abspath( __file__ ), '--child', path )

        with open( path ) as f:

            result = json.load( f )



    with open( OUT, 'w' ) as f:

        json.dump( result, f, indent = 1, ensure_ascii = False )
        f.write( '\n' )


    print( f'wrote {OUT}: {len( result[ "steps" ] )} steps' )


if __name__ == '__main__':

    main()

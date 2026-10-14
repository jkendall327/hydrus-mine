#!/usr/bin/env python3
"""Record the reference's external call editor panels on the real classes.

- `EditProcessCallExecutableAndParametersPanel.UserIsOKToOK`: the questions
  asked (and whether the dialog may close) for an empty path, a name `which`
  cannot find and a name it can, under scripted answers;
- `EditProcessCallPanel._ShowPATH`: the message shown, over a known PATH;
- the input parameter rows of a "send single file" call (label, enabled,
  replacement token), and `CheckValid`'s veto for each way a call can be wrong;
- the timeout control: what a timeout entered, or "this can live for a very long
  time", saves as, with the hide terminal and output is text flags.

Usage: QT_QPA_PLATFORM=offscreen ~/pyenv/bin/python oracle/record_external_call_editor.py
       (writes oracle/fixtures/external_call_editor.json)
"""
import json
import os
import sys
import tempfile

HERE = os.path.dirname( os.path.abspath( __file__ ) )
sys.path.insert( 0, HERE )

OUT = os.path.join( HERE, 'fixtures', 'external_call_editor.json' )


def record( session ):

    c = session.controller

    def work():

        from qtpy import QtWidgets as QW
        from hydrus.core import HydrusExceptions
        from hydrus.client.gui import ClientGUIDialogsMessage, ClientGUIDialogsQuick
        from hydrus.client.gui.panels.options import ExternalProgramsPanel as P
        from hydrus.client.executables import ClientExecutableActualCall as A, ClientExecutablePipelines as L

        out = {}

        answers = []
        asked = []

        def get_yes_no( parent, message, *args, **kwargs ):

            asked.append( message )

            return QW.QDialog.DialogCode.Accepted if answers.pop( 0 ) else QW.QDialog.DialogCode.Rejected

        shown = []

        old_yes_no = ClientGUIDialogsQuick.GetYesNo
        old_information = ClientGUIDialogsMessage.ShowInformation
        old_path = os.environ[ 'PATH' ]

        ClientGUIDialogsQuick.GetYesNo = get_yes_no
        ClientGUIDialogsMessage.ShowInformation = lambda parent, message, *args, **kwargs: shown.append( message )

        try:

            # Add: the editor on a brand new call, and what accepting it does
            from hydrus.client.executables import ClientExecutableCallables as C

            editor = P.EditClientExecutableCallablePanel( c.gui, C.ClientExecutableCallable( 'new call' ) )

            answers[:] = [ False ]
            asked.clear()
            declined = editor.UserIsOKToOK()
            asked_when_declined = list( asked )

            answers[:] = [ True ]
            asked.clear()
            accepted = editor.UserIsOKToOK()

            listing = P.ExternalProgramsPanel( c.gui, c.new_options.Duplicate() )
            existing = [ x.GetName() for x in listing._external_calls.GetData() ]

            keys = set()
            names_after = []

            for i in range( 2 ):

                value = P.EditClientExecutableCallablePanel( c.gui, C.ClientExecutableCallable( 'new call' ) ).GetValue()

                listing._AddCallableFullyFormed( value )

                added = [ x for x in listing._external_calls.GetData() if x.GetName().startswith( 'new call' ) ]

                names_after.append( sorted( x.GetName() for x in added ) )

                keys = { x.GetCallableKey() for x in added }

            out[ 'add' ] = {
                'name': editor._name.text(),
                'job': editor._pipeline_type.currentText(),
                'description': editor._pipeline_description.text(),
                'validity': editor._validity_text.text(),
                'ok_declined': [ declined, asked_when_declined ],
                'ok_accepted': [ accepted, list( asked ) ],
                'existing_names': existing,
                'names_after_each_add': names_after,
                'keys_unique': len( keys ) == 2,
                'row': listing._ConvertCallableToDisplayTuple( [ x for x in listing._external_calls.GetData() if x.GetName() == 'new call' ][0] ),
            }

            # the executable path questions
            out[ 'command' ] = []

            for ( path, scripted ) in [
                ( '', [ False ] ), ( '', [ True, False ] ), ( '', [ True, True ] ),
                ( 'no-such-program-anywhere', [ False ] ), ( 'no-such-program-anywhere', [ True ] ),
                ( 'sh', [] ),
            ]:

                panel = P.EditProcessCallExecutableAndParametersPanel( c.gui, path, [ '-c', '%path%' ] )

                answers[:] = list( scripted )
                asked.clear()

                result = panel.UserIsOKToOK()

                out[ 'command' ].append( { 'path' : path, 'answers' : scripted, 'asked' : list( asked ), 'ok' : result } )

                panel.deleteLater()

            # show PATH
            os.environ[ 'PATH' ] = os.pathsep.join( [ '/synthetic/one', '/synthetic/two bin' ] )

            process = P.EditProcessCallPanel( c.gui )
            process._ShowPATH()

            out[ 'show_path' ] = { 'path' : os.environ[ 'PATH' ], 'messages' : list( shown ) }

            os.environ[ 'PATH' ] = old_path

            # the input parameter rows of "send single file" and what is vetoed
            pipeline = L.EXECUTABLE_PIPELINE_TYPE_OPEN_EXTERNALLY_SINGLE_FILE

            def call( path, templates, rules ):

                return A.ExecutableLocalProcessCall( path, templates, rules )

            def veto( process ):

                try:

                    process.CheckValid()

                except HydrusExceptions.VetoException as e:

                    return str( e )

                return None

            def rows( process ):

                return [ [ p._name.text(), p._enable.isChecked(), p._replacement_string.text() ] for p in process._parameter_types_to_input_parameter_processing_rule_panels.values() ]

            process = P.EditProcessCallPanel( c.gui )
            process.SetValue( call( 'prog', [ '%path%' ], [ A.LocalProcessCallInputParameterProcessingRule( 0 ) ] ), pipeline )

            rules = { 'rows' : rows( process ), 'veto' : veto( process ), 'steps' : [] }

            panels = list( process._parameter_types_to_input_parameter_processing_rule_panels.values() )

            panels[1]._enable.click()

            rules[ 'steps' ].append( { 'do' : 'enable file URI', 'rows' : rows( process ), 'veto' : veto( process ) } )

            panels[1]._enable.click()

            rules[ 'steps' ].append( { 'do' : 'disable file URI', 'rows' : rows( process ), 'veto' : veto( process ) } )

            panels[0]._replacement_string.setText( '%renamed%' )

            rules[ 'steps' ].append( { 'do' : 'rename the file path token', 'rows' : rows( process ), 'veto' : veto( process ) } )

            panels[0]._replacement_string.setText( '%path%' )
            panels[0]._enable.click()

            rules[ 'steps' ].append( { 'do' : 'disable every input', 'rows' : rows( process ), 'veto' : veto( process ) } )

            empty = P.EditProcessCallPanel( c.gui )
            empty.SetValue( call( '', [ '%path%' ], [ A.LocalProcessCallInputParameterProcessingRule( 0 ) ] ), pipeline )

            rules[ 'steps' ].append( { 'do' : 'no executable', 'rows' : rows( empty ), 'veto' : veto( empty ) } )

            # each row's string processor button and the example values it tests with
            from hydrus.client import ClientStrings

            processor = ClientStrings.StringProcessor()
            processor.SetProcessingSteps( [ ClientStrings.StringConverter( conversions = [ ( ClientStrings.STRING_CONVERSION_PREPEND_TEXT, 'pre-' ) ] ) ] )

            rules[ 'processor_buttons' ] = []

            for panel in panels:

                before = panel._string_processing_button._edit_button.text()
                examples = list( panel._GetTestData().texts )

                panel._string_processing_button.SetValue( processor )

                rules[ 'processor_buttons' ].append( { 'parameter' : panel._name.text(), 'default_label' : before, 'label_with_a_conversion' : panel._string_processing_button._edit_button.text(), 'examples' : examples, 'step_count' : len( panel._string_processing_button.GetValue().GetProcessingSteps() ) } )

                panel._string_processing_button.SetValue( ClientStrings.StringProcessor() )

            rules[ 'pipeline' ] = L.executable_pipeline_types_to_strs[ pipeline ]
            rules[ 'parameter_names' ] = [ L.parameter_types_to_strs[ t ] for t in L.executable_pipeline_types_to_input_params[ pipeline ] ]

            out[ 'rules' ] = rules

            # the timeout control and the flags
            fresh = P.EditProcessCallPanel( c.gui )
            fresh.SetValue( call( 'prog', [ '%path%' ], [ A.LocalProcessCallInputParameterProcessingRule( 0 ) ] ), pipeline )

            def flags( process ):

                value = process.GetValue()

                return { 'timeout' : value.GetTimeout(), 'long_lived' : value.GetThisIsAPotentiallyLongLivedExternalGuy(), 'hide_terminal' : value.GetHideTerminal(), 'text' : value.GetText(), 'widget' : process._timeout.GetValue() }

            timeouts = { 'defaults' : flags( fresh ), 'entered' : [] }

            for entered in [ None, 0, 0.5, 1, 15, 59, 60, 75, 3599, 3600, 7200 ]:

                fresh._timeout.SetValue( entered )

                timeouts[ 'entered' ].append( { 'entered' : entered, **flags( fresh ) } )

            fresh._hide_terminal.click()
            fresh._text.click()

            timeouts[ 'flags_toggled' ] = flags( fresh )

            reopened = P.EditProcessCallPanel( c.gui )
            reopened.SetValue( fresh.GetValue(), pipeline )

            timeouts[ 'reopened' ] = flags( reopened )

            out[ 'timeouts' ] = timeouts

        finally:

            ClientGUIDialogsQuick.GetYesNo = old_yes_no
            ClientGUIDialogsMessage.ShowInformation = old_information
            os.environ[ 'PATH' ] = old_path

        return out

    return c.CallBlockingToQt( c.gui, work )


def main():

    import hydrus_driver
    import record_api

    if len( sys.argv ) > 1 and sys.argv[1] == '--child':

        output = sys.argv[2]

        result = hydrus_driver.run_client( record_api.unpack_fixture( 'basic' ), record )

        with open( output, 'w' ) as f:

            json.dump( result, f, ensure_ascii = False, indent = 1 )


        return

    with tempfile.TemporaryDirectory() as work:

        output = os.path.join( work, 'result.json' )

        hydrus_driver.run_in_subprocess( os.path.abspath( __file__ ), '--child', output )

        result = json.load( open( output ) )

        with open( OUT, 'w' ) as f:

            json.dump( result, f, ensure_ascii = False, indent = 1 )
            f.write( '\n' )


        print( 'wrote', OUT )


if __name__ == '__main__':

    main()

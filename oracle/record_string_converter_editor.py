#!/usr/bin/env python3
"""Record the reference's string converter editor
(`EditStringConverterPanel`) and its conversion editor (`_ConversionPanel`).

The converter editor lists its conversions numbered, each with the
example converted up to and including it (or the error: 'ERROR: Could not
apply "..." to string "...": ...'), with add, edit, delete ("Delete all
selected?"), move up and move down (one selected, not at the end), and the
example string. "add" opens the conversion editor on the conversion last
used (any editor's, kept in the options), else "append extra text", with
the example through every conversion; "edit" on the selected one, with
the example through those before it. The conversion editor has the
conversion type, and as the type has: a text (its label by type), a
number (its label and least value by type), the encoding or decoding,
the regex pattern and replacement, the date phrase link and timezones
(the offset for "Offset"), the hash function, or the easy date parser's
note; and the example, read only, and it converted. "ok" asks first
about a regex that looks like it captures a group with no replacement.

On the `basic` fixture, for each case of `CASES` (a converter to start
from, as conversions, the example the editor is given or `null`, and
steps), this makes the panel and records, after each step, its rows,
which are selected, whether move up and down are enabled, and the value;
and for each conversion editor opened, its state when opened and after
the case's edits (the type, the rows shown with their labels, the fields,
the example and its conversion), what "ok" asked, and what it gives.
Steps: `["click", row, ctrl]`, `["example", text]`, `["add", edits]`,
`["edit", edits]` (`edits` the fields to set by name, `null` to cancel;
`"ok"` the answer to "ok"'s question), `["delete"]` (answering yes),
`["up"]` and `["down"]`. Dates are kept, not run, by hydrus-rs: the
cases' date conversions are only opened and set.

Usage: QT_QPA_PLATFORM=offscreen python oracle/record_string_converter_editor.py
       (writes fixtures/string_converter_editor.json)
"""

import json
import os
import sys

HERE = os.path.dirname( os.path.abspath( __file__ ) )

sys.path.insert( 0, HERE )

OUT = os.path.join( HERE, 'fixtures', 'string_converter_editor.json' )

CASES = [
    { 'conversions' : [], 'example' : 'hello world', 'do' : [
        [ 'add', None ],
        [ 'add', {} ],
        [ 'add', { 'type' : 'prepend text', 'text' : '>> ' } ],
        [ 'add', { 'type' : 'remove text from beginning of string', 'number' : 3 } ],
        [ 'add', { 'type' : 'take the end of the string', 'number' : 4 } ],
        [ 'add', { 'type' : 'reverse text' } ],
        [ 'add', { 'type' : 'encode', 'encoding' : 'base64 (utf-8)' } ],
        [ 'add', { 'type' : 'decode', 'decoding' : 'hex (utf-8)' } ],
        [ 'click', 5, False ],
        [ 'delete' ],
        [ 'click', 0, False ],
        [ 'click', 2, True ],
        [ 'delete' ],
        [ 'click', 0, False ],
        [ 'down' ],
        [ 'down' ],
        [ 'down' ],
        [ 'up' ],
        [ 'example', 'goodbye' ],
        [ 'click', 1, False ],
        [ 'edit', { 'type' : 'take the start of the string', 'number' : 2 } ],
        [ 'edit', None ],
    ] },
    { 'conversions' : [ [ 11, -5 ], [ 9, [ r'(\d+)', '' ] ], [ 15, [ 'ab', 3 ] ] ], 'example' : '41', 'do' : [
        [ 'click', 0, False ],
        [ 'edit', { 'number' : -7 } ],
        [ 'edit', { 'type' : 'remove text from end of string' } ],
        [ 'edit', { 'type' : 'integer addition', 'number' : 10 } ],
        [ 'click', 1, False ],
        [ 'edit', { 'ok' : False } ],
        [ 'edit', { 'pattern' : '(?:x)', 'ok' : True } ],
        [ 'edit', { 'pattern' : r'(\d)', 'replacement' : r'<\1>', 'ok' : True } ],
        [ 'edit', { 'pattern' : r'(\d)', 'replacement' : '', 'ok' : True } ],
        [ 'click', 2, False ],
        [ 'edit', { 'type' : 'append random text' } ],
        [ 'edit', { 'type' : 'get hash of string', 'hash' : 'sha1' } ],
        [ 'edit', { 'type' : 'datestring to timestamp (easy)' } ],
        [ 'edit', { 'type' : 'datestring to timestamp (advanced)', 'text' : '%Y-%m-%d', 'timezone_decode' : 'Offset', 'offset' : 3600 } ],
        [ 'edit', { 'type' : 'timestamp to datestring', 'text' : '%Y', 'timezone_encode' : 'Local' } ],
        [ 'example', 'abc' ],
        [ 'add', {} ],
        [ 'click', 0, False ],
        [ 'edit', { 'type' : 'decode', 'decoding' : 'base64url (utf-8)' } ],
        [ 'edit', { 'decoding' : 'url percent encoding' } ],
        [ 'edit', { 'type' : 'encode', 'encoding' : 'html entities' } ],
        [ 'edit', { 'encoding' : 'unicode escape characters' } ],
    ] },
    { 'conversions' : [ [ 2, 'x' ] ], 'example' : None, 'do' : [
        [ 'add', { 'type' : 'append random text', 'text' : '', 'number' : 0 } ],
    ] },
]


def record( session ):

    controller = session.controller
    gui = controller.gui

    def f():

        from qtpy import QtWidgets as QW

        from hydrus.client import ClientStrings
        from hydrus.client.gui import ClientGUIDialogsQuick
        from hydrus.client.gui import ClientGUIStringPanels as M
        from hydrus.core import HydrusExceptions

        said = []
        opened = []
        edits_now = []

        # the last conversion used, as the options keep it (none at first)
        raw = {}

        get_raw = controller.new_options.GetRawSerialisable
        set_raw = controller.new_options.SetRawSerialisable

        def get_raw_serialisable( name ):

            if name == 'last_used_string_conversion_step':

                return raw[ name ]


            return get_raw( name )


        def set_raw_serialisable( name, value ):

            if name == 'last_used_string_conversion_step':

                raw[ name ] = value

                return


            set_raw( name, value )


        controller.new_options.GetRawSerialisable = get_raw_serialisable
        controller.new_options.SetRawSerialisable = set_raw_serialisable

        answers = []

        def yes_no( win, message, **kwargs ):

            said.append( { 'asked' : message } )

            if len( answers ) > 0 and not answers.pop( 0 ):

                return QW.QDialog.DialogCode.Rejected


            return QW.QDialog.DialogCode.Accepted


        ClientGUIDialogsQuick.GetYesNo = yes_no

        def as_processor( converter ):

            p = ClientStrings.StringProcessor()

            p.SetProcessingSteps( [ converter ] )

            return p.GetSerialisableTuple()


        def choice_set( choice, label ):

            i = [ choice.itemText( i ) for i in range( choice.count() ) ].index( label )

            choice.setCurrentIndex( i )


        def conversion_state( panel ):

            def label( widget, label_widget ):

                return None if widget.isHidden() else label_widget.text()


            return {
                'type' : panel._conversion_type.currentText(),
                'shown' : {
                    'text' : label( panel._data_text, panel._data_text_label ),
                    'number' : label( panel._data_number, panel._data_number_label ),
                    'encoding' : not panel._data_encoding.isHidden(),
                    'decoding' : not panel._data_decoding.isHidden(),
                    'regex' : not panel._data_regex_pattern.isHidden(),
                    'date_link' : not panel._data_date_link.isHidden(),
                    'timezone_decode' : not panel._data_timezone_decode.isHidden(),
                    'timezone_offset' : not panel._data_timezone_offset.isHidden(),
                    'timezone_encode' : not panel._data_timezone_encode.isHidden(),
                    'hash' : not panel._data_hash_function.isHidden(),
                    'dateparser' : not panel._data_dateparser_label.isHidden(),
                },
                'text' : panel._data_text.text(),
                'number' : panel._data_number.value(),
                'encoding' : panel._data_encoding.currentText(),
                'decoding' : panel._data_decoding.currentText(),
                'pattern' : panel._data_regex_pattern.GetValue(),
                'replacement' : panel._data_regex_repl.text(),
                'timezone_decode' : panel._data_timezone_decode.currentText(),
                'offset' : panel._data_timezone_offset.value(),
                'timezone_encode' : panel._data_timezone_encode.currentText(),
                'hash' : panel._data_hash_function.currentText(),
                'example' : panel._example_string.text(),
                'result' : panel._example_conversion.text(),
                'value' : as_processor( ClientStrings.StringConverter( conversions = [ panel.GetValue() ] ) ),
            }


        def set_fields( panel, edits ):

            for ( k, v ) in edits.items():

                if k == 'type':

                    choice_set( panel._conversion_type, v )

                elif k == 'text':

                    panel._data_text.setText( v )

                elif k == 'number':

                    panel._data_number.setValue( v )

                elif k == 'encoding':

                    choice_set( panel._data_encoding, v )

                elif k == 'decoding':

                    choice_set( panel._data_decoding, v )

                elif k == 'pattern':

                    panel._data_regex_pattern.SetValue( v )

                elif k == 'replacement':

                    panel._data_regex_repl.setText( v )

                elif k == 'timezone_decode':

                    choice_set( panel._data_timezone_decode, v )

                elif k == 'offset':

                    panel._data_timezone_offset.setValue( v )

                elif k == 'timezone_encode':

                    choice_set( panel._data_timezone_encode, v )

                elif k == 'hash':

                    choice_set( panel._data_hash_function, v )



        class Dialog( QW.QWidget ):

            def __init__( self, win, title, **kwargs ):

                super().__init__( gui )

                said.append( { 'dialog' : title } )

                self._panel = None


            def __enter__( self ):

                return self


            def __exit__( self, *args ):

                return False


            def SetPanel( self, panel ):

                self._panel = panel


            def exec( self ):

                panel = self._panel

                edits = edits_now.pop( 0 )

                record = { 'opened' : conversion_state( panel ) }

                opened.append( record )

                if edits is None:

                    return QW.QDialog.DialogCode.Rejected


                edits = dict( edits )

                answer = edits.pop( 'ok', None )

                set_fields( panel, edits )

                record[ 'edited' ] = conversion_state( panel )

                if answer is not None:

                    answers.append( answer )


                ok = panel.UserIsOKToOK()

                del answers[:]

                record[ 'ok' ] = ok

                return QW.QDialog.DialogCode.Accepted if ok else QW.QDialog.DialogCode.Rejected



        M.ClientGUITopLevelWindowsPanels.DialogEdit = Dialog

        out = []

        for case in CASES:

            raw.clear()

            converter = ClientStrings.StringConverter( conversions = [ tuple( c ) if not isinstance( c[ 1 ], list ) else ( c[ 0 ], tuple( c[ 1 ] ) ) for c in case[ 'conversions' ] ], example_string = 'the converter\'s own' )

            panel = M.EditStringConverterPanel( gui, converter, example_string_override = case[ 'example' ] )

            ctrl = panel._conversions

            def state():

                model = ctrl.model()

                rows = [ [ model.data( model.index( r, c ) ) for c in range( model.columnCount() ) ] for r in range( model.rowCount() ) ]

                selected = sorted( i.row() for i in ctrl.selectionModel().selectedRows() )

                return {
                    'rows' : rows,
                    'selected' : selected,
                    'can_up' : panel._CanMoveUp(),
                    'can_down' : panel._CanMoveDown(),
                    'value' : as_processor( panel.GetValue() ),
                }


            steps = [ { 'state' : state() } ]

            for step in case[ 'do' ]:

                del said[:]
                del opened[:]

                kind = step[ 0 ]

                if kind == 'click':

                    ( row, ctrl_held ) = step[ 1: ]

                    from qtpy import QtCore as QC

                    index = ctrl.model().index( row, 0 )

                    flags = QC.QItemSelectionModel.SelectionFlag.Rows | ( QC.QItemSelectionModel.SelectionFlag.Toggle if ctrl_held else QC.QItemSelectionModel.SelectionFlag.ClearAndSelect )

                    ctrl.selectionModel().select( index, flags )

                elif kind == 'example':

                    panel._example_string.setText( step[ 1 ] )

                elif kind == 'add':

                    edits_now.append( step[ 1 ] )

                    panel._AddConversion()

                elif kind == 'edit':

                    edits_now.append( step[ 1 ] )

                    panel._EditConversion()

                elif kind == 'delete':

                    panel._DeleteConversion()

                elif kind == 'up':

                    if panel._CanMoveUp():

                        panel._MoveUp()


                elif kind == 'down':

                    if panel._CanMoveDown():

                        panel._MoveDown()



                del edits_now[:]

                steps.append( { 'do' : step, 'said' : list( said ), 'opened' : list( opened ), 'state' : state() } )


            out.append( { 'conversions' : case[ 'conversions' ], 'example' : case[ 'example' ], 'converter' : as_processor( converter ), 'steps' : steps } )


        return { 'cases' : out }


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

        path = os.path.join( work, 'string_converter_editor.json' )

        hydrus_driver.run_in_subprocess( os.path.abspath( __file__ ), '--child', path )

        with open( path ) as f:

            result = json.load( f )



    with open( OUT, 'w' ) as f:

        json.dump( result, f, indent = 1, ensure_ascii = False )
        f.write( '\n' )


    print( f'wrote {OUT}: {len( result[ "cases" ] )} cases' )


if __name__ == '__main__':

    main()

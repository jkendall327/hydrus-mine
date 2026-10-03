#!/usr/bin/env python3
"""Record the reference's string processor editor
(`EditStringProcessorPanel`) and the step editors it opens for a
splitter, joiner, sorter and selector/slicer.

The editor lists its steps (`ToString( with_type = True )`) with up,
delete ("Remove 2 selected?"), down, add ("Which type of processing
step?") and edit; under them, the starting strings and those strings
processed, and a single example (the first starting string, or the one
clicked, or as typed) through each step but the slicers, a tab each
("splitter (3)"), stopping after a step with no results ("no results").
A new step's editor is given examples from the steps before its place.

On the `basic` fixture, for each case of `CASES` (a processor to start
from, as steps, and the starting strings), this makes the panel and
records, after each step, its rows, which are selected, the processed
strings, the example and its tabs, and the value; and for each step
editor opened, what it was given (its example string or strings) and, for
the four ported here, its fields and results when opened, after the case's
edits, and what "ok" gives (or says). Steps: `["click", row, ctrl]`,
`["text", index]` (a starting string clicked), `["example", text]`,
`["up"]`, `["down"]`, `["delete"]` (answering yes), `["add", kind,
edits]` (`kind` by its button, `null` to cancel; `edits` the fields to
set, or `null` to cancel the step's editor) and `["edit", edits]`. A
match, tag filter or converter's editor is stood in for, giving back the
step it was given.

Usage: QT_QPA_PLATFORM=offscreen python oracle/record_string_processor_editor.py
       (writes fixtures/string_processor_editor.json)
"""

import json
import os
import sys

HERE = os.path.dirname( os.path.abspath( __file__ ) )

sys.path.insert( 0, HERE )

OUT = os.path.join( HERE, 'fixtures', 'string_processor_editor.json' )

TEXTS = [ 'a,b,c', 'image 10, image 2', 'x', '' ]

CASES = [
    { 'steps' : [], 'texts' : TEXTS, 'do' : [
        [ 'add', None, None ],
        [ 'add', 'String Splitter', { 'separator' : ', ' } ],
        [ 'add', 'String Sorter', { 'sort_type' : 'human sort', 'asc' : True } ],
        [ 'add', 'String Joiner', { 'joiner' : ' | ' } ],
        [ 'text', 1 ],
        [ 'example', 'q,r' ],
        [ 'add', 'String Selector/Slicer', { 'select' : 'single', 'single' : 1 } ],
        [ 'add', 'String Match', {} ],
        [ 'add', 'String Converter', {} ],
        [ 'add', 'String Tag Filter', {} ],
    ] },
    { 'steps' : [ [ 'split', ',', None ], [ 'sort', 2, False, None ], [ 'slice', 0, 2 ], [ 'join', '-', None ] ], 'texts' : TEXTS, 'do' : [
        [ 'click', 0, False ],
        [ 'edit', { 'separator' : '' } ],
        [ 'edit', { 'separator' : '\\' } ],
        [ 'edit', { 'separator' : ',', 'max_splits' : 1 } ],
        [ 'click', 1, False ],
        [ 'edit', { 'regex' : r'\d+' } ],
        [ 'edit', { 'regex' : '(' } ],
        [ 'edit', { 'sort_type' : 'reverse' } ],
        [ 'click', 2, False ],
        [ 'edit', { 'select' : 'range', 'start' : -2, 'end' : None } ],
        [ 'edit', { 'select' : 'single', 'single' : -1 } ],
        [ 'edit', { 'select' : 'range', 'start' : 3, 'end' : 1 } ],
        [ 'edit', None ],
        [ 'click', 3, False ],
        [ 'edit', { 'joiner' : '\\n', 'tuple' : 2 } ],
        [ 'edit', { 'joiner' : '\\', 'tuple' : None } ],
        [ 'edit', { 'joiner' : '+', 'tuple' : 3 } ],
    ] },
    { 'steps' : [ [ 'split', ',', None ], [ 'slice', 1, None ], [ 'join', '', None ], [ 'sort', 1, True, None ] ], 'texts' : TEXTS, 'do' : [
        [ 'click', 3, False ],
        [ 'up' ],
        [ 'click', 0, True ],
        [ 'up' ],
        [ 'down' ],
        [ 'down' ],
        [ 'down' ],
        [ 'delete' ],
        [ 'click', 0, False ],
        [ 'delete' ],
        [ 'delete' ],
    ] },
    { 'steps' : [ [ 'match', 'z' ], [ 'split', ',', None ] ], 'texts' : TEXTS, 'do' : [
        [ 'example', 'zz,y' ],
        [ 'add', 'String Joiner', {} ],
        [ 'click', 1, False ],
        [ 'edit', {} ],
    ] },
    { 'steps' : [], 'texts' : [], 'do' : [
        [ 'add', 'String Splitter', {} ],
        [ 'add', 'String Sorter', { 'sort_type' : 'strict lexicographic', 'asc' : False } ],
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
        from hydrus.client.parsing import ClientParsing
        from hydrus.core import HydrusExceptions

        said = []
        answers = []
        opened = []

        def select_from_list_buttons( win, title, choice_tuples, message = '' ):

            said.append( { 'select' : title, 'choices' : [ [ t, d_desc ] for ( t, d, d_desc ) in choice_tuples ] } )

            answer = answers.pop( 0 )

            if answer is None:

                raise HydrusExceptions.CancelledException()


            return [ d for ( t, d, d_desc ) in choice_tuples if t == answer ][0]


        def yes_no( win, message, **kwargs ):

            said.append( { 'asked' : message } )

            return QW.QDialog.DialogCode.Accepted


        ClientGUIDialogsQuick.SelectFromListButtons = select_from_list_buttons
        ClientGUIDialogsQuick.GetYesNo = yes_no

        def items( list_widget ):

            return [ list_widget.item( i ).text() for i in range( list_widget.count() ) ]


        SORTS = { 'human sort' : ClientStrings.CONTENT_PARSER_SORT_TYPE_HUMAN_SORT, 'strict lexicographic' : ClientStrings.CONTENT_PARSER_SORT_TYPE_LEXICOGRAPHIC, 'reverse' : ClientStrings.CONTENT_PARSER_SORT_TYPE_REVERSE }

        def panel_state( panel ):

            if isinstance( panel, M.EditStringSplitterPanel ):

                return { 'separator' : panel._separator.text(), 'max_splits' : panel._max_splits.GetValue(), 'example' : panel._example_string.text(), 'results' : items( panel._example_string_splits ), 'invalid' : panel._separator.objectName() == 'HydrusInvalid' }

            elif isinstance( panel, M.EditStringJoinerPanel ):

                return { 'joiner' : panel._joiner.text(), 'tuple' : panel._join_tuple_size.GetValue(), 'summary' : panel._summary_st.text(), 'texts' : items( panel._example_strings ), 'results' : items( panel._example_strings_joined ), 'invalid' : panel._joiner.objectName() == 'HydrusInvalid' }

            elif isinstance( panel, M.EditStringSlicerPanel ):

                return { 'select' : 'single' if panel._select_type.GetValue() == M.SELECT_SINGLE else 'range', 'single' : panel._index_single.value(), 'start' : panel._index_start.GetValue(), 'end' : panel._index_end.GetValue(), 'summary' : panel._summary_st.text(), 'texts' : items( panel._example_strings ), 'results' : items( panel._example_strings_sliced ) }

            elif isinstance( panel, M.EditStringSorterPanel ):

                return { 'sort_type' : panel._sort_type.currentText(), 'asc' : panel._asc.isChecked(), 'regex' : panel._regex.GetValue(), 'texts' : items( panel._example_strings ), 'results' : items( panel._example_strings_sorted ) }


        def set_fields( panel, edits ):

            for ( k, v ) in edits.items():

                if k == 'separator':

                    panel._separator.setText( v )

                elif k == 'max_splits':

                    panel._max_splits.SetValue( v )

                elif k == 'joiner':

                    panel._joiner.setText( v )

                elif k == 'tuple':

                    panel._join_tuple_size.SetValue( v )

                elif k == 'select':

                    panel._select_type.SetValue( M.SELECT_SINGLE if v == 'single' else M.SELECT_RANGE )

                elif k == 'single':

                    panel._index_single.setValue( v )

                elif k == 'start':

                    panel._index_start.SetValue( v )

                elif k == 'end':

                    panel._index_end.SetValue( v )

                elif k == 'sort_type':

                    panel._sort_type.SetValue( SORTS[ v ] )

                elif k == 'asc':

                    panel._asc.setChecked( v )

                elif k == 'regex':

                    panel._regex.SetValue( v )



        def as_processor( steps ):

            p = ClientStrings.StringProcessor()

            p.SetProcessingSteps( steps )

            return p.GetSerialisableTuple()


        class Stand:

            # a match, tag filter or converter's editor: gives back its step
            def __init__( self, kind, step, given ):

                self._step = step

                opened.append( { 'kind' : kind, 'given' : given } )


            def GetValue( self ):

                return self._step



        def stand_match( dlg, step, test_data = None ):

            return Stand( 'match', step, list( test_data.texts ) )


        def stand_tag_filter( dlg, step, test_data = None ):

            return Stand( 'tag filter', step, list( test_data.texts ) )


        def stand_converter( dlg, step, example_string_override = None ):

            return Stand( 'converter', step, example_string_override )


        M.EditStringMatchPanel = stand_match
        M.EditStringTagFilterPanel = stand_tag_filter
        M.EditStringConverterPanel = stand_converter

        edits_now = []

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

                if isinstance( panel, Stand ):

                    return QW.QDialog.DialogCode.Accepted if edits is not None else QW.QDialog.DialogCode.Rejected


                record = { 'opened' : panel_state( panel ) }

                opened.append( record )

                if edits is None:

                    return QW.QDialog.DialogCode.Rejected


                set_fields( panel, edits )

                record[ 'edited' ] = panel_state( panel )

                try:

                    record[ 'value' ] = as_processor( [ panel.GetValue() ] )

                except HydrusExceptions.VetoException as e:

                    record[ 'veto' ] = str( e )

                    raise


                return QW.QDialog.DialogCode.Accepted



        M.ClientGUITopLevelWindowsPanels.DialogEdit = Dialog

        def make_step( s ):

            if s[ 0 ] == 'split':

                return ClientStrings.StringSplitter( separator = s[ 1 ], max_splits = s[ 2 ] )

            elif s[ 0 ] == 'sort':

                return ClientStrings.StringSorter( sort_type = s[ 1 ], asc = s[ 2 ], regex = s[ 3 ] )

            elif s[ 0 ] == 'slice':

                return ClientStrings.StringSlicer( index_start = s[ 1 ], index_end = s[ 2 ] )

            elif s[ 0 ] == 'join':

                return ClientStrings.StringJoiner( joiner = s[ 1 ], join_tuple_size = s[ 2 ] )

            elif s[ 0 ] == 'match':

                return ClientStrings.StringMatch( match_type = ClientStrings.STRING_MATCH_REGEX, match_value = s[ 1 ], example_string = 'z' )



        out = []

        for case in CASES:

            processor = ClientStrings.StringProcessor()
            processor.SetProcessingSteps( [ make_step( s ) for s in case[ 'steps' ] ] )

            test_data = ClientParsing.ParsingTestData( {}, tuple( case[ 'texts' ] ) )

            panel = M.EditStringProcessorPanel( gui, processor, test_data )

            box = panel._processing_steps._listbox

            def state():

                tabs = panel._single_test_panel._example_results

                return {
                    'rows' : items( box ),
                    'selected' : [ i for i in range( box.count() ) if box.item( i ).isSelected() ],
                    'processed' : items( panel._multiline_test_panel._result_data ),
                    'example' : panel._single_test_panel._example_string.text(),
                    'tabs' : [ [ tabs.tabText( i ), items( tabs.widget( i ) ) ] for i in range( tabs.count() ) ],
                    'value' : panel.GetValue().GetSerialisableTuple(),
                }


            steps = [ { 'state' : state() } ]

            for step in case[ 'do' ]:

                del said[:]
                del opened[:]

                kind = step[ 0 ]

                if kind == 'click':

                    ( row, ctrl ) = step[ 1: ]

                    if not ctrl:

                        box.clearSelection()


                    item = box.item( row )

                    item.setSelected( not item.isSelected() if ctrl else True )

                elif kind == 'text':

                    test_list = panel._multiline_test_panel._test_data

                    test_list.clearSelection()
                    test_list.item( step[ 1 ] ).setSelected( True )

                elif kind == 'example':

                    panel._single_test_panel._example_string.setText( step[ 1 ] )

                elif kind == 'up':

                    panel._processing_steps._Up()

                elif kind == 'down':

                    panel._processing_steps._Down()

                elif kind == 'delete':

                    panel._processing_steps._Delete()

                elif kind == 'add':

                    answers.append( step[ 1 ] )
                    edits_now.append( step[ 2 ] )

                    try:

                        panel._processing_steps._Add()

                    except HydrusExceptions.VetoException:

                        pass


                elif kind == 'edit':

                    edits_now.append( step[ 1 ] )

                    try:

                        panel._processing_steps._Edit()

                    except HydrusExceptions.VetoException:

                        pass



                del answers[:]
                del edits_now[:]

                steps.append( { 'do' : step, 'said' : list( said ), 'opened' : list( opened ), 'state' : state() } )


            out.append( { 'processor' : processor.GetSerialisableTuple(), 'texts' : case[ 'texts' ], 'steps' : steps } )


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

        path = os.path.join( work, 'string_processor_editor.json' )

        hydrus_driver.run_in_subprocess( os.path.abspath( __file__ ), '--child', path )

        with open( path ) as f:

            result = json.load( f )



    with open( OUT, 'w' ) as f:

        json.dump( result, f, indent = 1, ensure_ascii = False )
        f.write( '\n' )


    print( f'wrote {OUT}: {len( result[ "cases" ] )} cases' )


if __name__ == '__main__':

    main()

#!/usr/bin/env python3
"""Record the reference's sidecar editors: the source (importer) editor,
the destination (exporter) widget, the router editor and the routers list
(`ClientGUIMetadataMigration*`), as the import folder, "filename tagging"
and export folder dialogs open them.

There are two contexts. An import (import folders, filename tagging) reads
.txt and .json sidecars into a file's tags, notes, urls or timestamps; an
export (export folders) writes those to sidecars.

On the `basic` fixture, for each case of `CASES` (an editor, its context,
what it starts on, then steps), this makes the editor and records, after
each step, what it shows and its value:

- a source or destination: its "change type" button's label; which of its
  boxes show; the sidecar filename box (remove .ext, suffix, the
  filename conversion button's label, the test path and the resulting
  sidecar path); the .txt separator (choice, custom text, whether custom
  is editable); the tag service and display type; the timestamp type
  (its choices and value, and whichever of file service, deleted file
  service, viewer or domain shows, with its choices); for a destination,
  the forced note name and the JSON object names; the processing
  button's label; and the value (`GetValue`: its `ToString` and
  serialised form).
- a router: its sources' labels, its processing label, its destination as
  above, and the questions "ok" asks.
- the routers list: the label of each router, and the templates menu (its
  entries, and the routers each adds).

Steps: `["change_type", label]` (recording the "Which type?" question),
`["suffix", text]`, `["remove_ext", on]`, `["example", path]`,
`["separator", label, custom]`, `["display", label]`, `["service",
name]` (recording the "select service" question), `["timestamp_type",
label]`, `["file_service", name]`, `["deleted_service", name]`,
`["canvas", label]`, `["domain", text]`, `["forced_name", text or
null]`, `["nested", [names]]`, and for a router `["ok"]`.

Usage: QT_QPA_PLATFORM=offscreen python oracle/record_sidecar_editors.py
       (writes fixtures/sidecar_editors.json)
"""

import json
import os
import sys

HERE = os.path.dirname( os.path.abspath( __file__ ) )

sys.path.insert( 0, HERE )

OUT = os.path.join( HERE, 'fixtures', 'sidecar_editors.json' )

CASES = [
    # an import's sources: .txt, then .json, keeping the naming
    { 'editor' : 'importer', 'context' : 'import', 'start' : 'txt', 'steps' : [
        [ 'suffix', 'tags' ],
        [ 'remove_ext', True ],
        [ 'example', 'folder/my_image.jpg' ],
        [ 'separator', 'four pipes (||||)', None ],
        [ 'separator', 'custom text', ', ' ],
        [ 'change_type', 'a .json sidecar' ],
        [ 'change_type', 'a .txt sidecar' ],
        [ 'example', 'noext' ],
    ] },
    { 'editor' : 'importer', 'context' : 'import', 'start' : 'json', 'steps' : [
    ] },
    # an export's sources: a file's tags, timestamps, notes, urls
    { 'editor' : 'importer', 'context' : 'export', 'start' : 'tags', 'steps' : [
        [ 'display', 'display tags' ],
        [ 'service', 'second tags' ],
        [ 'change_type', 'a file\'s timestamps' ],
        [ 'timestamp_type', 'imported time' ],
        [ 'file_service', 'trash' ],
        [ 'timestamp_type', 'deleted time' ],
        [ 'timestamp_type', 'last viewed time' ],
        [ 'canvas', 'preview viewer' ],
        [ 'timestamp_type', 'domain modified time' ],
        [ 'domain', 'example.com' ],
        [ 'timestamp_type', 'aggregate modified time' ],
        [ 'change_type', 'a file\'s notes' ],
        [ 'change_type', 'a file\'s tags' ],
    ] },
    # an import's destinations
    { 'editor' : 'exporter', 'context' : 'import', 'start' : 'tags', 'steps' : [
        [ 'service', 'downloader tags' ],
        [ 'change_type', 'a file\'s notes' ],
        [ 'forced_name', 'comment' ],
        [ 'change_type', 'a file\'s timestamps' ],
        [ 'timestamp_type', 'imported time' ],
        [ 'change_type', 'a file\'s URLs' ],
    ] },
    # an export's destinations
    { 'editor' : 'exporter', 'context' : 'export', 'start' : 'txt', 'steps' : [
        [ 'suffix', 'urls' ],
        [ 'separator', 'custom text', ',' ],
        [ 'change_type', 'a .json sidecar' ],
        [ 'nested', [ 'meta', 'tags' ] ],
        [ 'change_type', 'a .txt sidecar' ],
    ] },
    # new routers, and what "ok" asks of notes split by newlines
    { 'editor' : 'router', 'context' : 'import', 'start' : 'new', 'steps' : [
    ] },
    { 'editor' : 'router', 'context' : 'export', 'start' : 'new', 'steps' : [
    ] },
    { 'editor' : 'router', 'context' : 'import', 'start' : 'txt notes', 'steps' : [
        [ 'ok' ],
    ] },
    { 'editor' : 'router', 'context' : 'export', 'start' : 'notes txt', 'steps' : [
        [ 'ok' ],
    ] },
    # the lists, with their templates
    { 'editor' : 'routers', 'context' : 'import', 'start' : 'empty', 'steps' : [
    ] },
    { 'editor' : 'routers', 'context' : 'export', 'start' : 'empty', 'steps' : [
    ] },
]


def record( session ):

    controller = session.controller
    gui = controller.gui

    def f():

        from qtpy import QtWidgets as QW

        from hydrus.client import ClientConstants as CC
        from hydrus.client import ClientStrings
        from hydrus.client.gui import ClientGUIDialogsMessage
        from hydrus.client.gui import ClientGUIDialogsQuick
        from hydrus.client.gui.metadata import ClientGUIMetadataMigration as GM
        from hydrus.client.gui.metadata import ClientGUIMetadataMigrationExporters as GE
        from hydrus.client.gui.metadata import ClientGUIMetadataMigrationImporters as GI
        from hydrus.client.gui.metadata import ClientGUIMetadataMigrationTest as GT
        from hydrus.client.metadata import ClientMetadataMigration as MM
        from hydrus.client.metadata import ClientMetadataMigrationExporters as E
        from hydrus.client.metadata import ClientMetadataMigrationImporters as I
        from hydrus.client.metadata import ClientTags

        services_manager = controller.services_manager

        said = []
        answers = []

        def select_from_list( win, title, choice_tuples, value_to_select = None, sort_tuples = True, allow_insta_one_item_select = True ):

            texts = [ t for ( t, d ) in choice_tuples ]

            if len( choice_tuples ) == 1 and allow_insta_one_item_select:

                said.append( { 'auto' : title, 'choices' : texts } )

                return choice_tuples[0][1]


            said.append( { 'select' : title, 'choices' : sorted( texts ) if sort_tuples else texts } )

            answer = answers.pop( 0 )

            return [ d for ( t, d ) in choice_tuples if t == answer ][0]


        def select_buttons( win, title, choice_tuples, message = '', **kwargs ):

            said.append( { 'buttons' : title, 'message' : message, 'choices' : [ [ t, tt ] for ( t, d, tt ) in choice_tuples ] } )

            answer = answers.pop( 0 )

            return [ d for ( t, d, tt ) in choice_tuples if t == answer ][0]


        def yes_no( win, message, **kwargs ):

            said.append( { 'asked' : message } )

            return QW.QDialog.DialogCode.Rejected


        def information( win, message, **kwargs ):

            said.append( { 'information' : message } )


        def warning( win, message, **kwargs ):

            said.append( { 'warning' : message } )


        ClientGUIDialogsQuick.SelectFromList = select_from_list
        ClientGUIDialogsQuick.SelectFromListButtons = select_buttons
        ClientGUIDialogsQuick.GetYesNo = yes_no
        ClientGUIDialogsMessage.ShowInformation = information
        ClientGUIDialogsMessage.ShowWarning = warning

        contexts = {
            'import' : (
                [ I.SingleFileMetadataImporterTXT, I.SingleFileMetadataImporterJSON ],
                [ E.SingleFileMetadataExporterMediaTags, E.SingleFileMetadataExporterMediaNotes, E.SingleFileMetadataExporterMediaURLs, E.SingleFileMetadataExporterMediaTimestamps ],
                lambda: GT.MigrationTestContextFactorySidecar( [] ),
            ),
            'export' : (
                [ I.SingleFileMetadataImporterMediaTags, I.SingleFileMetadataImporterMediaNotes, I.SingleFileMetadataImporterMediaURLs, I.SingleFileMetadataImporterMediaTimestamps ],
                [ E.SingleFileMetadataExporterTXT, E.SingleFileMetadataExporterJSON ],
                lambda: GT.MigrationTestContextFactoryMedia( [] ),
            ),
        }

        def start_importer( start ):

            return {
                'txt' : lambda: I.SingleFileMetadataImporterTXT(),
                'json' : lambda: I.SingleFileMetadataImporterJSON(),
                'tags' : lambda: I.SingleFileMetadataImporterMediaTags( service_key = CC.DEFAULT_LOCAL_TAG_SERVICE_KEY ),
            }[ start ]()


        def start_exporter( start ):

            return {
                'txt' : lambda: E.SingleFileMetadataExporterTXT(),
                'tags' : lambda: E.SingleFileMetadataExporterMediaTags(),
            }[ start ]()


        def start_router( start ):

            return {
                'txt notes' : lambda: MM.SingleFileMetadataRouter( importers = [ I.SingleFileMetadataImporterTXT() ], exporter = E.SingleFileMetadataExporterMediaNotes() ),
                'notes txt' : lambda: MM.SingleFileMetadataRouter( importers = [ I.SingleFileMetadataImporterMediaNotes() ], exporter = E.SingleFileMetadataExporterTXT() ),
            }[ start ]()


        def choice( c ):

            return {
                'options' : [ c.itemText( i ) for i in range( c.count() ) ],
                'value' : c.currentText(),
            }


        def node_state( w, sidecar_panel, separator_panel, stub, extra ):

            s = {
                'type' : w._change_type_button.text(),
                'sidecar' : None if sidecar_panel.isHidden() else {
                    'remove_ext' : sidecar_panel._remove_actual_filename_ext.isChecked(),
                    'suffix' : sidecar_panel._suffix.text(),
                    'converter' : sidecar_panel._filename_string_converter.text(),
                    'example' : sidecar_panel._example_input.text(),
                    'result' : sidecar_panel._example_output.text(),
                },
                'separator' : None if separator_panel.isHidden() else {
                    'choice' : choice( separator_panel._choice ),
                    'custom' : separator_panel._custom_input.text(),
                    'custom_enabled' : separator_panel._custom_input.isEnabled(),
                },
                'timestamp' : None if w._timestamp_data_stub_panel.isHidden() else {
                    'type' : choice( stub._timestamp_type ),
                    'file_service' : None if stub._current_file_service.isHidden() else choice( stub._current_file_service ),
                    'deleted_service' : None if stub._deleted_file_service.isHidden() else choice( stub._deleted_file_service ),
                    'canvas' : None if stub._canvas_type.isHidden() else choice( stub._canvas_type ),
                    'domain' : None if stub._domain_panel.isHidden() else stub._domain.text(),
                },
            }

            s.update( extra )

            try:

                value = w.GetValue()

                s[ 'value' ] = { 'text' : value.ToString(), 'object' : value.GetSerialisableTuple() }

            except Exception as e:

                s[ 'value' ] = { 'error' : str( e ) }


            return s


        def importer_state( w ):

            return node_state( w, w._sidecar_panel, w._txt_separator_panel, w._timestamp_data_stub, {
                'tags' : None if w._tag_service_panel.isHidden() else {
                    'service' : w._service_selection_button.text(),
                    'display' : w._tag_display_type_button.text(),
                },
                'json_formula' : not w._json_parsing_formula_panel.isHidden(),
                'processing' : w._string_processor_button._edit_button.text(),
            } )


        def exporter_state( w ):

            return node_state( w, w._sidecar_panel, w._txt_separator_panel, w._timestamp_data_stub, {
                'tags' : None if w._service_selection_panel.isHidden() else {
                    'service' : w._service_selection_button.text(),
                },
                'forced_name' : None if w._forced_note_name_panel.isHidden() else w._forced_note_name.GetValue(),
                'nested' : None if w._nested_object_names_panel.isHidden() else w._nested_object_names_list.GetData(),
            } )


        def router_state( p ):

            return {
                'sources' : [ i.ToString() for i in p._importers_list.GetData() ],
                'processing' : p._string_processor_button._edit_button.text(),
                'destination' : exporter_state( p._exporter_widget ),
            }


        def step_node( w, sidecar_panel, separator_panel, stub, step ):

            ( kind, *args ) = step

            if kind == 'change_type':

                answers.append( args[ 0 ] )

                w._ChangeType()

            elif kind == 'suffix':

                sidecar_panel._suffix.setText( args[ 0 ] )

            elif kind == 'remove_ext':

                sidecar_panel._remove_actual_filename_ext.setChecked( args[ 0 ] )
                sidecar_panel._UpdateExample()

            elif kind == 'example':

                sidecar_panel._example_input.setText( args[ 0 ] )

            elif kind == 'separator':

                c = separator_panel._choice
                c.setCurrentIndex( [ c.itemText( i ) for i in range( c.count() ) ].index( args[ 0 ] ) )

                if args[ 1 ] is not None:

                    separator_panel._custom_input.setText( args[ 1 ] )


            elif kind == 'display':

                b = w._tag_display_type_button
                b.SetValue( [ d for ( t, d ) in [ ( ClientTags.tag_display_str_lookup[ x ], x ) for x in ( ClientTags.TAG_DISPLAY_STORAGE, ClientTags.TAG_DISPLAY_DISPLAY_ACTUAL ) ] if t == args[ 0 ] ][0] )

            elif kind == 'service':

                answers.append( args[ 0 ] )

                w._SelectService()

            elif kind in ( 'timestamp_type', 'file_service', 'deleted_service', 'canvas' ):

                c = { 'timestamp_type' : stub._timestamp_type, 'file_service' : stub._current_file_service, 'deleted_service' : stub._deleted_file_service, 'canvas' : stub._canvas_type }[ kind ]

                c.setCurrentIndex( [ c.itemText( i ) for i in range( c.count() ) ].index( args[ 0 ] ) )

            elif kind == 'domain':

                stub._domain.setText( args[ 0 ] )

            elif kind == 'forced_name':

                w._forced_note_name.SetValue( args[ 0 ] )

            elif kind == 'nested':

                w._nested_object_names_list.Clear()
                w._nested_object_names_list.AddDatas( args[ 0 ] )



        out = []

        for case in CASES:

            ( allowed_importers, allowed_exporters, factory ) = contexts[ case[ 'context' ] ]

            editor = case[ 'editor' ]

            if editor == 'importer':

                w = GI.EditSingleFileMetadataImporterPanel( gui, start_importer( case[ 'start' ] ), allowed_importers, factory() )

                state = lambda: importer_state( w )
                do = lambda step: step_node( w, w._sidecar_panel, w._txt_separator_panel, w._timestamp_data_stub, step )

            elif editor == 'exporter':

                w = GE.EditSingleFileMetadataExporterWidget( gui, start_exporter( case[ 'start' ] ), allowed_exporters )

                state = lambda: exporter_state( w )
                do = lambda step: step_node( w, w._sidecar_panel, w._txt_separator_panel, w._timestamp_data_stub, step )

            elif editor == 'router':

                if case[ 'start' ] == 'new':

                    # as the list's "add" makes one
                    exporter = allowed_exporters[0]()

                    router = MM.SingleFileMetadataRouter( exporter = exporter )

                else:

                    router = start_router( case[ 'start' ] )


                w = GM.EditSingleFileMetadataRouterPanel( gui, router, allowed_importers, allowed_exporters, factory() )

                state = lambda: router_state( w )

                def do( step ):

                    if step[ 0 ] == 'ok':

                        said.append( { 'ok' : w.UserIsOKToOK() } )



            else:

                w = GM.SingleFileMetadataRoutersControl( gui, [], allowed_importers, allowed_exporters, factory() )

                def state():

                    templates = []

                    if hasattr( w, '_templates_button' ):

                        for item in w._templates_button._menu_template_items:

                            routers = item.args[ 0 ]

                            templates.append( {
                                'label' : item.title,
                                'description' : item.description,
                                'routers' : [ { 'text' : r.ToString( pretty = True ), 'object' : r.GetSerialisableTuple() } for r in routers ],
                            } )



                    return { 'labels' : [ GM.convert_router_to_pretty_string( r ) for r in w.GetData() ], 'templates' : templates }


                do = lambda step: None


            states = [ { 'step' : None, 'state' : state() } ]

            for step in case[ 'steps' ]:

                del said[:]
                del answers[:]

                do( step )

                states.append( { 'step' : step, 'said' : list( said ), 'state' : state() } )


            out.append( { 'editor' : editor, 'context' : case[ 'context' ], 'start' : case[ 'start' ], 'states' : states } )

            w.deleteLater()


        names = { s.GetServiceKey().hex() : s.GetName() for s in services_manager.GetServices() }

        return { 'names' : names, 'cases' : out }


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

        path = os.path.join( work, 'sidecar_editors.json' )

        hydrus_driver.run_in_subprocess( os.path.abspath( __file__ ), '--child', path )

        with open( path ) as f:

            result = json.load( f )



    with open( OUT, 'w' ) as f:

        json.dump( result, f, indent = 1, ensure_ascii = False )
        f.write( '\n' )


    print( f'wrote {OUT}: {len( result[ "cases" ] )} cases' )


if __name__ == '__main__':

    main()

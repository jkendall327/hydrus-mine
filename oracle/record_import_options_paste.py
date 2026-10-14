#!/usr/bin/env python3
"""Record the reference's import options container editor's copy and paste.

`EditSpecificImportOptionsContainerPanel` (a specific importer's options, in
the simple mode a new client has) and `EditImportOptionsContainerPanel` for
the "global" defaults are built on a container and driven through their copy
button and paste menu: copy (the text put on the clipboard), merge-paste,
fill-in-gaps-paste and replace-paste of a container held in the clipboard (a
partial one, a full one and one with every kind customised), the menu's
entries for each editor, and what is said when the clipboard holds no import
options (text that is not JSON, and JSON of another kind of object). After every
step the container the editor would give back (`GetValue`) is recorded, as
the reference's tuple with its dictionaries sorted.

Usage: QT_QPA_PLATFORM=offscreen python oracle/record_import_options_paste.py
       (writes fixtures/import_options_paste.json)
"""

import json
import os
import sys

HERE = os.path.dirname( os.path.abspath( __file__ ) )

sys.path.insert( 0, HERE )

OUT = os.path.join( HERE, 'fixtures', 'import_options_paste.json' )


def record( session ):

    from record_subscription_import_options import normalise

    from hydrus.core import HydrusConstants as HC
    from hydrus.core import HydrusTags
    from hydrus.client.gui import ClientGUIDialogsMessage
    from hydrus.client.gui import ClientGUIDialogsQuick
    from hydrus.client.gui.importing import ClientGUIImportOptionsContainer as O
    from hydrus.client.importing.options import ExternalProgramsImportOptions as E
    from hydrus.client.importing.options import FileFilteringImportOptions as FF
    from hydrus.client.importing.options import ImportOptionsConstants as IOC
    from hydrus.client.importing.options import ImportOptionsContainer as C
    from hydrus.client.importing.options import LocationImportOptions as L
    from hydrus.client.importing.options import NoteImportOptions as N
    from hydrus.client.importing.options import PrefetchImportOptions as PF
    from hydrus.client.importing.options import PresentationImportOptions as PR
    from hydrus.client.importing.options import TagFilteringImportOptions as TF
    from hydrus.client.importing.options import TagImportOptions as T

    controller = session.controller
    gui = controller.gui

    def qt( f ):

        return controller.CallBlockingToQt( gui, f )


    def drive():

        constructors = [ PF.PrefetchImportOptions, FF.FileFilteringImportOptions, TF.TagFilteringImportOptions, L.LocationImportOptions, T.TagImportOptions, N.NoteImportOptions, PR.PresentationImportOptions, E.ExternalProgramsImportOptions ]

        full = C.ImportOptionsContainer()

        for constructor in constructors:

            full.SetImportOptions( constructor() )


        # (the same containers as record_subscription_import_options.py)
        prefetch = PF.PrefetchImportOptions()
        prefetch._preimport_hash_check_type = 0
        prefetch._fetch_metadata_even_if_url_recognised_and_file_already_in_db = True
        notes = N.NoteImportOptions()
        notes._get_notes = False
        notes._conflict_resolution = 0
        notes._all_name_override = 'synthetic'
        notes._names_to_name_overrides = { 'old' : 'new' }
        presentation = PR.PresentationImportOptions()
        presentation._presentation_status = 2
        presentation._presentation_inbox = 1

        existing = C.ImportOptionsContainer()
        existing.SetImportOptions( PF.PrefetchImportOptions() )
        existing.SetImportOptions( notes )

        incoming = C.ImportOptionsContainer()
        incoming.SetImportOptions( prefetch )
        incoming.SetImportOptions( presentation )

        file_filter = FF.FileFilteringImportOptions()
        file_filter._min_size = 1234
        file_filter._max_resolution = ( 2048, 4096 )

        every = full.Duplicate()
        every.SetImportOptions( prefetch )
        every.SetImportOptions( file_filter )
        every.SetImportOptions( notes )
        every.SetImportOptions( presentation )

        containers = { 'existing' : existing, 'incoming' : incoming, 'full' : full, 'every kind customised' : every }

        def dump( container ):

            return normalise( json.loads( container.DumpToString() ) )


        clipboard = { 'text' : '' }
        copied = []
        informations = []
        criticals = []
        parse_errors = []

        old_pub = controller.pub

        def pub( *args, **kwargs ):

            if args[ :2 ] == ( 'clipboard', 'text' ):

                copied.append( args[ 2 ] )

            else:

                old_pub( *args, **kwargs )


        controller.pub = pub
        controller.GetClipboardText = lambda: clipboard[ 'text' ]

        ClientGUIDialogsMessage.ShowInformation = lambda win, message: informations.append( message )
        ClientGUIDialogsMessage.ShowCritical = lambda win, title, message: criticals.append( { 'title' : title, 'message' : message } )

        manager = controller.import_options_manager

        def specific_panel( current ):

            return O.EditSpecificImportOptionsContainerPanel( gui, manager, IOC.IMPORT_OPTIONS_CALLER_TYPE_POST_URLS, current.Duplicate() )


        def global_panel( current ):

            return O.EditImportOptionsContainerPanel( gui, manager, IOC.IMPORT_OPTIONS_CALLER_TYPE_GLOBAL, current.Duplicate(), simple_mode = False )


        def menu_entries( panel ):

            menu = panel._paste_button._menu_template_items

            out = []

            for item in menu:

                title = item.GetTitle() if hasattr( item, 'GetTitle' ) else None

                out.append( title )


            return out


        out = { 'containers' : { name : dump( c ) for ( name, c ) in containers.items() }, 'simple_mode' : controller.new_options.GetBoolean( 'import_options_simple_mode' ) }

        specific = []

        for current_name in [ 'existing', 'full' ]:

            for incoming_name in [ 'incoming', 'full', 'every kind customised' ]:

                for mode in [ 'replace-paste', 'merge-paste', 'fill-in-gaps-paste' ]:

                    panel = specific_panel( containers[ current_name ] )

                    clipboard[ 'text' ] = containers[ incoming_name ].DumpToString()

                    del informations[:]

                    { 'replace-paste' : panel._Paste, 'merge-paste' : panel._PasteMerge, 'fill-in-gaps-paste' : panel._PasteFillIn }[ mode ]()

                    specific.append( { 'current' : current_name, 'pasted' : incoming_name, 'mode' : mode, 'value' : dump( panel.GetValue() ), 'information' : list( informations ) } )



        out[ 'specific' ] = specific
        out[ 'specific_menu' ] = menu_entries( specific_panel( existing ) )

        # copy
        panel = specific_panel( existing )
        del copied[:]
        panel._Copy()
        out[ 'specific_copy' ] = { 'text' : copied[ 0 ], 'value' : dump( panel.GetValue() ) }

        panel = specific_panel( every )
        del copied[:]
        panel._Copy()
        out[ 'specific_copy_every' ] = { 'text' : copied[ 0 ], 'value' : dump( panel.GetValue() ) }

        # the global defaults
        glob = []

        for incoming_name in [ 'incoming', 'full', 'every kind customised' ]:

            for mode in [ 'merge-paste', 'replace-paste' ]:

                panel = global_panel( full )

                clipboard[ 'text' ] = containers[ incoming_name ].DumpToString()

                del informations[:]

                { 'replace-paste' : panel._Paste, 'merge-paste' : panel._PasteMerge }[ mode ]()

                glob.append( { 'pasted' : incoming_name, 'mode' : mode, 'value' : dump( panel.GetValue() ), 'information' : list( informations ) } )



        out[ 'global' ] = glob
        out[ 'global_menu' ] = menu_entries( global_panel( full ) )

        # not import options
        errors = []

        for text in [ 'not json', '[26, 3, []]', json.dumps( [ 21, 2, [] ] ), '' ]:

            panel = specific_panel( existing )

            clipboard[ 'text' ] = text

            del criticals[:]

            panel._Paste()

            errors.append( { 'text' : text, 'criticals' : list( criticals ), 'value' : dump( panel.GetValue() ) } )


        out[ 'errors' ] = errors

        return out


    return qt( drive )


def main():

    import hydrus_driver
    import record_api

    db_dir = record_api.unpack_fixture( 'basic' )

    result = hydrus_driver.run_client( db_dir, record )

    with open( OUT, 'w' ) as f:

        json.dump( result, f, indent = 1, sort_keys = True, ensure_ascii = False )
        f.write( '\n' )


    print( f'wrote {OUT}' )


if __name__ == '__main__':

    main()

#!/usr/bin/env python3
"""Record the reference's import options editor pages (`ClientGUIImportOptionsPanels`).

Four of the editor's pages are built on their own in the running client, on
the `basic` fixture, and driven as a user would drive their widgets; after each
scenario the options the page gives back (`GetValue`) and, where there is one,
what the reference then does with them are recorded:

* file filtering (`EditFileFilteringImportOptionsPanel`): the two checkboxes,
  the three byte limits (value and unit, or "no limit") and the two
  resolution limits; the resulting options and `CheckFileIsValid`'s verdict
  (its message) for a list of probe files; and how `SetValue` shows saved
  options (value and unit, and whether "no limit" is ticked);
* prefetch (`EditPrefetchImportOptionsPanel`): after every step, the choices,
  the flags, and whether the neighbour-spam box is enabled (the two
  "dispositive" choices give way to each other);
* notes (`EditNoteImportOptionsPanel`): the options and the notes
  `GetUpdateeNamesToNotes` would write for probe notes; the conflict choices'
  labels in order;
* a tag service's rules (`EditServiceTagImportOptionsPanel`, the cog menu's
  checks): the rules and the tags `ServiceTagImportOptions.GetTags` gives a
  new file, a file already in the inbox and one already archived.

Usage: QT_QPA_PLATFORM=offscreen python oracle/record_import_options_fields.py
       (writes fixtures/import_options_fields.json)
"""

import json
import os
import sys

HERE = os.path.dirname( os.path.abspath( __file__ ) )

sys.path.insert( 0, HERE )

OUT = os.path.join( HERE, 'fixtures', 'import_options_fields.json' )

UNITS = { 'B' : 1, 'KB' : 1024, 'MB' : 1024 ** 2, 'GB' : 1024 ** 3, 'TB' : 1024 ** 4 }

# ( name, ops ): ( 'exclude_deleted', bool ), ( 'bombs', bool ),
# ( 'size', 'min'|'max'|'gif', none, value, unit ), ( 'resolution', 'min'|'max', none, width, height )
FILTERING = [
    ( 'defaults', [] ),
    ( 'checkboxes off', [ ( 'exclude_deleted', False ), ( 'bombs', False ) ] ),
    ( 'the editor test', [
        ( 'exclude_deleted', False ), ( 'bombs', False ),
        ( 'size', 'min', False, 3, 'KB' ), ( 'size', 'max', True, 1, 'MB' ), ( 'size', 'gif', False, 2, 'MB' ),
        ( 'resolution', 'min', False, 640, 480 ), ( 'resolution', 'max', False, 4000, 3000 ) ] ),
    ( 'minimum above maximum', [
        ( 'size', 'min', False, 10, 'MB' ), ( 'size', 'max', False, 1, 'MB' ),
        ( 'resolution', 'min', False, 5000, 5000 ), ( 'resolution', 'max', False, 100, 100 ) ] ),
    ( 'every unit', [
        ( 'size', 'min', False, 7, 'B' ), ( 'size', 'max', False, 3, 'GB' ), ( 'size', 'gif', False, 1, 'TB' ) ] ),
    ( 'the spin boxes\' limits', [
        ( 'size', 'min', False, 2000000, 'B' ), ( 'size', 'max', False, 0, 'KB' ),
        ( 'resolution', 'min', False, 2000000, 0 ), ( 'resolution', 'max', False, 1000000, 999999 ) ] ),
    ( 'everything off again', [
        ( 'size', 'min', False, 3, 'KB' ), ( 'size', 'min', True, 3, 'KB' ),
        ( 'resolution', 'max', False, 800, 600 ), ( 'resolution', 'max', True, 800, 600 ) ] ),
]

# ( size, mime name, width, height )
PROBES = [
    ( 1000, 'IMAGE_JPEG', 100, 100 ),
    ( 4096, 'IMAGE_JPEG', 640, 480 ),
    ( 3 * 1024, 'IMAGE_JPEG', 640, 480 ),
    ( 3 * 1024 - 1, 'IMAGE_JPEG', 640, 480 ),
    ( 5 * 1024 * 1024, 'IMAGE_JPEG', 4000, 3000 ),
    ( 5 * 1024 * 1024, 'IMAGE_JPEG', 4001, 3000 ),
    ( 5 * 1024 * 1024, 'IMAGE_JPEG', 5000, 100 ),
    ( 5 * 1024 * 1024, 'IMAGE_JPEG', 639, 480 ),
    ( 2 * 1024 * 1024, 'ANIMATION_GIF', 800, 600 ),
    ( 2 * 1024 * 1024 + 1, 'ANIMATION_GIF', 800, 600 ),
    ( 2 * 1024 * 1024 + 1, 'IMAGE_PNG', 800, 600 ),
    ( 100 * 1024 * 1024, 'IMAGE_JPEG', 1024, 768 ),
    ( 4 * 1024 * 1024 * 1024, 'IMAGE_JPEG', 1024, 768 ),
    ( 10 * 1024 * 1024 * 1024, 'IMAGE_JPEG', 1024, 768 ),
    ( 5000, 'VIDEO_MP4', None, None ),
    ( 5000, 'APPLICATION_ZIP', None, None ),
]

# options whose saved values the page shows: ( min, max, gif, min_resolution, max_resolution )
DISPLAYS = [
    ( None, None, None, None, None ),
    ( 5120, 104857600, 33554432, ( 50, 50 ), ( 8192, 8192 ) ),
    ( 1536, 1000, 3 * 1024 ** 3, ( 640, 480 ), ( 4000, 3000 ) ),
    ( 0, 1024 ** 4, 1536 * 1024, ( 0, 0 ), ( 1, 1000000 ) ),
]

# prefetch steps: ( 'hash', index ), ( 'url', index ), ( 'fetch_hash', bool ), ( 'fetch_url', bool ), ( 'neighbour', bool )
PREFETCH = [
    ( 'defaults', [] ),
    ( 'hash dispositive then url dispositive', [ ( 'hash', 2 ), ( 'url', 2 ), ( 'fetch_hash', True ), ( 'fetch_url', True ), ( 'neighbour', False ) ] ),
    ( 'url dispositive then hash dispositive', [ ( 'url', 2 ), ( 'hash', 2 ), ( 'hash', 1 ), ( 'hash', 2 ) ] ),
    ( 'no url check', [ ( 'neighbour', True ), ( 'url', 0 ), ( 'neighbour', False ), ( 'url', 1 ), ( 'hash', 0 ), ( 'fetch_url', True ) ] ),
]

NOTES = [
    ( 'defaults', [] ),
    ( 'the editor test', [ ( 'get_notes', True ), ( 'extend', True ), ( 'conflict', 2 ), ( 'whitelist', [ 'source', 'artist notes' ] ),
                           ( 'renames', [ [ 'old name', 'new name' ], [ 'second', '2nd' ] ] ), ( 'all_override', 'imported' ) ] ),
    ( 'replace, no extend', [ ( 'extend', False ), ( 'conflict', 0 ) ] ),
    ( 'ignore', [ ( 'conflict', 1 ) ] ),
    ( 'rename, mass-rename off', [ ( 'conflict', 3 ), ( 'all_override', 'imported' ), ( 'all_override', None ) ] ),
    ( 'get notes off', [ ( 'get_notes', False ) ] ),
]

# ( existing notes, incoming notes )
NOTE_PROBES = [
    ( { 'source' : 'abc' }, [ [ 'source', 'abc def' ], [ 'source2', 'zzz' ], [ 'artist notes', 'n1' ], [ 'old name', 'renamed note' ], [ 'second', '2nd note' ] ] ),
    ( { 'source' : 'abc' }, [ [ 'source', 'xyz' ] ] ),
    ( { 'source' : 'abc', 'source (1)' : 'abc more' }, [ [ 'source', 'abc more and more' ] ] ),
    ( {}, [ [ 'a', '1' ], [ 'b', '2' ], [ 'old name', 'x' ] ] ),
    ( { 'a' : '1' }, [ [ 'a', '1' ], [ 'b', '1' ] ] ),
]

# a tag service's steps: ( 'get_tags', bool ) or ( 'cog', attribute, bool )
TAGS = [
    ( 'defaults', [] ),
    ( 'the editor test', [ ( 'get_tags', True ), ( 'cog', '_to_new_files', True ), ( 'cog', '_to_already_in_inbox', False ), ( 'cog', '_to_already_in_archive', False ), ( 'cog', '_get_tags_overwrite_deleted', True ) ] ),
    ( 'inbox only', [ ( 'get_tags', True ), ( 'cog', '_to_new_files', False ), ( 'cog', '_to_already_in_archive', False ) ] ),
    ( 'archive only, additional overwrite', [ ( 'cog', '_to_new_files', False ), ( 'cog', '_to_already_in_inbox', False ), ( 'cog', '_additional_tags_overwrite_deleted', True ) ] ),
    ( 'nothing applies', [ ( 'get_tags', True ), ( 'cog', '_to_new_files', False ), ( 'cog', '_to_already_in_inbox', False ), ( 'cog', '_to_already_in_archive', False ) ] ),
]

FILTERABLE = [ 'blue eyes', 'character:samus aran', 'series:metroid' ]
ADDITIONAL = [ 'extra' ]
DELETED = [ 'blue eyes' ]


def record( session ):

    from qtpy import QtWidgets as QW

    from hydrus.core import HydrusConstants as HC
    from hydrus.core import HydrusExceptions
    from hydrus.client import ClientConstants as CC
    from hydrus.client.gui.importing import ClientGUIImportOptionsPanels as Panels
    from hydrus.client.importing.options import FileFilteringImportOptions
    from hydrus.client.importing.options import NoteImportOptions
    from hydrus.client.importing.options import PrefetchImportOptions
    from hydrus.client.importing.options import TagImportOptions
    from hydrus.client.metadata import ClientTags

    controller = session.controller
    gui = controller.gui

    def qt( f ):

        return controller.CallBlockingToQt( gui, f )


    out = {}

    # --- file filtering
    def filtering_value( options ):

        return {
            'exclude_deleted' : options.ExcludesDeleted(),
            'bombs' : options.AllowsDecompressionBombs(),
            'min_size' : options.GetMinSize(),
            'max_size' : options.GetMaxSize(),
            'max_gif_size' : options.GetMaxGifSize(),
            'min_resolution' : options.GetMinResolution(),
            'max_resolution' : options.GetMaxResolution()
        }


    def probe_filtering( options ):

        results = []

        for ( size, mime_name, width, height ) in PROBES:

            mime = getattr( HC, mime_name )

            try:

                options.CheckFileIsValid( size, mime, width, height )

                error = None

            except HydrusExceptions.FileImportRulesException as e:

                error = str( e )


            results.append( { 'size' : size, 'mime' : mime, 'width' : width, 'height' : height, 'error' : error } )


        return results


    def filtering():

        scenarios = []

        for ( name, ops ) in FILTERING:

            panel = Panels.EditFileFilteringImportOptionsPanel( gui, FileFilteringImportOptions.FileFilteringImportOptions() )

            for op in ops:

                if op[ 0 ] == 'exclude_deleted':

                    panel._exclude_deleted.setChecked( op[ 1 ] )

                elif op[ 0 ] == 'bombs':

                    panel._allow_decompression_bombs.setChecked( op[ 1 ] )

                elif op[ 0 ] == 'size':

                    control = { 'min' : panel._min_size, 'max' : panel._max_size, 'gif' : panel._max_gif_size }[ op[ 1 ] ]

                    control._none_checkbox.setChecked( op[ 2 ] )
                    control._UpdateEnabled()
                    control._bytes.SetSeparatedValue( op[ 3 ], UNITS[ op[ 4 ] ] )

                elif op[ 0 ] == 'resolution':

                    control = { 'min' : panel._min_resolution, 'max' : panel._max_resolution }[ op[ 1 ] ]

                    control._checkbox.setChecked( op[ 2 ] )
                    control._one.setValue( op[ 3 ] )
                    control._two.setValue( op[ 4 ] )


            options = panel.GetValue()

            scenarios.append( { 'name' : name, 'ops' : [ list( op ) for op in ops ], 'value' : filtering_value( options ), 'probes' : probe_filtering( options ) } )


        displays = []

        for ( min_size, max_size, gif, min_res, max_res ) in DISPLAYS:

            options = FileFilteringImportOptions.FileFilteringImportOptions()
            options.SetMinSize( min_size )
            options.SetMaxSize( max_size )
            options.SetMaxGifSize( gif )
            options.SetMinResolution( min_res )
            options.SetMaxResolution( max_res )

            panel = Panels.EditFileFilteringImportOptionsPanel( gui, options )

            def size_state( control ):

                ( value, unit ) = control._bytes.GetSeparatedValue()

                return { 'none' : control._none_checkbox.isChecked(), 'value' : value, 'unit' : unit }


            def resolution_state( control ):

                return { 'none' : control._checkbox.isChecked(), 'width' : control._one.value(), 'height' : control._two.value() }


            displays.append( {
                'saved' : [ min_size, max_size, gif, min_res, max_res ],
                'min_size' : size_state( panel._min_size ),
                'max_size' : size_state( panel._max_size ),
                'max_gif_size' : size_state( panel._max_gif_size ),
                'min_resolution' : resolution_state( panel._min_resolution ),
                'max_resolution' : resolution_state( panel._max_resolution )
            } )


        # the controls as a new options object shows them (limits off, with the controls' own defaults behind)
        panel = Panels.EditFileFilteringImportOptionsPanel( gui, FileFilteringImportOptions.FileFilteringImportOptions() )

        defaults = {
            'min_size' : panel._min_size._bytes.GetSeparatedValue(),
            'max_size' : panel._max_size._bytes.GetSeparatedValue(),
            'max_gif_size' : panel._max_gif_size._bytes.GetSeparatedValue(),
            'min_resolution' : [ panel._min_resolution._one.value(), panel._min_resolution._two.value() ],
            'max_resolution' : [ panel._max_resolution._one.value(), panel._max_resolution._two.value() ]
        }

        return { 'scenarios' : scenarios, 'displays' : displays, 'control_defaults' : defaults, 'units' : list( UNITS ) }


    out[ 'file_filtering' ] = qt( filtering )

    # --- prefetch
    def prefetch_state( panel ):

        value = panel.GetValue()

        return {
            'hash': panel._preimport_hash_check_type.currentIndex(),
            'url': panel._preimport_url_check_type.currentIndex(),
            'fetch_hash' : value.ShouldFetchMetadataEvenIfHashKnownAndFileAlreadyInDB(),
            'fetch_url' : value.ShouldFetchMetadataEvenIfURLKnownAndFileAlreadyInDB(),
            'neighbour' : value.PreImportURLCheckLooksForNeighbourSpam(),
            'neighbour_enabled' : panel._preimport_url_check_looks_for_neighbour_spam.isEnabled(),
            'hash_check_type' : value.GetPreImportHashCheckType(),
            'url_check_type' : value.GetPreImportURLCheckType()
        }


    def prefetch():

        scenarios = []

        for ( name, ops ) in PREFETCH:

            panel = Panels.EditPrefetchImportOptionsPanel( gui, PrefetchImportOptions.PrefetchImportOptions() )

            states = [ prefetch_state( panel ) ]

            for op in ops:

                if op[ 0 ] == 'hash':

                    panel._preimport_hash_check_type.setCurrentIndex( op[ 1 ] )

                elif op[ 0 ] == 'url':

                    panel._preimport_url_check_type.setCurrentIndex( op[ 1 ] )

                elif op[ 0 ] == 'fetch_hash':

                    panel._fetch_metadata_even_if_hash_recognised_and_file_already_in_db.setChecked( op[ 1 ] )

                elif op[ 0 ] == 'fetch_url':

                    panel._fetch_metadata_even_if_url_recognised_and_file_already_in_db.setChecked( op[ 1 ] )

                elif op[ 0 ] == 'neighbour':

                    panel._preimport_url_check_looks_for_neighbour_spam.setChecked( op[ 1 ] )


                states.append( prefetch_state( panel ) )


            scenarios.append( { 'name' : name, 'ops' : [ list( op ) for op in ops ], 'states' : states } )


        panel = Panels.EditPrefetchImportOptionsPanel( gui, PrefetchImportOptions.PrefetchImportOptions() )

        return {
            'scenarios' : scenarios,
            'hash_choices' : [ panel._preimport_hash_check_type.itemText( i ) for i in range( panel._preimport_hash_check_type.count() ) ],
            'url_choices' : [ panel._preimport_url_check_type.itemText( i ) for i in range( panel._preimport_url_check_type.count() ) ]
        }


    out[ 'prefetch' ] = qt( prefetch )

    # --- notes
    def notes():

        scenarios = []

        for ( name, ops ) in NOTES:

            panel = Panels.EditNoteImportOptionsPanel( gui, NoteImportOptions.NoteImportOptions() )

            for op in ops:

                if op[ 0 ] == 'get_notes':

                    panel._get_notes.setChecked( op[ 1 ] )

                elif op[ 0 ] == 'extend':

                    panel._extend_existing_note_if_possible.setChecked( op[ 1 ] )

                elif op[ 0 ] == 'conflict':

                    panel._conflict_resolution.setCurrentIndex( op[ 1 ] )

                elif op[ 0 ] == 'whitelist':

                    panel._name_whitelist.Clear()
                    panel._name_whitelist.AddDatas( op[ 1 ] )

                elif op[ 0 ] == 'renames':

                    panel._names_to_name_overrides.SetValue( { a : b for ( a, b ) in op[ 1 ] } )

                elif op[ 0 ] == 'all_override':

                    panel._all_name_override.SetValue( op[ 1 ] )


            value = panel.GetValue()

            probes = []

            for ( existing, incoming ) in NOTE_PROBES:

                updates = value.GetUpdateeNamesToNotes( existing, [ tuple( x ) for x in incoming ] )

                probes.append( { 'existing' : existing, 'incoming' : incoming, 'updates' : updates } )


            scenarios.append( {
                'name' : name,
                'ops' : [ list( op ) for op in ops ],
                'value' : {
                    'get_notes' : value.GetGetNotes(),
                    'extend' : value.GetExtendExistingNoteIfPossible(),
                    'conflict' : value.GetConflictResolution(),
                    'whitelist' : sorted( value.GetNameWhitelist() ),
                    'renames' : sorted( [ list( kv ) for kv in value.GetNamesToNameOverrides().items() ] ),
                    'all_override' : value.GetAllNameOverride()
                },
                'probes' : probes
            } )


        panel = Panels.EditNoteImportOptionsPanel( gui, NoteImportOptions.NoteImportOptions() )

        combo = panel._conflict_resolution

        return {
            'scenarios' : scenarios,
            'conflict_choices' : [ combo.itemText( i ) for i in range( combo.count() ) ],
            'conflict_codes' : [ combo.itemData( i ) for i in range( combo.count() ) ]
        }


    out[ 'notes' ] = qt( notes )

    # --- a tag service's rules
    class StubTagsManager( object ):

        def GetDeleted( self, service_key, display_type ):

            return set( DELETED )


    class StubMediaResult( object ):

        def __init__( self, inbox ):

            self._inbox = inbox

        def GetInbox( self ):

            return self._inbox

        def GetTagsManager( self ):

            return StubTagsManager()


    def tags():

        scenarios = []

        for ( name, ops ) in TAGS:

            panel = Panels.EditServiceTagImportOptionsPanel( gui, CC.DEFAULT_LOCAL_TAG_SERVICE_KEY, TagImportOptions.ServiceTagImportOptions() )

            for op in ops:

                if op[ 0 ] == 'get_tags':

                    panel._get_tags_checkbox.setChecked( op[ 1 ] )
                    panel._UpdateGetTags()

                elif op[ 0 ] == 'cog':

                    setattr( panel, op[ 1 ], op[ 2 ] )


            rules = panel.GetValue()

            ( get_tags, get_tags_filter, additional_tags, to_new, to_inbox, to_archive, only_existing, only_existing_filter, get_overwrite, additional_overwrite ) = rules.ToTuple()

            # (the rules as the importer applies them to a file of each kind)
            outcomes = {}

            for ( label, status, inbox ) in [ ( 'new', CC.STATUS_SUCCESSFUL_AND_NEW, True ), ( 'inbox', CC.STATUS_SUCCESSFUL_BUT_REDUNDANT, True ), ( 'archive', CC.STATUS_SUCCESSFUL_BUT_REDUNDANT, False ) ]:

                outcomes[ label ] = sorted( rules.GetTags( CC.DEFAULT_LOCAL_TAG_SERVICE_KEY, status, StubMediaResult( inbox ), FILTERABLE, ADDITIONAL ) )


            scenarios.append( {
                'name' : name,
                'ops' : [ list( op ) for op in ops ],
                'value' : {
                    'get_tags' : get_tags, 'to_new_files' : to_new, 'to_already_in_inbox' : to_inbox, 'to_already_in_archive' : to_archive,
                    'only_add_existing_tags' : only_existing, 'get_tags_overwrite_deleted' : get_overwrite, 'additional_tags_overwrite_deleted' : additional_overwrite,
                    'worth_fetching_tags' : rules.WorthFetchingTags()
                },
                'outcomes' : outcomes
            } )


        # what a service gets before anything is set (a new TagImportOptions' services)
        fresh = TagImportOptions.ServiceTagImportOptions().ToTuple()

        return { 'scenarios' : scenarios, 'filterable' : FILTERABLE, 'additional' : ADDITIONAL, 'deleted' : DELETED,
                 'fresh' : { 'get_tags' : fresh[ 0 ], 'to_new_files' : fresh[ 3 ], 'to_already_in_inbox' : fresh[ 4 ], 'to_already_in_archive' : fresh[ 5 ] } }


    out[ 'tags' ] = qt( tags )

    return out


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

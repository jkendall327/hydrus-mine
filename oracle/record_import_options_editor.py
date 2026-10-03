#!/usr/bin/env python3
"""Record the reference's import options editor for an importer
(`EditSpecificImportOptionsContainerPanel`): its frame, which lists the
kinds of import options and, for each, whether it uses the default (and
whose) or custom options.

For each importer kind in `CALLERS` (the default stack its options fall
back to: `IMPORT_OPTIONS_CALLER_TYPE_*`), in simple mode and not, this
opens the editor on an empty container and records the description text,
the list's labels (`tabs`: "default <kind> (<whose>)", or "> <kind>:
<summary>" for custom), and each kind's page: its description and its
two choices ("use the default import options (<whose>)", "set custom
import options"). Then, on a container with custom presentation (new
files only) and file filtering (a new client's), the list's labels
again (`custom_tabs`), and the container the editor gives back
(`custom_value`, its kinds by `IOC.import_options_type_str_lookup`).

`summaries` are each kind's `GetSummary` (as the editor's list shows it,
for a specific importer) for the options in `SUMMARIES`: a kind and the
settings changed from its defaults (filetypes are `HC` mime codes; tag
filters are `[slice, rule]` pairs, rule 0 whitelist, 1 blacklist;
`tags` maps "downloader" or "my tags" to a service's settings).

The client's import options defaults are a new client's.

Usage: QT_QPA_PLATFORM=offscreen python oracle/record_import_options_editor.py
       (writes fixtures/import_options_editor.json)
"""

import json
import os
import sys

HERE = os.path.dirname( os.path.abspath( __file__ ) )

sys.path.insert( 0, HERE )

OUT = os.path.join( HERE, 'fixtures', 'import_options_editor.json' )

# ( name, caller type code )
CALLERS = [
    ( 'gallery/post urls', 1 ),
    ( 'watchable urls', 3 ),
    ( 'subscription', 2 ),
    ( 'local hard drive import', 0 ),
    ( 'import folder', 9 ),
]

SUMMARIES = [
    { 'kind' : 'prefetch' },
    { 'kind' : 'prefetch', 'hash' : 0, 'url' : 0 },
    { 'kind' : 'prefetch', 'hash' : 0 },
    { 'kind' : 'prefetch', 'url' : 0 },
    { 'kind' : 'prefetch', 'fetch_hash' : True, 'fetch_url' : True },
    { 'kind' : 'prefetch', 'fetch_hash' : True },
    { 'kind' : 'prefetch', 'url' : 2, 'hash' : 1, 'fetch_url' : True },
    { 'kind' : 'file filtering' },
    { 'kind' : 'file filtering', 'exclude_deleted' : False, 'bombs' : False },
    { 'kind' : 'file filtering', 'min_size' : 5120, 'max_size' : 104857600, 'max_gif_size' : 33554432, 'min_resolution' : [ 50, 50 ], 'max_resolution' : [ 8192, 8192 ] },
    { 'kind' : 'file filtering', 'filetypes' : [ 1, 2 ] },
    { 'kind' : 'tag filtering' },
    { 'kind' : 'tag filtering', 'blacklist' : [ [ 'orc', 1 ], [ 'goblin', 1 ] ] },
    { 'kind' : 'tag filtering', 'whitelist' : [ 'blue eyes', 'green eyes' ] },
    { 'kind' : 'tag filtering', 'blacklist' : [ [ 'creator:', 1 ], [ 'creator:someone', 0 ] ], 'whitelist' : [ 'blue eyes' ] },
    { 'kind' : 'tag filtering', 'blacklist' : [ [ '', 1 ], [ ':', 1 ] ] },
    { 'kind' : 'locations' },
    { 'kind' : 'locations', 'archive' : True, 'destinations_for_already_in_db' : True },
    { 'kind' : 'tags' },
    { 'kind' : 'tags', 'tags' : { 'downloader' : { 'get_tags' : True } } },
    { 'kind' : 'tags', 'tags' : { 'downloader' : { 'get_tags' : True, 'filter' : [ [ 'goblin', 1 ] ] }, 'my tags' : { 'additional' : [ 'zebra', 'apple' ] } } },
    { 'kind' : 'tags', 'tags' : { 'downloader' : { 'get_tags' : True, 'filter' : [ [ '', 1 ], [ ':', 1 ], [ 'creator:', 0 ] ] } } },
    { 'kind' : 'tags', 'tags' : { 'downloader' : { 'get_tags' : True, 'filter' : [ [ '', 1 ], [ 'series:', 0 ], [ 'series:bad', 1 ] ] } } },
    { 'kind' : 'tags', 'tags' : { 'downloader' : { 'get_tags' : True, 'filter' : [ [ ':', 1 ] ] } } },
    { 'kind' : 'notes' },
    { 'kind' : 'notes', 'get_notes' : False },
    { 'kind' : 'notes', 'extend' : False, 'conflict' : 0 },
    { 'kind' : 'notes', 'conflict' : 1, 'all_name_override' : 'note' },
    { 'kind' : 'notes', 'conflict' : 2, 'overrides' : { 'comment' : 'artist comment' } },
    { 'kind' : 'presentation' },
    { 'kind' : 'presentation', 'status' : 1, 'inbox' : 1 },
    { 'kind' : 'presentation', 'status' : 2 },
]


def make_options( case ):

    from hydrus.core import HydrusTags
    from hydrus.client import ClientConstants as CC
    from hydrus.client.importing.options import FileFilteringImportOptions
    from hydrus.client.importing.options import LocationImportOptions
    from hydrus.client.importing.options import NoteImportOptions
    from hydrus.client.importing.options import PrefetchImportOptions
    from hydrus.client.importing.options import PresentationImportOptions
    from hydrus.client.importing.options import TagFilteringImportOptions
    from hydrus.client.importing.options import TagImportOptions

    def tag_filter( rules ):

        f = HydrusTags.TagFilter()

        for ( tag_slice, rule ) in rules:

            f.SetRule( tag_slice, rule )


        return f


    kind = case[ 'kind' ]

    if kind == 'prefetch':

        o = PrefetchImportOptions.PrefetchImportOptions()

        if 'hash' in case: o.SetPreImportHashCheckType( case[ 'hash' ] )
        if 'url' in case: o.SetPreImportURLCheckType( case[ 'url' ] )
        if 'fetch_hash' in case: o.SetShouldFetchMetadataEvenIfHashKnownAndFileAlreadyInDB( case[ 'fetch_hash' ] )
        if 'fetch_url' in case: o.SetShouldFetchMetadataEvenIfURLKnownAndFileAlreadyInDB( case[ 'fetch_url' ] )

    elif kind == 'file filtering':

        o = FileFilteringImportOptions.FileFilteringImportOptions()

        if 'exclude_deleted' in case: o.SetExcludesDeleted( case[ 'exclude_deleted' ] )
        if 'bombs' in case: o.SetAllowsDecompressionBombs( case[ 'bombs' ] )
        if 'min_size' in case: o.SetMinSize( case[ 'min_size' ] )
        if 'max_size' in case: o.SetMaxSize( case[ 'max_size' ] )
        if 'max_gif_size' in case: o.SetMaxGifSize( case[ 'max_gif_size' ] )
        if 'min_resolution' in case: o.SetMinResolution( tuple( case[ 'min_resolution' ] ) )
        if 'max_resolution' in case: o.SetMaxResolution( tuple( case[ 'max_resolution' ] ) )
        if 'filetypes' in case: o.SetAllowedSpecificFiletypes( case[ 'filetypes' ] )

    elif kind == 'tag filtering':

        o = TagFilteringImportOptions.TagFilteringImportOptions( tag_blacklist = tag_filter( case.get( 'blacklist', [] ) ), tag_whitelist = case.get( 'whitelist', [] ) )

    elif kind == 'locations':

        o = LocationImportOptions.LocationImportOptions()

        if 'archive' in case: o.SetAutomaticallyArchives( case[ 'archive' ] )
        if 'destinations_for_already_in_db' in case: o.SetDoImportDestinationsOnAlreadyInDBFiles( case[ 'destinations_for_already_in_db' ] )

    elif kind == 'tags':

        keys = { 'downloader' : CC.DEFAULT_LOCAL_DOWNLOADER_TAG_SERVICE_KEY, 'my tags' : CC.DEFAULT_LOCAL_TAG_SERVICE_KEY }

        services = {}

        for ( name, s ) in case.get( 'tags', {} ).items():

            services[ keys[ name ] ] = TagImportOptions.ServiceTagImportOptions( get_tags = s.get( 'get_tags', False ), get_tags_filter = tag_filter( s.get( 'filter', [] ) ), additional_tags = s.get( 'additional', [] ) )


        o = TagImportOptions.TagImportOptions( service_keys_to_service_tag_import_options = services )

    elif kind == 'notes':

        o = NoteImportOptions.NoteImportOptions()

        if 'get_notes' in case: o.SetGetNotes( case[ 'get_notes' ] )
        if 'extend' in case: o.SetExtendExistingNoteIfPossible( case[ 'extend' ] )
        if 'conflict' in case: o.SetConflictResolution( case[ 'conflict' ] )
        if 'all_name_override' in case: o.SetAllNameOverride( case[ 'all_name_override' ] )
        if 'overrides' in case: o.SetNamesToNameOverrides( case[ 'overrides' ] )

    else:

        o = PresentationImportOptions.PresentationImportOptions()

        if 'status' in case: o.SetPresentationStatus( case[ 'status' ] )
        if 'inbox' in case: o.SetPresentationInbox( case[ 'inbox' ] )


    return o


def record( session ):

    controller = session.controller
    gui = controller.gui

    def f():

        from hydrus.client.gui.importing import ClientGUIImportOptionsContainer
        from hydrus.client.importing.options import FileFilteringImportOptions
        from hydrus.client.importing.options import ImportOptionsConstants as IOC
        from hydrus.client.importing.options import ImportOptionsContainer
        from hydrus.client.importing.options import PresentationImportOptions

        manager = controller.import_options_manager

        def tabs( panel ):

            book = panel._listbook

            return [ book.tabText( i ) for i in range( book.count() ) ]


        out = []

        for ( name, caller ) in CALLERS:

            for simple in ( True, False ):

                controller.new_options.SetBoolean( 'import_options_simple_mode', simple )

                panel = ClientGUIImportOptionsContainer.EditSpecificImportOptionsContainerPanel( gui, manager, caller, ImportOptionsContainer.ImportOptionsContainer() )

                pages = []

                for page in panel._listbook.GetPages():

                    dropdown = page._use_default_dropdown

                    pages.append( {
                        'description' : page._description_label.text(),
                        'choices' : [ dropdown.itemText( i ) for i in range( dropdown.count() ) ],
                    } )


                container = ImportOptionsContainer.ImportOptionsContainer()

                presentation = PresentationImportOptions.PresentationImportOptions()
                presentation.SetPresentationStatus( PresentationImportOptions.PRESENTATION_STATUS_NEW_ONLY )

                container.SetImportOptions( presentation )
                container.SetImportOptions( FileFilteringImportOptions.FileFilteringImportOptions() )

                custom = ClientGUIImportOptionsContainer.EditSpecificImportOptionsContainerPanel( gui, manager, caller, container )

                value = custom.GetValue()

                out.append( {
                    'caller' : name,
                    'code' : caller,
                    'simple' : simple,
                    'description' : panel._description_label.text(),
                    'tabs' : tabs( panel ),
                    'pages' : pages,
                    'custom_tabs' : tabs( custom ),
                    'custom_value' : [ IOC.import_options_type_str_lookup[ t ] for t in IOC.IMPORT_OPTIONS_TYPES_CANONICAL_ORDER if value.HasImportOptions( t ) ],
                } )



        summaries = [ dict( case, summary = make_options( case ).GetSummary( IOC.IMPORT_OPTIONS_CALLER_TYPE_SPECIFIC_IMPORTER ) ) for case in SUMMARIES ]

        return { 'editors' : out, 'summaries' : summaries }


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

        path = os.path.join( work, 'import_options_editor.json' )

        hydrus_driver.run_in_subprocess( os.path.abspath( __file__ ), '--child', path )

        with open( path ) as f:

            result = json.load( f )



    with open( OUT, 'w' ) as f:

        json.dump( result, f, indent = 1, ensure_ascii = False )
        f.write( '\n' )


    print( f'wrote {OUT}: {len( result[ "editors" ] )} editors, {len( result[ "summaries" ] )} summaries' )


if __name__ == '__main__':

    main()

#!/usr/bin/env python3
"""Record how the reference's import and export folders dialogs write their
lists.

file > import/export folders > "manage import folders…" lists the import
folders (`EditImportFoldersPanel._ConvertImportFolderToDisplayTuple`):
name, path, paused ("yes") and check period ("1 hour", "not checking
regularly"). "manage export folders…" lists the export folders
(`EditExportFoldersPanel._ConvertExportFolderToDisplayTuple`): name, path,
type ("regular", "synchronise", "… and deleting from the client!"), query
(its predicates as the reference writes them, joined by ", "), period
("1 day", "not running regularly", "… (running after dialog ok)"),
phrase and recent error.

In the running client, on the `basic` fixture, this makes the folders
`IMPORT_FOLDERS` and `EXPORT_FOLDERS` describe and records each one's row
and the column titles. Export folders' queries are given as Client API
tag lists (`tags`), parsed as the Client API parses them.

Usage: QT_QPA_PLATFORM=offscreen python oracle/record_folders_lists.py
       (writes fixtures/folders_lists.json)
"""

import json
import os
import sys

HERE = os.path.dirname( os.path.abspath( __file__ ) )

sys.path.insert( 0, HERE )

OUT = os.path.join( HERE, 'fixtures', 'folders_lists.json' )

HOUR = 3600
DAY = 86400

IMPORT_FOLDERS = [
    { 'name' : 'inbox drop', 'path' : '/home/me/drop', 'paused' : False, 'check_regularly' : True, 'period' : HOUR },
    { 'name' : 'Archive scans', 'path' : '/mnt/scans', 'paused' : True, 'check_regularly' : True, 'period' : 3 * DAY + 4 * HOUR },
    { 'name' : 'manual only', 'path' : '/home/me/manual', 'paused' : False, 'check_regularly' : False, 'period' : 5 * 60 },
]

EXPORT_FOLDERS = [
    { 'name' : 'favourites', 'path' : '/home/me/export', 'type' : 'regular', 'delete' : False, 'tags' : [ 'character:samus aran', 'system:archive' ], 'run_regularly' : True, 'period' : DAY, 'phrase' : '{hash}', 'run_now' : False, 'last_error' : '' },
    { 'name' : 'Mirror', 'path' : '/mnt/mirror', 'type' : 'synchronise', 'delete' : False, 'tags' : [ 'blue eyes', '-red hair' ], 'run_regularly' : True, 'period' : 7 * DAY, 'phrase' : '{hash} {tags}', 'run_now' : True, 'last_error' : '' },
    { 'name' : 'clear out', 'path' : '/tmp/out', 'type' : 'regular', 'delete' : True, 'tags' : [ 'system:inbox' ], 'run_regularly' : False, 'period' : HOUR, 'phrase' : '{hash}', 'run_now' : False, 'last_error' : 'could not write to /tmp/out' },
    { 'name' : 'nothing', 'path' : '', 'type' : 'regular', 'delete' : False, 'tags' : [], 'run_regularly' : False, 'period' : DAY, 'phrase' : '{hash}', 'run_now' : True, 'last_error' : '' },
]


def record( session ):

    controller = session.controller
    gui = controller.gui

    def f():

        from hydrus.core import HydrusConstants as HC

        from hydrus.client import ClientLocation
        from hydrus.client import ClientConstants as CC
        from hydrus.client.exporting import ClientExportingFiles
        from hydrus.client.gui.exporting import ClientGUIExport
        from hydrus.client.gui.importing import ClientGUIImportFolders
        from hydrus.client.gui.lists import ClientGUIListConstants as CGLC
        from hydrus.client.importing import ClientImportLocal
        from hydrus.client.networking.api import ClientLocalServerCore
        from hydrus.client.search import ClientSearchFileSearchContext

        def titles( list_id, count ):

            return [ CGLC.column_list_column_name_lookup[ list_id ][ i ] for i in range( count ) ]


        imports = []

        for case in IMPORT_FOLDERS:

            folder = ClientImportLocal.ImportFolder( case[ 'name' ], path = case[ 'path' ], period = case[ 'period' ], check_regularly = case[ 'check_regularly' ] )

            folder._paused = case[ 'paused' ]

            row = ClientGUIImportFolders.EditImportFoldersPanel._ConvertImportFolderToDisplayTuple( None, folder )

            imports.append( { 'folder' : case, 'row' : list( row ) } )


        exports = []

        location_context = ClientLocation.LocationContext.STATICCreateSimple( CC.COMBINED_LOCAL_FILE_DOMAINS_SERVICE_KEY )

        for case in EXPORT_FOLDERS:

            predicates = ClientLocalServerCore.ConvertTagListToPredicates( None, case[ 'tags' ], do_permission_check = False ) if case[ 'tags' ] else []

            file_search_context = ClientSearchFileSearchContext.FileSearchContext( location_context = location_context, predicates = predicates )

            export_type = HC.EXPORT_FOLDER_TYPE_SYNCHRONISE if case[ 'type' ] == 'synchronise' else HC.EXPORT_FOLDER_TYPE_REGULAR

            folder = ClientExportingFiles.ExportFolder(
                case[ 'name' ],
                path = case[ 'path' ],
                export_type = export_type,
                delete_from_client_after_export = case[ 'delete' ],
                file_search_context = file_search_context,
                run_regularly = case[ 'run_regularly' ],
                period = case[ 'period' ],
                phrase = case[ 'phrase' ],
                run_now = case[ 'run_now' ],
                last_error = case[ 'last_error' ]
            )

            row = ClientGUIExport.EditExportFoldersPanel._ConvertExportFolderToDisplayTuple( None, folder )

            exports.append( { 'folder' : case, 'row' : list( row ) } )


        return {
            'import_columns' : titles( CGLC.COLUMN_LIST_IMPORT_FOLDERS.ID, 4 ),
            'export_columns' : [ CGLC.column_list_column_name_lookup[ CGLC.COLUMN_LIST_EXPORT_FOLDERS.ID ][ c ] for c in ( 0, 1, 2, 3, 5, 6, 7 ) ],
            'import_folders' : imports,
            'export_folders' : exports,
        }


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

        path = os.path.join( work, 'folders_lists.json' )

        hydrus_driver.run_in_subprocess( os.path.abspath( __file__ ), '--child', path )

        with open( path ) as f:

            result = json.load( f )



    with open( OUT, 'w' ) as f:

        json.dump( result, f, indent = 1, ensure_ascii = False )
        f.write( '\n' )


    print( f'wrote {OUT}: {len( result[ "import_folders" ] )} import folders, {len( result[ "export_folders" ] )} export folders' )


if __name__ == '__main__':

    main()

#!/usr/bin/env python3
"""Record the reference's file > import/export folders > "check import
folder now" and "run export folder now" (`ClientGUI._CheckImportFolder`
and `_RunExportFolder`).

In the running client, on the `basic` fixture, three import folders (one
of them paused by hand, one not checked regularly) and three export folders
are made. For each of: check one import folder, check all, run one export
folder, run all, and each again with the folders paused under the file
menu (`pause_import_folders_sync`, `pause_export_folders_sync`), the
folders' flags (`paused`, `check_now` / `run_now`) as they are in the
database afterwards and the text the client shows (`HydrusData.ShowText`)
are recorded, from the same starting folders each time.

Usage: QT_QPA_PLATFORM=offscreen python oracle/record_folder_runs.py
       (writes fixtures/folder_runs.json)
"""

import json
import os
import sys
import time

HERE = os.path.dirname( os.path.abspath( __file__ ) )

sys.path.insert( 0, HERE )

OUT = os.path.join( HERE, 'fixtures', 'folder_runs.json' )

IMPORT_FOLDERS = [
    ( 'drop box', False, True ),
    ( 'paused one', True, True ),
    ( 'manual only', False, False ),
]

EXPORT_FOLDERS = [
    ( 'favourites', True ),
    ( 'mirror', True ),
    ( 'clear out', False ),
]


def record( session ):

    controller = session.controller
    gui = controller.gui

    from hydrus.core import HydrusConstants as HC
    from hydrus.core import HydrusData
    from hydrus.core import HydrusSerialisable
    from hydrus.client import ClientConstants as CC
    from hydrus.client import ClientLocation
    from hydrus.client.exporting import ClientExportingFiles
    from hydrus.client.importing import ClientImportLocal
    from hydrus.client.search import ClientSearchFileSearchContext

    def qt( f ):

        return controller.CallBlockingToQt( gui, f )


    # (the folders' own workers must not touch the flags: they are paused for
    # the whole recording, and the menu's pause is the options' flag the
    # menu methods read)
    controller.new_options.SetBoolean( 'pause_import_folders_sync', True )
    controller.new_options.SetBoolean( 'pause_export_folders_sync', True )

    messages = []

    real_show_text = HydrusData.ShowText

    HydrusData.ShowText = lambda text, *args, **kwargs: messages.append( str( text ) )

    gui_module = sys.modules[ 'hydrus.client.gui.ClientGUI' ]

    gui_module.HydrusData.ShowText = HydrusData.ShowText

    location_context = ClientLocation.LocationContext.STATICCreateSimple( CC.COMBINED_LOCAL_FILE_DOMAINS_SERVICE_KEY )

    def make_folders():

        for ( name, paused, check_regularly ) in IMPORT_FOLDERS:

            folder = ClientImportLocal.ImportFolder( name, path = '/nonexistent/' + name, check_regularly = check_regularly )

            folder._paused = paused

            controller.WriteSynchronous( 'serialisable', folder )


        for ( name, run_regularly ) in EXPORT_FOLDERS:

            context = ClientSearchFileSearchContext.FileSearchContext( location_context = location_context, predicates = [] )

            folder = ClientExportingFiles.ExportFolder( name, path = '/nonexistent/' + name, file_search_context = context, run_regularly = run_regularly )

            controller.WriteSynchronous( 'serialisable', folder )


    def delete_folders():

        for ( name, paused, check_regularly ) in IMPORT_FOLDERS:

            controller.WriteSynchronous( 'delete_serialisable_named', HydrusSerialisable.SERIALISABLE_TYPE_IMPORT_FOLDER, name )


        for ( name, run_regularly ) in EXPORT_FOLDERS:

            controller.WriteSynchronous( 'delete_serialisable_named', HydrusSerialisable.SERIALISABLE_TYPE_EXPORT_FOLDER, name )



    def flags():

        imports = {}

        for ( name, paused, check_regularly ) in IMPORT_FOLDERS:

            folder = controller.Read( 'serialisable_named', HydrusSerialisable.SERIALISABLE_TYPE_IMPORT_FOLDER, name )

            imports[ name ] = { 'paused' : folder._paused, 'check_now' : folder._check_now }


        exports = {}

        for ( name, run_regularly ) in EXPORT_FOLDERS:

            folder = controller.Read( 'serialisable_named', HydrusSerialisable.SERIALISABLE_TYPE_EXPORT_FOLDER, name )

            exports[ name ] = { 'run_now' : folder._run_now }


        return { 'imports' : imports, 'exports' : exports }


    cases = []

    for sync_paused in ( False, True ):

        controller.new_options.SetBoolean( 'pause_import_folders_sync', sync_paused )
        controller.new_options.SetBoolean( 'pause_export_folders_sync', sync_paused )

        for ( what, call ) in (
            ( 'check one', lambda: gui._CheckImportFolder( 'paused one' ) ),
            ( 'check one, not paused', lambda: gui._CheckImportFolder( 'drop box' ) ),
            ( 'check all', lambda: gui._CheckImportFolder() ),
            ( 'run one', lambda: gui._RunExportFolder( 'mirror' ) ),
            ( 'run all', lambda: gui._RunExportFolder() ),
        ):

            make_folders()

            before = flags()

            del messages[:]

            qt( call )

            time.sleep( 0.2 )

            cases.append( { 'sync_paused' : sync_paused, 'what' : what, 'before' : before, 'after' : flags(), 'messages' : list( messages ) } )

            delete_folders()


    HydrusData.ShowText = real_show_text

    return {
        'import_folders' : [ { 'name' : n, 'paused' : p, 'check_regularly' : c } for ( n, p, c ) in IMPORT_FOLDERS ],
        'export_folders' : [ { 'name' : n, 'run_regularly' : r } for ( n, r ) in EXPORT_FOLDERS ],
        'cases' : cases,
    }


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

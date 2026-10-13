#!/usr/bin/env python3
"""Flow 3: an import folder whose files have sidecars, set up through the
real dialogs and run by the client, then run again when a file arrives.

On the `basic` fixture, a folder of files with `.txt` sidecars (copied from
`fixtures/flow_import_folder/`, with settled modified times):

1. "add": file > manage import folders (`_ManageImportFolders`, its real
   worker and dialogs), "add", the edit import folder dialog: named "drop
   box", the folder's path, new files moved to another folder (with their
   sidecars), and three sidecar routers through the sidecar dialogs:
   `<file>.txt` to my tags, `<file>.urls.txt` to the file's URLs, and
   `<file>.notes.txt` (split by four pipes) to a note named "comment".
   Apply, apply. The import folders manager picks the folder up and runs
   it; wait until it has.
2. "reopen": the dialog again, to see its list and the folder's sidecar
   button, then cancel.
3. "new file": a file with sidecars arrives in the folder, file > import
   folders > check import folder now ("drop box"); wait until the folder has
   run again.

After each step: the dialogs' rows and what the dialogs showed, the folder's
file log (the files left in it), both folders' contents, the imported files
(tags, URLs, notes, inbox, domains) and the popups still shown. Texts are
normalised by `flows.normaliser`, the two folders' paths becoming `WATCH`
and `MOVED`.

Usage: QT_QPA_PLATFORM=offscreen ~/pyenv/bin/python oracle/record_flow_import_folder.py
       (writes oracle/fixtures/flow_import_folder.json)
"""

import hashlib
import os
import shutil
import sys
import tempfile
import time

HERE = os.path.dirname( os.path.abspath( __file__ ) )
sys.path.insert( 0, HERE )

import flows

OUT = os.path.join( HERE, 'fixtures', 'flow_import_folder.json' )
SOURCE = os.path.join( HERE, 'fixtures', 'flow_import_folder' )

# (the flow has no site of its own, but every flow has a port)
SITE_PORT = 46013

PHASES = [ flows.booru_phase( {} ) ]

NAME = 'drop box'
OLD_MTIME = 1_600_000_000


def listing( folder ):
    """Every file under `folder` (relative path, size)."""

    out = {}

    for ( root, dirs, files ) in os.walk( folder ):

        for name in files:

            path = os.path.join( root, name )

            try:

                out[ os.path.relpath( path, folder ) ] = os.path.getsize( path )

            except FileNotFoundError:

                # (moved by the folder's worker as we look)
                pass




    return dict( sorted( out.items() ) )


def source_files():
    """The images the flow brings (path under the source folder, sha256)."""

    out = {}

    for ( root, dirs, files ) in os.walk( SOURCE ):

        for name in files:

            if name.endswith( '.txt' ):

                continue


            path = os.path.join( root, name )

            with open( path, 'rb' ) as f:

                out[ os.path.relpath( path, SOURCE ) ] = hashlib.sha256( f.read() ).hexdigest()



    return dict( sorted( out.items() ) )


def place( folder, only = None ):
    """Copy the source files (or those under `only`) into `folder`, as old
    files."""

    for ( root, dirs, files ) in os.walk( SOURCE ):

        for name in files:

            path = os.path.join( root, name )
            relative = os.path.relpath( path, SOURCE )

            if ( only is None ) == ( relative.startswith( 'later' + os.sep ) ):

                continue


            if only is not None:

                relative = os.path.relpath( relative, only )


            target = os.path.join( folder, relative )

            os.makedirs( os.path.dirname( target ), exist_ok = True )

            shutil.copy( path, target )

            os.utime( target, ( OLD_MTIME, OLD_MTIME ) )




def record( session, site, watch, moved ):

    controller = session.controller
    gui = controller.gui
    base_normalise = flows.normaliser( site )

    def normalise( text ):

        if text is None:

            return None


        return base_normalise( text.replace( watch, 'WATCH' ).replace( moved, 'MOVED' ) )


    domain_manager = controller.CallBlockingToQt( gui, lambda: flows.install( controller, site ) )

    from qtpy import QtWidgets as QW

    from hydrus.core import HydrusExceptions
    from hydrus.core import HydrusSerialisable
    from hydrus.client import ClientConstants as CC
    from hydrus.client.gui import ClientGUIDialogsMessage
    from hydrus.client.gui import ClientGUIDialogsQuick

    steps = []

    # (what the dialogs ask or tell, and the answers they get)
    asked = []

    def show( kind ):

        def shower( win, message, **kwargs ):

            asked.append( { 'kind' : kind, 'message' : normalise( message ) } )


        return shower


    ClientGUIDialogsMessage.ShowWarning = show( 'warning' )
    ClientGUIDialogsMessage.ShowInformation = show( 'information' )

    hashes = source_files()

    def state_of_files():

        known = {}

        for h in hashes.values():

            try:

                session.read( 'media_result', bytes.fromhex( h ) )

            except HydrusExceptions.DataMissing:

                continue


            known[ h ] = True


        files = flows.files_state( session, set( known ), normalise )

        for h in files:

            media_result = session.read( 'media_result', bytes.fromhex( h ) )

            notes = media_result.GetNotesManager().GetNamesToNotes()

            files[ h ][ 'notes' ] = { name : normalise( text ) for ( name, text ) in sorted( notes.items() ) }


        # (a file not yet imported is known to the database by its hash alone)
        files = { h : f for ( h, f ) in files.items() if len( f[ 'in' ] ) > 0 }


        return files


    def folder():

        return controller.Read( 'serialisable_named', HydrusSerialisable.SERIALISABLE_TYPE_IMPORT_FOLDER, NAME )


    def log_state():

        try:

            import_folder = folder()

        except HydrusExceptions.DataMissing:

            return None


        return flows.file_log_state( import_folder.GetFileSeedCache(), normalise )


    def qt( f ):

        return controller.CallBlockingToQt( gui, f )


    def rows( panel ):

        out = []

        for import_folder in panel._import_folders.GetData():

            display = panel._ConvertImportFolderToDisplayTuple( import_folder )

            out.append( [ normalise( str( d ) ) for d in display ] )


        return out


    def step( name, seen ):

        steps.append( {
            'step' : name,
            'dialog' : seen,
            'asked' : list( asked ),
            'folder log' : log_state(),
            'watch' : listing( watch ),
            'moved' : listing( moved ),
            'files' : state_of_files(),
            'popups' : flows.popups_shown( controller, normalise ),
            'hits' : site.take_hits(),
        } )

        asked.clear()


    def folder_done( expected ):
        """The folder's worker has taken everything it found: no file left
        unknown, the manager not working, the sync not paused for an edit."""

        def done():

            if controller.new_options.GetBoolean( 'pause_import_folders_sync' ):

                return False


            from hydrus.core import HydrusGlobals as HG

            if HG.import_folders_running:

                return False


            try:

                import_folder = folder()

            except HydrusExceptions.DataMissing:

                return False


            cache = import_folder.GetFileSeedCache()

            return cache.GetFileSeedCount( CC.STATUS_UNKNOWN ) == 0 and expected()


        return done


    # ---------------------------------------------------------------- 1. add

    seen = {}

    choices = [ 'a .txt sidecar', 'a .txt sidecar', 'a file\'s URLs', 'a .txt sidecar', 'a file\'s notes' ]

    def choose( dlg, panel ):
        """The "Which type?" question: the next of `choices`."""

        buttons = { b.text() : b for b in panel.findChildren( QW.QPushButton ) if b.text() != '' }

        wanted = choices.pop( 0 )

        seen.setdefault( 'type questions', [] ).append( { 'offered' : sorted( buttons ), 'chosen' : wanted } )

        buttons[ wanted ].click()

        return True


    def importer( suffix, separator = None ):

        def script( dlg, panel ):

            sidecar = panel._sidecar_panel

            seen.setdefault( 'sources', [] ).append( {
                'type' : panel._change_type_button.text() if hasattr( panel, '_change_type_button' ) else None,
                'example' : sidecar._example_output.text(),
            } )

            sidecar._suffix.setText( suffix )

            if separator is not None:

                panel._txt_separator_panel.SetValue( separator )


            seen[ 'sources' ][ -1 ][ 'result' ] = sidecar._example_output.text()

            return True


        return script


    def router_script( suffix, separator = None, forced_name = None, exporter_label = None ):

        def script( dlg, panel ):

            importers = panel._importers_list

            importers._Add()

            if exporter_label is not None:

                panel._exporter_widget._ChangeType()

                if forced_name is not None:

                    panel._exporter_widget._forced_note_name.SetValue( forced_name )



            seen.setdefault( 'routers', [] ).append( {
                'sources' : [ importers._data_to_pretty_callable( d ) for d in importers.GetData() ],
                'processing' : panel._string_processor_button._edit_button.text(),
            } )

            return True


        return script


    router_scripts = [
        router_script( '' ),
        router_script( 'urls', exporter_label = 'urls' ),
        router_script( 'notes', exporter_label = 'notes', forced_name = 'comment' ),
    ]

    importer_scripts = [ importer( '' ), importer( 'urls' ), importer( 'notes', separator = '||||' ) ]

    def routers_dialog( dlg, panel ):

        control = panel._control

        for i in range( 3 ):

            control._Add()


        seen[ 'router rows' ] = [ control._data_to_pretty_callable( d ) for d in control.GetData() ]

        return True


    def edit_folder( dlg, panel ):

        seen[ 'name starts as' ] = panel._name.text()
        seen[ 'check regularly' ] = panel._check_regularly.isChecked()
        seen[ 'search subdirectories' ] = panel._search_subdirectories.isChecked()
        seen[ 'check on ok' ] = panel._check_now.isChecked()
        seen[ 'sidecars start as' ] = panel._metadata_routers_button.text()

        panel._name.setText( NAME )
        panel._path.SetPath( watch )
        panel._action_successful.SetValue( CC.IMPORT_FOLDER_MOVE )
        panel._location_successful.SetPath( moved )

        seen[ 'actions' ] = [ panel._action_successful.currentText(), panel._action_redundant.currentText(), panel._action_deleted.currentText(), panel._action_failed.currentText() ]

        panel._metadata_routers_button._Edit()

        seen[ 'sidecars end as' ] = panel._metadata_routers_button.text()

        return True


    def manage( first_script, extra, expected ):
        """file > manage import folders, its dialogs scripted; until the
        dialog has closed and the client's worker has finished."""

        done = []

        def opened( dlg, panel ):

            try:

                return first_script( dlg, panel )

            finally:

                done.append( True )



        scripts = { 'EditImportFoldersPanel' : [ opened ] } | extra

        with flows.scripted_dialogs( scripts ) as dialogs:

            # (the real method works on a thread of its own, then asks Qt)
            gui._ManageImportFolders()

            flows.wait_until_idle( controller, also = lambda: dialogs.check() and len( done ) > 0 and folder_done( expected )() )



    def add( dlg, panel ):

        seen[ 'rows when opened' ] = rows( panel )

        panel._Add()

        seen[ 'rows at apply' ] = rows( panel )

        return True


    extra = {
        'EditImportFolderPanel' : [ edit_folder ],
        'EditSingleCtrlPanel' : [ routers_dialog ],
        'EditSingleFileMetadataRouterPanel' : router_scripts,
        'EditSelectFromListButtonsPanel' : [ choose ] * 5,
        'EditSingleFileMetadataImporterPanel' : importer_scripts,
    }

    place( watch )

    # (the manager runs the new folder at once)
    manage( add, extra, lambda: len( listing( moved ) ) > 0 )

    step( 'add', seen )

    # ------------------------------------------------------------- 2. reopen

    seen = {}

    def look( dlg, panel ):

        seen[ 'rows when opened' ] = rows( panel )

        panel._import_folders.SelectDatas( panel._import_folders.GetData() )

        return False


    manage( look, {}, lambda: True )

    step( 'reopen', seen )

    # ----------------------------------------------------------- 3. new file

    seen = {}

    place( watch, only = 'later' )

    before = len( listing( watch ) )

    qt( lambda: gui._CheckImportFolder( NAME ) )

    flows.wait_until_idle( controller, also = lambda: len( listing( watch ) ) < before and folder_done( lambda: True )() )

    step( 'new file', seen )

    return { 'domain_manager' : domain_manager, 'sources' : hashes, 'steps' : steps }


def child( out ):

    import hydrus_driver
    import record_api

    work = tempfile.mkdtemp( prefix = 'hydrus_flow_import_folder_' )

    watch = os.path.join( work, 'watch' )
    moved = os.path.join( work, 'moved' )

    os.makedirs( watch )
    os.makedirs( moved )

    try:

        with flows.Site( SITE_PORT, PHASES ) as site:

            result = hydrus_driver.run_client( record_api.unpack_fixture( 'basic' ), lambda session: record( session, site, watch, moved ), network = True )


    finally:

        shutil.rmtree( work, ignore_errors = True )


    result[ 'site' ] = site.describe()

    flows.write_fixture( out, result )


def main():

    if len( sys.argv ) > 2 and sys.argv[1] == '--child':

        child( sys.argv[2] )

        return


    import hydrus_driver

    hydrus_driver.run_in_subprocess( os.path.abspath( __file__ ), '--child', OUT )


if __name__ == '__main__':

    main()

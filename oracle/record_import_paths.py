#!/usr/bin/env python3
"""Record how paths reach the reference's "review files to import" window
(file > import files, `ReviewLocalFileImports`): its "add files" and "add
folder" pickers, and files dropped on it or on the main window.

In the running client, on the `basic` fixture, file > import files opens the
window with no paths; then, in turn:

- "add files": the file dialog it opens (its title and mode are recorded) is
  answered with two files, then cancelled;
- "add folder": the folder picker (its title recorded) is answered with a
  folder of two pictures, then cancelled;
- a drop on the window of a file and a web URL (the URL is not a path, so the
  window takes only the file);
- a drop on the main window of a file: a second review window opens with it.

The system's dialogs are replaced by scripted answers; the drops are real Qt
drop events handed to the drop target (`FileDropTarget`) that each widget
installs, as Qt would deliver them (the offscreen platform delivers none), its
check that the drop landed in its own top-level window answered yes, as the
offscreen platform has no screen to look at. After each step, every review window's
rows (`[#, path, filetype, size]`, the work folder written `<folder>`) and
progress text are recorded, once parsing has settled.

Usage: QT_QPA_PLATFORM=offscreen python oracle/record_import_paths.py
       (writes fixtures/import_paths.json)
"""

import json
import os
import shutil
import sys
import tempfile
import time

HERE = os.path.dirname( os.path.abspath( __file__ ) )
OUT = os.path.join( HERE, 'fixtures', 'import_paths.json' )
FOLDER = os.path.join( HERE, 'fixtures', 'import_folder' )


def record( session ):

    controller = session.controller
    gui = controller.gui

    work = tempfile.mkdtemp()
    folder = os.path.join( work, 'in' )
    shutil.copytree( FOLDER, folder )

    def shown( path ):

        return path.replace( folder, '<folder>' )


    from qtpy import QtCore as QC, QtGui as QG, QtWidgets as QW
    from hydrus.core import HydrusExceptions
    from hydrus.client.gui import ClientGUIDialogsFiles, ClientGUIDialogsQuick, ClientGUIDragDrop
    from hydrus.client.gui.panels import ClientGUILocalFileImports as L

    asked = []
    script = { 'files' : None, 'folder' : None }

    class FileDialog( object ):

        def __init__( self, parent = None, message = None, fileMode = None, **kwargs ):

            mode = fileMode.name if hasattr( fileMode, 'name' ) else str( fileMode )
            asked.append( [ 'file dialog', message, mode ] )


        def __enter__( self ):

            return self


        def __exit__( self, *args ):

            return False


        def exec( self ):

            return QW.QDialog.DialogCode.Accepted if script[ 'files' ] is not None else QW.QDialog.DialogCode.Rejected


        def GetPaths( self ):

            return list( script[ 'files' ] )



    def pick_directory( parent, message, *args, **kwargs ):

        asked.append( [ 'pick directory', message ] )

        if script[ 'folder' ] is None:

            raise HydrusExceptions.CancelledException()


        return script[ 'folder' ]


    old = ( ClientGUIDialogsFiles.FileDialog, ClientGUIDialogsQuick.PickDirectory, ClientGUIDragDrop.FileDropTarget.OnDrop )

    ClientGUIDialogsFiles.FileDialog = FileDialog
    ClientGUIDialogsQuick.PickDirectory = pick_directory
    ClientGUIDragDrop.FileDropTarget.OnDrop = lambda self, x, y: True

    def panels():

        return list( gui.findChildren( L.ReviewLocalFileImports ) )


    def state():

        out = []

        for panel in panels():

            parses = sorted( panel._paths_list.GetData(), key = lambda p: p.index )

            out.append( {
                'rows' : [ [ shown( c ) for c in panel._ConvertPathToDisplayTuple( p ) ] for p in parses ],
                'progress' : panel._progress._st.text(),
            } )


        return out


    def settle():

        last = None

        for _ in range( 200 ):

            time.sleep( 0.1 )

            def done():

                return all( p._unparsed_paths_queue.empty() and p._parsed_paths_queue.empty() for p in panels() ), state()


            ( empty, now ) = controller.CallBlockingToQt( gui, done )

            if empty and now == last:

                return now


            last = now


        return last


    def drop( target, paths, urls = () ):

        mime = QC.QMimeData()
        mime.setUrls( [ QC.QUrl.fromLocalFile( p ) for p in paths ] + [ QC.QUrl( u ) for u in urls ] )

        event = QG.QDropEvent( QC.QPointF( 10, 10 ), QC.Qt.DropAction.CopyAction, mime, QC.Qt.MouseButton.NoButton, QC.Qt.KeyboardModifier.NoModifier )

        # (the drop target's own filter: Qt's offscreen platform delivers no drop events
        # it did not start a drag for)
        drop_target = [ f for f in target.children() if isinstance( f, ClientGUIDragDrop.FileDropTarget ) ][-1]

        drop_target.eventFilter( target, event )


    steps = []

    def step( name, action, files = None, folder_choice = None, given = () ):

        script[ 'files' ] = files
        script[ 'folder' ] = folder_choice
        asked.clear()

        controller.CallBlockingToQt( gui, action )

        windows = settle()

        steps.append( {
            'step' : name,
            'given' : [ shown( p ) for p in given ],
            'asked' : list( asked ),
            'windows' : windows,
        } )


    a = os.path.join( folder, 'a.png' )
    b = os.path.join( folder, 'b p12.jpg' )
    sub = os.path.join( folder, 'sub' )
    dupe = os.path.join( folder, 'dupe.gif' )
    recent = os.path.join( folder, 'recent.png' )

    try:

        step( 'file > import files', lambda: gui._ImportFiles() )
        step( 'add files', lambda: panels()[0].AddPaths(), files = [ a, b ], given = [ a, b ] )
        step( 'add files, cancelled', lambda: panels()[0].AddPaths() )
        step( 'add folder', lambda: panels()[0].AddFolder(), folder_choice = sub, given = [ sub ] )
        step( 'add folder, cancelled', lambda: panels()[0].AddFolder() )
        step( 'dropped on the window, with a web URL', lambda: drop( panels()[0].widget(), [ dupe ], [ 'https://example.com/post/1' ] ), given = [ dupe ] )
        step( 'dropped on the main window', lambda: drop( gui, [ recent ] ), given = [ recent ] )

    finally:

        ( ClientGUIDialogsFiles.FileDialog, ClientGUIDialogsQuick.PickDirectory, ClientGUIDragDrop.FileDropTarget.OnDrop ) = old


    shutil.rmtree( work, ignore_errors = True )

    return { 'steps' : steps }


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


    import hydrus_driver

    with tempfile.TemporaryDirectory() as work:

        path = os.path.join( work, 'import_paths.json' )

        hydrus_driver.run_in_subprocess( os.path.abspath( __file__ ), '--child', path )

        with open( path ) as f:

            result = json.load( f )



    with open( OUT, 'w' ) as f:

        json.dump( result, f, indent = 1, ensure_ascii = False )
        f.write( '\n' )


    print( f'wrote {OUT}: {len( result[ "steps" ] )} steps' )


if __name__ == '__main__':

    main()

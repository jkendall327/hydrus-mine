#!/usr/bin/env python3
"""Record the reference's "review files to import" dialog (file > import
files, `ReviewLocalFileImports`).

In the running client, on the `basic` fixture, the dialog is given paths as
file > import files or a drop gives them, and left to parse them: a folder
(a copy of `fixtures/import_folder`, with sidecars, a subfolder, a
duplicate and a file it can't import, and with an empty file and a
`Thumbs.db` (a picture, but not one to import) added), one of the
folder's files again, and a path that isn't
there, the folder searched with and without its subdirectories; and a
file followed by its sidecars and another text file, each given as a
path of its own. Recorded for each:

- `"rows"`: the list's rows in order, each `[#, path, filetype, size]` as
  the list shows them, the folder's path written `<folder>` and the missing
  path's `<missing>`;
- `"progress"`: the text under the list once parsing is done;
- `"enabled"`: whether "import now", "add tags/urls with the import >>",
  the pause and the stop buttons can be pressed;
- `"imported"`: the paths "import now" hands to the import page, in order.

`"delete_warning"` is the text shown as "delete original files after
successful import" is ticked, and `"empty"` the dialog's progress text and
buttons before it has any paths.

Usage: QT_QPA_PLATFORM=offscreen python oracle/record_local_import.py
       (writes fixtures/local_import_dialog.json)
"""

import json
import os
import shutil
import sys
import tempfile
import time

HERE = os.path.dirname( os.path.abspath( __file__ ) )
OUT = os.path.join( HERE, 'fixtures', 'local_import_dialog.json' )
FOLDER = os.path.join( HERE, 'fixtures', 'import_folder' )


def record( session ):

    controller = session.controller
    gui = controller.gui

    work = tempfile.mkdtemp()
    folder = os.path.join( work, 'in' )
    shutil.copytree( FOLDER, folder )

    open( os.path.join( folder, 'empty.png' ), 'wb' ).close()

    # (a picture, but by its name not one to import)
    shutil.copy( os.path.join( folder, 'a.png' ), os.path.join( folder, 'Thumbs.db' ) )

    missing = os.path.join( work, 'gone.png' )

    def shown( path ):

        if path == missing:

            return '<missing>'


        return path.replace( folder, '<folder>' )


    from hydrus.client.gui.panels import ClientGUILocalFileImports

    def state( panel ):

        return {
            'progress' : panel._progress._st.text(),
            'enabled' : {
                'import now' : panel._add_button.isEnabled(),
                'add tags/urls with the import >>' : panel._tag_button.isEnabled(),
                'pause' : panel._progress_pause.isEnabled(),
                'stop' : panel._progress_cancel.isEnabled(),
            },
        }


    def make( search_subdirectories ):

        panel = ClientGUILocalFileImports.ReviewLocalFileImports( gui )

        panel._search_subdirectories.setChecked( search_subdirectories )

        return panel


    out = { 'cases' : [] }

    panel = controller.CallBlockingToQt( gui, make, True )

    time.sleep( 0.5 )

    out[ 'empty' ] = controller.CallBlockingToQt( gui, state, panel )

    def warn( panel ):

        panel._delete_after_success.setChecked( True )
        panel.EventDeleteAfterSuccessCheck()

        return panel._delete_after_success_st.text()


    out[ 'delete_warning' ] = controller.CallBlockingToQt( gui, warn, panel )

    controller.CallBlockingToQt( gui, panel.deleteLater )

    # the folder (and one of its files again, and a missing file), with
    # and without its subfolders; and a file with its sidecars given
    # after it, as paths of their own
    given = [
        ( True, [ folder, os.path.join( folder, 'a.png' ), missing ] ),
        ( False, [ folder, os.path.join( folder, 'a.png' ), missing ] ),
        ( True, [ os.path.join( folder, name ) for name in ( 'a.png', 'a.png.txt', 'a.png.json', 'notes.txt' ) ] ),
    ]

    for ( search_subdirectories, paths ) in given:

        panel = controller.CallBlockingToQt( gui, make, search_subdirectories )

        controller.CallBlockingToQt( gui, panel._AddPathsToList, paths )

        # (parsed when the queue is empty and the text has settled)
        last = None

        for _ in range( 200 ):

            time.sleep( 0.1 )

            def done( panel ):

                return panel._unparsed_paths_queue.empty() and panel._parsed_paths_queue.empty(), panel._progress._st.text()


            ( empty, text ) = controller.CallBlockingToQt( gui, done, panel )

            if empty and text == last:

                break


            last = text


        def rows( panel ):

            parses = sorted( panel._paths_list.GetData(), key = lambda p: p.index )

            return [ [ shown( c ) for c in panel._ConvertPathToDisplayTuple( p ) ] for p in parses ]


        case = {
            'search_subdirectories' : search_subdirectories,
            'given' : [ shown( p ) for p in paths ],
            'rows' : controller.CallBlockingToQt( gui, rows, panel ),
            **controller.CallBlockingToQt( gui, state, panel ),
        }

        # "import now": the paths it hands over
        handed = []

        original = controller.pub

        def pub( topic, *args, **kwargs ):

            if topic == 'new_hdd_import':

                handed.append( [ shown( p ) for p in args[ 0 ] ] )

                return


            return original( topic, *args, **kwargs )


        controller.pub = pub

        try:

            controller.CallBlockingToQt( gui, panel._DoImport )

        finally:

            controller.pub = original


        case[ 'imported' ] = handed[ 0 ] if len( handed ) > 0 else None

        controller.CallBlockingToQt( gui, panel.deleteLater )

        out[ 'cases' ].append( case )


    shutil.rmtree( work, ignore_errors = True )

    return out


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

        path = os.path.join( work, 'local_import.json' )

        hydrus_driver.run_in_subprocess( os.path.abspath( __file__ ), '--child', path )

        with open( path ) as f:

            result = json.load( f )



    with open( OUT, 'w' ) as f:

        json.dump( result, f, indent = 1, ensure_ascii = False )
        f.write( '\n' )


    print( f'wrote {OUT}: {len( result[ "cases" ] )} cases' )


if __name__ == '__main__':

    main()

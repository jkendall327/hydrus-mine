#!/usr/bin/env python3
"""Record the reference's "force filetypes" panel
(`EditFilesForcedFiletypePanel`), as manage > force filetype opens it.

In the running client, for each case (the selected files' original
filetypes, and those forced, each as counts by filetype name), the real
panel is made and its text and choices recorded: `text` (the warning and
the summary of the files' filetypes) and `choices` (each label and the
filetype it forces, by name, or null to remove forcing), in order.

The `disk` entries record what applying the choice does to the files: the
real `set_files_forced_filetypes` (entry "action") and the
`ChangeFileExt` it calls (entry "change file ext", given the file's old
filetype) run on a jpeg of the basic fixture, once
plainly, once with the file read-only (which still counts as free to the
reference, so it is a plain move) and once with the move failing (the
reference then copies); for
each, which files exist under the old and the new extension before and
after, whether bytes and modification time were kept, and the writes the
reference made (the database `force_filetype` and the file maintenance job
queued for the copies).

Usage: QT_QPA_PLATFORM=offscreen python oracle/record_force_filetype.py
       (writes fixtures/force_filetype.json)
"""

import json
import os
import sys

HERE = os.path.dirname( os.path.abspath( __file__ ) )

sys.path.insert( 0, HERE )

OUT = os.path.join( HERE, 'fixtures', 'force_filetype.json' )

CASES = [
    { 'name' : 'two jpegs', 'original' : { 'jpeg' : 2 }, 'forced' : {} },
    { 'name' : 'mixed, some forced', 'original' : { 'jpeg' : 1, 'png' : 2, 'webm' : 1 }, 'forced' : { 'png' : 1, 'mp4' : 1 } },
    { 'name' : 'all forced', 'original' : { 'zip' : 3 }, 'forced' : { 'cbz' : 3 } },
    { 'name' : 'one thousand', 'original' : { 'static gif' : 1000, 'apng' : 1 }, 'forced' : { 'animated gif' : 1000 } },
]


def record( session ):

    controller = session.controller
    gui = controller.gui

    def f():

        from hydrus.core import HydrusConstants as HC
        from hydrus.client.gui.panels import ClientGUIScrolledPanelsEdit as M

        by_name = { name : mime for ( mime, name ) in HC.mime_string_lookup.items() }

        out = []

        for case in CASES:

            original = { by_name[ name ] : count for ( name, count ) in case[ 'original' ].items() }
            forced = { by_name[ name ] : count for ( name, count ) in case[ 'forced' ].items() }

            panel = M.EditFilesForcedFiletypePanel( gui, original, forced )

            from qtpy import QtWidgets as QW

            text = [ w.text() for w in panel.findChildren( QW.QLabel ) if w.text() != '' ]

            choice = panel._forced_mime

            choices = []

            for i in range( choice.count() ):

                mime = choice.itemData( i )

                choices.append( [ choice.itemText( i ), None if mime is None else HC.mime_string_lookup[ mime ] ] )


            out.append( { 'name' : case[ 'name' ], 'original' : case[ 'original' ], 'forced' : case[ 'forced' ], 'text' : text, 'choices' : choices, 'value' : None if choice.GetValue() is None else HC.mime_string_lookup[ choice.GetValue() ] } )

            panel.deleteLater()


        return { 'cases' : out }


    return controller.CallBlockingToQt( gui, f )


DISK_CASES = [ 'plain', 'read only', 'move fails' ]


def record_disk( case, entry ):

    def hook( session ):

        controller = session.controller
        gui = controller.gui

        def f():

            import hashlib

            from hydrus.core import HydrusConstants as HC
            from hydrus.core import HydrusPaths
            from hydrus.core import HydrusTime
            from hydrus.client import ClientGlobals as CG
            from hydrus.client.files import ClientFilesMaintenance
            from hydrus.client.gui.media import ClientGUIMediaModalActions as Actions
            from hydrus.client.media import ClientMediaSingle

            fm = CG.client_controller.client_files_manager

            candidates = controller.Read( 'media_results_from_ids', list( range( 1, 12 ) ) )

            local = [ m for m in candidates if m.GetLocationsManager().IsLocal() and not m.GetLocationsManager().IsTrashed() and m.GetMime() == HC.IMAGE_JPEG ]

            result = local[0]

            hash = result.GetHash()

            jpeg_path = fm._GenerateExpectedFilePath( hash, HC.IMAGE_JPEG )
            png_path = fm._GenerateExpectedFilePath( hash, HC.IMAGE_PNG )

            os.utime( jpeg_path, ( 1500000000, 1500000000 ) )

            original_bytes = open( jpeg_path, 'rb' ).read()

            if case == 'read only':

                os.chmod( jpeg_path, 0o444 )

            original_merge = HydrusPaths.MergeFile

            if case == 'move fails':

                def broken( source, dest ):

                    raise OSError( 'the move failed' )

                HydrusPaths.MergeFile = broken

            def files():

                out = {}

                for ( label, path ) in ( ( 'old', jpeg_path ), ( 'new', png_path ) ):

                    if os.path.exists( path ):

                        out[ label ] = { 'name' : os.path.basename( path ).replace( hash.hex(), '{hash}' ), 'same_bytes' : open( path, 'rb' ).read() == original_bytes, 'mtime' : int( os.stat( path ).st_mtime ) }

                return out

            before = files()

            writes = []

            controller_ = CG.client_controller
            original_write = controller_.WriteSynchronous

            def spy( name, *args, **kwargs ):

                if name == 'force_filetype':

                    writes.append( [ name, len( args[0] ), HC.mime_string_lookup[ args[1] ] ] )

                elif name == 'file_maintenance_add_jobs_hashes':

                    writes.append( [ name, len( args[0] ), ClientFilesMaintenance.regen_file_enum_to_str_lookup[ args[1] ], args[2] - HydrusTime.GetNow() ] )

                return original_write( name, *args, **kwargs )

            controller_.WriteSynchronous = spy

            returned = None

            try:

                if entry == 'action':

                    Actions.set_files_forced_filetypes( [ ClientMediaSingle.MediaSingle( result ) ], HC.IMAGE_PNG )

                else:

                    returned = fm.ChangeFileExt( hash, HC.IMAGE_JPEG, HC.IMAGE_PNG )

            finally:

                controller_.WriteSynchronous = original_write
                HydrusPaths.MergeFile = original_merge

            after = files()

            for w in writes:

                if w[0] == 'file_maintenance_add_jobs_hashes':

                    w[3] = round( w[3] / 100 ) * 100 # (the clock moved a moment)

            return { 'case' : case, 'entry' : entry, 'before' : before, 'after' : after, 'writes' : writes, 'needed_to_copy' : returned }

        return controller.CallBlockingToQt( gui, f )

    return hook


def child( out, case = None, entry = None ):

    import hydrus_driver
    import record_api

    db_dir = record_api.unpack_fixture( 'basic' )

    result = hydrus_driver.run_client( db_dir, record if case is None else record_disk( case, entry ) )

    with open( out, 'w' ) as f:

        json.dump( result, f, ensure_ascii = False )



def main():

    if len( sys.argv ) > 1 and sys.argv[ 1 ] == '--child':

        child( sys.argv[ 2 ], sys.argv[ 3 ] if len( sys.argv ) > 3 else None, sys.argv[ 4 ] if len( sys.argv ) > 4 else None )

        return


    import tempfile

    import hydrus_driver

    with tempfile.TemporaryDirectory() as work:

        path = os.path.join( work, 'force_filetype.json' )

        hydrus_driver.run_in_subprocess( os.path.abspath( __file__ ), '--child', path )

        with open( path ) as f:

            result = json.load( f )

        result[ 'disk' ] = []

        for case in DISK_CASES:

            for entry in ( 'action', 'change file ext' ):

                hydrus_driver.run_in_subprocess( os.path.abspath( __file__ ), '--child', path, case, entry )

                with open( path ) as f:

                    result[ 'disk' ].append( json.load( f ) )



    with open( OUT, 'w' ) as f:

        json.dump( result, f, indent = 1, ensure_ascii = False )
        f.write( '\n' )


    print( f'wrote {OUT}: {len( result[ "cases" ] )} cases' )


if __name__ == '__main__':

    main()

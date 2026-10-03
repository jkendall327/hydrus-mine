#!/usr/bin/env python3
"""Record the reference's "force filetypes" panel
(`EditFilesForcedFiletypePanel`), as manage > force filetype opens it.

In the running client, for each case (the selected files' original
filetypes, and those forced, each as counts by filetype name), the real
panel is made and its text and choices recorded: `text` (the warning and
the summary of the files' filetypes) and `choices` (each label and the
filetype it forces, by name, or null to remove forcing), in order.

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

        path = os.path.join( work, 'force_filetype.json' )

        hydrus_driver.run_in_subprocess( os.path.abspath( __file__ ), '--child', path )

        with open( path ) as f:

            result = json.load( f )



    with open( OUT, 'w' ) as f:

        json.dump( result, f, indent = 1, ensure_ascii = False )
        f.write( '\n' )


    print( f'wrote {OUT}: {len( result[ "cases" ] )} cases' )


if __name__ == '__main__':

    main()

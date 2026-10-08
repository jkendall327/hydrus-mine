#!/usr/bin/env python3
"""Record where the reference opens a child window from its frame location.

`SetInitialTLWSizeAndPosition` sizes a window by its frame's default gravity
(`GetSafeSize`: the size hint, or the main window's available size less the
child padding) and places it at the main window's top-left, centre or the
mouse (or at its remembered size and place). In the running client, on the
`basic` fixture, with the main window at a fixed place and size, this opens a
dialog of a fixed size hint for each frame setting and records the size and
place it was given, with the main window's and the screen's geometry.

Usage: QT_QPA_PLATFORM=offscreen python oracle/record_frame_placement.py
       (writes fixtures/frame_placement.json)
"""

import json
import os
import sys

HERE = os.path.dirname( os.path.abspath( __file__ ) )

sys.path.insert( 0, HERE )

OUT = os.path.join( HERE, 'fixtures', 'frame_placement.json' )

# ( remember_size, remember_position, last_size, last_position, gravity, position )
CASES = [
    ( False, False, None, None, ( -1, -1 ), 'topleft' ),
    ( False, False, None, None, ( 1, -1 ), 'topleft' ),
    ( False, False, None, None, ( -1, 1 ), 'topleft' ),
    ( False, False, None, None, ( 1, 1 ), 'topleft' ),
    ( False, False, None, None, ( -1, -1 ), 'center' ),
    ( False, False, None, None, ( 1, 1 ), 'center' ),
    ( False, False, None, None, ( -1, -1 ), 'mouse' ),
    ( True, True, ( 410, 310 ), ( 33, 44 ), ( 1, 1 ), 'center' ),
    ( True, False, ( 410, 310 ), ( 33, 44 ), ( -1, -1 ), 'topleft' ),
    ( False, True, ( 410, 310 ), ( 33, 44 ), ( -1, -1 ), 'topleft' ),
    # remembering with nothing remembered
    ( True, True, None, None, ( -1, -1 ), 'center' ),
]

MAIN = ( 120, 80, 900, 700 )

HINT = ( 300, 200 )


def record( session ):

    controller = session.controller
    gui = controller.gui

    from qtpy import QtGui as QG
    from qtpy import QtWidgets as QW

    from hydrus.client.gui import ClientGUITopLevelWindows

    result = {}

    def work():

        gui.showNormal()
        gui.resize( MAIN[2], MAIN[3] )
        gui.move( MAIN[0], MAIN[1] )

        QW.QApplication.processEvents()

        screen = gui.screen().geometry()

        result[ 'screen' ] = [ screen.x(), screen.y(), screen.width(), screen.height() ]
        result[ 'main' ] = [ gui.frameGeometry().x(), gui.frameGeometry().y(), gui.frameGeometry().width(), gui.frameGeometry().height() ]
        result[ 'main_client' ] = [ gui.width(), gui.height() ]
        result[ 'hint' ] = list( HINT )
        result[ 'child_position_padding' ] = ClientGUITopLevelWindows.CHILD_POSITION_PADDING

        QG.QCursor.setPos( 700, 500 )

        QW.QApplication.processEvents()

        from hydrus.client.gui import ClientGUIFunctions

        mouse = ClientGUIFunctions.GetMousePos()

        result[ 'mouse' ] = [ mouse.x(), mouse.y() ]

        cases = []

        for ( remember_size, remember_position, last_size, last_position, gravity, position ) in CASES:

            controller.new_options.SetFrameLocation( 'regular_dialog', remember_size, remember_position, last_size, last_position, gravity, position, False, False )

            dialog = QW.QDialog( gui )

            layout = QW.QVBoxLayout( dialog )

            filler = QW.QWidget( dialog )
            filler.setMinimumSize( HINT[0], HINT[1] )
            filler.setMaximumSize( HINT[0], HINT[1] )

            layout.setContentsMargins( 0, 0, 0, 0 )
            layout.addWidget( filler )

            ClientGUITopLevelWindows.SetInitialTLWSizeAndPosition( dialog, 'regular_dialog' )

            dialog.show()

            QW.QApplication.processEvents()

            cases.append( {
                'frame' : {
                    'remember_size' : remember_size,
                    'remember_position' : remember_position,
                    'last_size' : last_size,
                    'last_position' : last_position,
                    'gravity' : list( gravity ),
                    'position' : position,
                },
                'size' : [ dialog.width(), dialog.height() ],
                'position' : [ dialog.frameGeometry().x(), dialog.frameGeometry().y() ],
            } )

            dialog.close()
            dialog.deleteLater()

        result[ 'cases' ] = cases

    controller.CallBlockingToQt( gui, work )

    return result


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

        path = os.path.join( work, 'frame_placement.json' )

        hydrus_driver.run_in_subprocess( os.path.abspath( __file__ ), '--child', path )

        with open( path ) as f:

            result = json.load( f )

    with open( OUT, 'w' ) as f:

        json.dump( result, f, indent = 1, ensure_ascii = False )
        f.write( '\n' )

    print( f'wrote {OUT}: {len( result[ "cases" ] )} cases' )


if __name__ == '__main__':

    main()

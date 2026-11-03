#!/usr/bin/env python3
"""Record the window titles Linux shows for the reference's windows.

The reference puts the application display name on every window through
`QApplication.setApplicationDisplayName` (`UpdateAppDisplayName`); Qt's
platform plugin composes what the window manager is given, so offscreen Qt
shows nothing of it. This runs the client on X11 (Xvfb, Qt's xcb plugin) on
the `basic` fixture, and for each display name the Options panel can save
(as `gui_settings.json` records them) reads the `_NET_WM_NAME` the window
manager would show, with xprop, for: the main window; a frame opened before
the name changed (retitled by `UpdateAppDisplayName`); one opened after; and
a frame with an empty title.

Usage: QT_QPA_PLATFORM=xcb xvfb-run -a ~/pyenv/bin/python oracle/record_window_titles.py
       (writes fixtures/window_titles.json; needs xprop and Qt's xcb
       libraries: libxcb-cursor0 libxcb-icccm4 libxcb-keysyms1 libxcb-xkb1
       libxkbcommon-x11-0)
"""

import json
import os
import subprocess
import sys

HERE = os.path.dirname( os.path.abspath( __file__ ) )

sys.path.insert( 0, HERE )

OUT = os.path.join( HERE, 'fixtures', 'window_titles.json' )

# what is typed into "Application display name:"
TYPED = [ 'night library', '', '  ' ]


def wm_name( widget ):

    out = subprocess.run( [ 'xprop', '-id', str( int( widget.winId() ) ), '_NET_WM_NAME' ], capture_output = True, text = True, check = True ).stdout.strip()

    # _NET_WM_NAME(UTF8_STRING) = "..."
    value = out.split( ' = ', 1 )[ 1 ]

    return json.loads( value )


def record( session ):

    controller = session.controller
    gui = controller.gui

    from qtpy import QtWidgets as QW

    from hydrus.client.gui import ClientGUIFunctions
    from hydrus.client.gui import ClientGUITopLevelWindowsPanels
    from hydrus.core import HydrusConstants as HC

    result = { 'platform' : None, 'version' : HC.SOFTWARE_VERSION, 'names' : [] }

    def settle():

        for _ in range( 50 ):

            QW.QApplication.processEvents()



    def frame( title ):

        f = ClientGUITopLevelWindowsPanels.FrameThatTakesScrollablePanel( gui, title )

        f.show()

        settle()

        return f


    def work():

        result[ 'platform' ] = QW.QApplication.platformName()

        assert result[ 'platform' ] == 'xcb', result[ 'platform' ]

        gui.show()

        before = frame( 'review services' )

        for typed in TYPED:

            # as the Options panel saves it
            saved = typed if typed != '' else 'hydrus client'

            controller.new_options.SetString( 'app_display_name', saved )

            ClientGUIFunctions.UpdateAppDisplayName()

            settle()

            after = frame( 'manage options' )
            untitled = frame( '' )

            result[ 'names' ].append( {
                'typed' : typed,
                'saved' : saved,
                'application_display_name' : QW.QApplication.applicationDisplayName(),
                'main' : { 'title' : gui.windowTitle(), 'shown' : wm_name( gui ) },
                'opened_before' : { 'title' : before.windowTitle(), 'shown' : wm_name( before ) },
                'opened_after' : { 'title' : after.windowTitle(), 'shown' : wm_name( after ) },
                'untitled' : { 'title' : untitled.windowTitle(), 'shown' : wm_name( untitled ) }
            } )

            for f in ( after, untitled ):

                f.close()
                f.deleteLater()


            settle()


        before.close()
        before.deleteLater()


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

        path = os.path.join( work, 'window_titles.json' )

        hydrus_driver.run_in_subprocess( os.path.abspath( __file__ ), '--child', path )

        with open( path ) as f:

            result = json.load( f )



    with open( OUT, 'w' ) as f:

        json.dump( result, f, indent = 1, ensure_ascii = False )
        f.write( '\n' )


    print( f'wrote {OUT}: {len( result[ "names" ] )} names' )


if __name__ == '__main__':

    main()

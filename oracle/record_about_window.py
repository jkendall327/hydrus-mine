#!/usr/bin/env python3
"""Record the reference's about window (help > about, `ShowAboutWindow`).

The window ("about hydrus") shows the hydrus icon, the name, the version
("v688, using network version 20"), a link to the site, and four tabs:
"Description" (a sentence, then the platform, library versions, boot
time, directories and database settings, a line each), "Optional
Libraries" (each "name: yes" or "name: not available"), "Credits"
("Created by Anonymous") and "License" (the LICENSE file).

On the `basic` fixture, this opens the window and records its title, the
header's texts, the tabs' names and each tab's text. The values on the
description's lines depend on the machine; hydrus-rs's tests compare the
lines' labels and forms.

Usage: QT_QPA_PLATFORM=offscreen python oracle/record_about_window.py
       (writes fixtures/about_window.json)
"""

import json
import os
import sys

HERE = os.path.dirname( os.path.abspath( __file__ ) )

sys.path.insert( 0, HERE )

OUT = os.path.join( HERE, 'fixtures', 'about_window.json' )


def record( session ):

    controller = session.controller
    gui = controller.gui

    def f():

        from hydrus.client.gui import ClientGUIAboutWindow
        from hydrus.client.gui import ClientGUITopLevelWindowsPanels

        seen = {}

        real_frame = ClientGUITopLevelWindowsPanels.FrameThatTakesScrollablePanel

        class Frame( real_frame ):

            def __init__( self, parent, title, *args, **kwargs ):

                super().__init__( parent, title, *args, **kwargs )

                seen[ 'title' ] = title


            def SetPanel( self, panel ):

                super().SetPanel( panel )

                seen[ 'panel' ] = panel



        ClientGUIAboutWindow.ClientGUITopLevelWindowsPanels.FrameThatTakesScrollablePanel = Frame

        ClientGUIAboutWindow.ShowAboutWindow( gui )

        from qtpy import QtWidgets as QW

        panel = seen[ 'panel' ]

        labels = [ w.text() for w in panel.findChildren( QW.QLabel ) if w.text() != '' ]

        tabs = panel.findChildren( QW.QTabWidget )[0]

        def tab_text( w ):

            if isinstance( w, QW.QTextEdit ):

                return w.toPlainText()


            return w.text()


        out = {
            'title' : seen[ 'title' ],
            'labels' : labels,
            'tabs' : [ [ tabs.tabText( i ), tab_text( tabs.widget( i ) ) ] for i in range( tabs.count() ) ],
        }

        panel.window().close()

        return out


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

        path = os.path.join( work, 'about_window.json' )

        hydrus_driver.run_in_subprocess( os.path.abspath( __file__ ), '--child', path )

        with open( path ) as f:

            result = json.load( f )



    with open( OUT, 'w' ) as f:

        json.dump( result, f, indent = 1, ensure_ascii = False )
        f.write( '\n' )


    print( f'wrote {OUT}' )


if __name__ == '__main__':

    main()

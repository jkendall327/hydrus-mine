#!/usr/bin/env python3
"""Record the reference's thumbnail grid under real pointer events: left
click (plain, ctrl, shift, on empty space), double-click, middle press and
right press.

The grid is v688's default, `MediaResultsPanelThumbnailsGraphicsViewTest`
(the option `test_thumbnails_graphics_view` is on in a new client,
`ClientOptions.py:371`): its `mousePressEvent`
(`ClientGUIMediaResultsPanelThumbnails.py:4099`) selects on a left or right
press on a thumbnail and a left press on empty space, opens the viewer on a
middle press on a thumbnail (`ShowMediaFullScreen`) and does nothing for a
middle press on empty space; `mouseDoubleClickEvent` (`:4023`) opens the
viewer; `contextMenuEvent` (`:3360`) shows the menu.

In the running client, a page of the `basic` fixture's files is opened on
"my files" and `QTest` presses are sent to the view's viewport at a
thumbnail's centre, or at a point between thumbnails.
`_LaunchMediaViewer` is replaced to note which file the viewer would open
on (and not open it), and `PopupMenu` to note that a menu was shown and its
first entries (and not run it). After each step: the files selected and the
one focused, by their place in the page, the viewer launches and the menus.

Usage: QT_QPA_PLATFORM=offscreen ~/pyenv/bin/python oracle/record_thumbnail_pointer.py
       (writes fixtures/thumbnail_pointer.json)
"""

import json
import sys
import tempfile
import time
from pathlib import Path

HERE = Path( __file__ ).resolve().parent

sys.path.insert( 0, str( HERE ) )

# ( button, file or None (the margin), ctrl, shift, double )
SCRIPT = [
    ( 'left', 0, False, False, False ),
    ( 'middle', 3, False, False, False ),
    ( 'left', 1, False, False, False ),
    ( 'middle', 4, True, False, False ),
    ( 'middle', 6, False, True, False ),
    ( 'middle', None, False, False, False ),
    ( 'left', 2, False, False, False ),
    ( 'left', 5, True, False, False ),
    ( 'left', 7, False, True, False ),
    ( 'left', None, False, False, False ),
    ( 'left', 3, False, False, True ),
    ( 'right', 1, False, False, False ),
    ( 'right', None, False, False, False ),
    ( 'right', 2, False, False, False ),
    ( 'left', 4, True, False, False ),
    ( 'right', 4, False, False, False ),
]


def record( session ):

    from qtpy import QtCore as QC
    from qtpy import QtWidgets as QW
    from qtpy import QtTest as QT

    from hydrus.client import ClientConstants as CC
    from hydrus.client import ClientLocation
    from hydrus.client.gui import ClientGUICore
    from hydrus.client.gui.pages import ClientGUIMediaResultsPanel

    controller = session.controller
    gui = controller.gui

    manifest = json.loads( ( HERE / 'fixtures' / 'legacy_db' / 'basic.manifest.json' ).read_text() )

    hashes = [ bytes.fromhex( f[ 'hash' ] ) for f in manifest[ 'files' ] ]

    def qt( f ):

        return controller.CallBlockingToQt( gui, f )


    page = qt( lambda: gui._notebook.NewPageQuery( ClientLocation.LocationContext.STATICCreateSimple( CC.LOCAL_FILE_SERVICE_KEY ), initial_hashes = hashes ) )

    for _ in range( 400 ):

        panel = qt( page.GetMediaResultsPanel )

        if hasattr( panel, '_media_to_thumbnails' ) and len( panel._sorted_media ) > 0 and len( panel._media_to_thumbnails ) == len( panel._sorted_media ):

            break


        time.sleep( 0.05 )

    else:

        raise RuntimeError( 'the page never loaded: {} {}'.format( type( panel ), len( getattr( panel, '_sorted_media', [] ) ) ) )


    time.sleep( 1 )

    launched = []
    menus = []

    def launch( self, first_media = None ):

        launched.append( first_media )


    ClientGUIMediaResultsPanel.MediaResultsPanel._LaunchMediaViewer = launch
    ClientGUIMediaResultsPanel.MediaResultsPanelGraphicsViewTest._LaunchMediaViewer = launch

    def popup( self, widget, menu ):

        actions = [ a.text() for a in menu.actions() if not a.isSeparator() ]

        menus.append( actions[ : 3 ] )


    ClientGUICore.GUICore.PopupMenu = popup

    def drive():

        media = list( panel._sorted_media )

        index = { m : i for ( i, m ) in enumerate( media ) }

        widget = panel.viewport()

        def where( i ):

            if i is None:

                # in the margin between the first two thumbnails
                ( a, b ) = ( panel._media_to_thumbnails[ media[ k ] ].sceneBoundingRect() for k in ( 0, 1 ) )

                point = panel.mapFromScene( QC.QPointF( ( a.right() + b.left() ) / 2, a.center().y() ) )

                assert panel.itemAt( point ) is None

                return point


            rect = panel._media_to_thumbnails[ media[ i ] ].sceneBoundingRect()

            point = panel.mapFromScene( rect.center() )

            assert panel._media_to_thumbnails[ media[ i ] ] is panel.itemAt( point )

            return point


        buttons = { 'left' : QC.Qt.MouseButton.LeftButton, 'middle' : QC.Qt.MouseButton.MiddleButton, 'right' : QC.Qt.MouseButton.RightButton }

        steps = []

        for ( button, i, ctrl, shift, double ) in SCRIPT:

            del launched[ : ]
            del menus[ : ]

            modifiers = QC.Qt.KeyboardModifier.NoModifier

            if ctrl:

                modifiers |= QC.Qt.KeyboardModifier.ControlModifier


            if shift:

                modifiers |= QC.Qt.KeyboardModifier.ShiftModifier


            point = where( i )

            if double:

                # (a user's double-click: a click, then the second press,
                # which Qt delivers as the double-click)
                QT.QTest.mouseClick( widget, buttons[ button ], modifiers, point )
                QT.QTest.mouseDClick( widget, buttons[ button ], modifiers, point )

            else:

                QT.QTest.mouseClick( widget, buttons[ button ], modifiers, point )


            if button == 'right' and len( menus ) == 0:

                # (as the platform does after a right press, if QTest did not)
                QW.QApplication.sendEvent( widget, QG_context( point, modifiers ) )


            QW.QApplication.processEvents()

            steps.append( {
                'at' : [ point.x(), point.y() ],
                'button' : button,
                'file' : i,
                'ctrl' : ctrl,
                'shift' : shift,
                'double' : double,
                'selected' : sorted( index[ m ] for m in panel._selected_media ),
                'focused' : index.get( panel._focused_media ),
                'launched' : [ index.get( m ) if m is not None else None for m in launched ],
                'menus' : list( menus ),
            } )


        return { 'graphics_view' : type( panel ).__name__, 'files' : [ m.GetHash().hex() for m in media ], 'steps' : steps }


    return qt( drive )


def QG_context( point, modifiers ):

    from qtpy import QtGui as QG

    return QG.QContextMenuEvent( QG.QContextMenuEvent.Reason.Mouse, point, point, modifiers )


def main():

    import hydrus_driver
    import record_api

    if len( sys.argv ) > 1 and sys.argv[ 1 ] == '--child':

        from hydrus.client.gui.canvas import ClientGUIMPV

        ClientGUIMPV.MPV_IS_AVAILABLE = False

        Path( sys.argv[ 2 ] ).write_text( json.dumps( hydrus_driver.run_client( record_api.unpack_fixture( 'basic' ), record ) ) )

        return


    with tempfile.TemporaryDirectory() as directory:

        out = Path( directory ) / 'out.json'

        hydrus_driver.run_in_subprocess( str( Path( __file__ ).resolve() ), '--child', str( out ) )

        result = json.loads( out.read_text() )


    target = HERE / 'fixtures' / 'thumbnail_pointer.json'

    target.write_text( json.dumps( result, indent = 1 ) + '\n' )

    print( 'wrote', target )


if __name__ == '__main__':

    main()

#!/usr/bin/env python3
"""Record the keys of the reference's lists (`BetterListCtrlTreeView`, a
`QTreeView` in extended selection, `ClientGUIListCtrl.py:904-938`): the
arrows, Home/End, PageUp/PageDown (with and without shift and ctrl),
Ctrl+Space, Ctrl+A, Enter and Ctrl+C.

In the running client, on the `basic` fixture:

- navigation: the "review files to import" list of `ReviewLocalFileImports`
  (the list `record_list_selection.py` records clicks on) is given thirty
  files. Each sequence starts from a fresh panel, with nothing current, and
  is a run of clicks and key presses sent to the list as real events. After
  every step: the rows selected (sorted), the order the list hands them out
  in (`selectedRows`), the current row, and, once per panel, the viewport's
  height and a row's height (what a page is).
- Enter and Ctrl+C: a file log (`EditFileSeedCachePanel`), the one list
  that supplies both an activation (`_ShowSelectionInNewPage`, which opens a
  page of the selected rows' files) and a copy-rows callable (the selected
  rows' sources). The page asked for and the clipboard are captured. Ctrl+C
  is also pressed on the import list, which supplies no copy callable, to
  see what `QTreeView`'s own copy does.

Usage: QT_QPA_PLATFORM=offscreen ~/pyenv/bin/python oracle/record_list_keys.py
       (writes fixtures/list_keys.json)
"""

import json
import os
import shutil
import sys
import tempfile
import time
from pathlib import Path

HERE = Path( __file__ ).resolve().parent

sys.path.insert( 0, str( HERE ) )

MEDIA = [ 'bmp_24.bmp', 'apng_rgba.png', 'gif_static.gif', 'png_rgba.png', 'bmp_8bit.bmp', 'bmp_gray.bmp' ]

COUNT = 30

PORT = 45991

# steps: ( 'click', row, ctrl, shift ), ( 'key', name, ctrl, shift ),
# ( 'drag', ( from, to ), ctrl, shift ) (pressed on one row, moved to the
# other, released), ( 'double', row, ... ) and ( 'right', row, ... )
SEQUENCES = [
    ( 'down with nothing current', [ ( 'key', 'down', False, False ), ( 'key', 'down', False, False ) ] ),
    ( 'up with nothing current', [ ( 'key', 'up', False, False ) ] ),
    ( 'end with nothing current', [ ( 'key', 'end', False, False ) ] ),
    ( 'arrows move the selection', [ ( 'click', 3, False, False ), ( 'key', 'down', False, False ), ( 'key', 'down', False, False ), ( 'key', 'up', False, False ) ] ),
    ( 'up at the top, down at the bottom', [ ( 'click', 0, False, False ), ( 'key', 'up', False, False ), ( 'click', COUNT - 1, False, False ), ( 'key', 'down', False, False ) ] ),
    ( 'shift extends from the click', [ ( 'click', 5, False, False ), ( 'key', 'down', False, True ), ( 'key', 'down', False, True ), ( 'key', 'up', False, True ), ( 'key', 'up', False, True ), ( 'key', 'up', False, True ) ] ),
    ( 'shift after a shift-click', [ ( 'click', 2, False, False ), ( 'click', 5, False, True ), ( 'key', 'down', False, True ), ( 'key', 'down', False, False ) ] ),
    ( 'ctrl moves only the current row', [ ( 'click', 2, False, False ), ( 'key', 'down', True, False ), ( 'key', 'down', True, False ), ( 'key', 'space', True, False ), ( 'key', 'down', False, True ) ] ),
    ( 'ctrl-space toggles the current row', [ ( 'click', 4, False, False ), ( 'key', 'space', True, False ), ( 'key', 'space', True, False ) ] ),
    ( 'ctrl+shift arrows', [ ( 'click', 1, False, False ), ( 'click', 6, True, False ), ( 'key', 'down', True, True ), ( 'key', 'down', True, True ) ] ),
    ( 'ctrl-click then shift arrow', [ ( 'click', 1, False, False ), ( 'click', 6, True, False ), ( 'key', 'down', False, True ) ] ),
    ( 'home and end', [ ( 'click', 10, False, False ), ( 'key', 'home', False, False ), ( 'key', 'end', False, False ), ( 'key', 'home', False, True ) ] ),
    ( 'shift end', [ ( 'click', 25, False, False ), ( 'key', 'end', False, True ) ] ),
    ( 'page down and up', [ ( 'click', 0, False, False ), ( 'key', 'pagedown', False, False ), ( 'key', 'pagedown', False, False ), ( 'key', 'pagedown', False, False ), ( 'key', 'pageup', False, False ) ] ),
    ( 'shift page down', [ ( 'click', 2, False, False ), ( 'key', 'pagedown', False, True ) ] ),
    ( 'ctrl+a', [ ( 'click', 4, False, False ), ( 'key', 'a', True, False ) ] ),
    ( 'ctrl+a with nothing current', [ ( 'key', 'a', True, False ) ] ),
    ( 'ctrl+a then shift arrow', [ ( 'click', 4, False, False ), ( 'key', 'a', True, False ), ( 'key', 'down', False, True ) ] ),
    ( 'ctrl+a then shift click', [ ( 'click', 4, False, False ), ( 'key', 'a', True, False ), ( 'click', 8, False, True ) ] ),
    ( 'ctrl+a then arrow', [ ( 'click', 4, False, False ), ( 'key', 'a', True, False ), ( 'key', 'down', False, False ) ] ),
    ( 'ctrl move then shift arrow', [ ( 'click', 2, False, False ), ( 'key', 'down', True, False ), ( 'key', 'down', True, False ), ( 'key', 'down', False, True ) ] ),
    ( 'ctrl move then shift click', [ ( 'click', 2, False, False ), ( 'key', 'down', True, False ), ( 'key', 'down', True, False ), ( 'click', 7, False, True ) ] ),
    ( 'space selects the current row', [ ( 'click', 2, False, False ), ( 'key', 'down', True, False ), ( 'key', 'down', True, False ), ( 'key', 'space', False, False ), ( 'key', 'space', False, False ), ( 'key', 'down', False, True ) ] ),
    ( 'shift space', [ ( 'click', 2, False, False ), ( 'key', 'down', True, False ), ( 'key', 'down', True, False ), ( 'key', 'space', False, True ) ] ),
    ( 'drag down', [ ( 'drag', ( 1, 4 ), False, False ) ] ),
    ( 'drag up', [ ( 'drag', ( 6, 3 ), False, False ) ] ),
    ( 'drag then shift arrow', [ ( 'drag', ( 1, 4 ), False, False ), ( 'key', 'down', False, True ) ] ),
    ( 'arrow after ctrl-click off', [ ( 'click', 4, False, False ), ( 'click', 4, True, False ), ( 'key', 'down', False, False ) ] ),
]


def record( session ):

    from qtpy import QtCore as QC
    from qtpy import QtWidgets as QW
    from qtpy import QtTest as QT
    from qtpy import QtGui as QG

    from hydrus.client import ClientConstants as CC
    from hydrus.client.gui import ClientGUICore
    from hydrus.client.gui.importing import ClientGUIFileSeedCache
    from hydrus.client.gui.panels import ClientGUILocalFileImports
    from hydrus.client.importing import ClientImportFileSeeds as S

    controller = session.controller
    api = session.api
    gui = controller.gui

    menus = []

    def popup( self, widget, menu ):

        menus.append( [ a.text() for a in menu.actions() if not a.isSeparator() ][ : 3 ] )


    ClientGUICore.GUICore.PopupMenu = popup

    keys = {
        'down' : QC.Qt.Key.Key_Down,
        'up' : QC.Qt.Key.Key_Up,
        'home' : QC.Qt.Key.Key_Home,
        'end' : QC.Qt.Key.Key_End,
        'pagedown' : QC.Qt.Key.Key_PageDown,
        'pageup' : QC.Qt.Key.Key_PageUp,
        'space' : QC.Qt.Key.Key_Space,
        'a' : QC.Qt.Key.Key_A,
        'c' : QC.Qt.Key.Key_C,
        'insert' : QC.Qt.Key.Key_Insert,
        'enter' : QC.Qt.Key.Key_Return,
        'keypad enter' : QC.Qt.Key.Key_Enter,
    }

    def modifiers_of( ctrl, shift ):

        modifiers = QC.Qt.KeyboardModifier.NoModifier

        if ctrl:

            modifiers |= QC.Qt.KeyboardModifier.ControlModifier


        if shift:

            modifiers |= QC.Qt.KeyboardModifier.ShiftModifier


        return modifiers


    def press( view, kind, what, ctrl, shift ):

        modifiers = modifiers_of( ctrl, shift )

        def centre( row ):

            index = view.model().index( row, 0 )

            view.scrollTo( index )

            QW.QApplication.processEvents()

            return view.visualRect( index ).center()


        if kind == 'click':

            QT.QTest.mouseClick( view.viewport(), QC.Qt.MouseButton.LeftButton, modifiers, centre( what ) )

        elif kind == 'double':

            # (a user's double-click: a click, then the second press, which
            # Qt delivers as the double-click)
            QT.QTest.mouseClick( view.viewport(), QC.Qt.MouseButton.LeftButton, modifiers, centre( what ) )
            QT.QTest.mouseDClick( view.viewport(), QC.Qt.MouseButton.LeftButton, modifiers, centre( what ) )

        elif kind == 'right':

            point = centre( what )

            QT.QTest.mouseClick( view.viewport(), QC.Qt.MouseButton.RightButton, modifiers, point )

            if len( menus ) == 0:

                # (as the platform does after a right press, if QTest did not)
                QW.QApplication.sendEvent( view.viewport(), QG.QContextMenuEvent( QG.QContextMenuEvent.Reason.Mouse, point, view.viewport().mapToGlobal( point ), modifiers ) )

                # (the rows menu is shown a moment later)
                for _ in range( 20 ):

                    QW.QApplication.processEvents()



        elif kind == 'drag':

            ( a, b ) = what

            ( start, end ) = ( centre( a ), centre( b ) )

            QT.QTest.mousePress( view.viewport(), QC.Qt.MouseButton.LeftButton, modifiers, start )

            step = 1 if b > a else -1

            for row in range( a + step, b + step, step ):

                # (QTest's move carries no held button; send the move Qt
                # would)
                point = centre( row )

                move = QG.QMouseEvent( QC.QEvent.Type.MouseMove, QC.QPointF( point ), QC.QPointF( view.viewport().mapToGlobal( point ) ), QC.Qt.MouseButton.NoButton, QC.Qt.MouseButton.LeftButton, modifiers )

                QW.QApplication.sendEvent( view.viewport(), move )

                QW.QApplication.processEvents()


            QT.QTest.mouseRelease( view.viewport(), QC.Qt.MouseButton.LeftButton, modifiers, end )

        else:

            QT.QTest.keyClick( view, keys[ what ], modifiers )


        QW.QApplication.processEvents()


    def state_of( view ):

        rows = [ i.row() for i in view.selectionModel().selectedRows() ]

        current = view.currentIndex()

        return { 'selected' : sorted( rows ), 'order' : rows, 'current' : current.row() if current.isValid() else -1 }


    # navigation

    folder = tempfile.mkdtemp()

    paths = []

    for i in range( COUNT ):

        name = MEDIA[ i % len( MEDIA ) ]

        path = os.path.join( folder, '{:02}_{}'.format( i, name ) )

        shutil.copy( HERE / 'fixtures' / 'media' / name, path )

        paths.append( path )


    holder = {}

    def make():

        panel = ClientGUILocalFileImports.ReviewLocalFileImports( gui, paths = paths )
        panel.resize( 900, 600 )
        panel.show()
        holder[ 'panel' ] = panel


    def row_count():

        return holder[ 'panel' ]._paths_list.model().rowCount()


    def fresh_panel():

        if 'panel' in holder:

            controller.CallBlockingToQt( gui, lambda: holder[ 'panel' ].deleteLater() )


        controller.CallBlockingToQt( gui, make )

        for _ in range( 200 ):

            if controller.CallBlockingToQt( gui, row_count ) == COUNT:

                break


            time.sleep( 0.1 )


        time.sleep( 0.5 )

        def focus():

            view = holder[ 'panel' ]._paths_list
            view.activateWindow()
            view.setFocus()
            QW.QApplication.processEvents()


        controller.CallBlockingToQt( gui, focus )


    def geometry():

        view = holder[ 'panel' ]._paths_list

        return {
            'viewport_height' : view.viewport().height(),
            'row_height' : view.visualRect( view.model().index( 0, 0 ) ).height(),
            'rows' : view.model().rowCount(),
            'start' : state_of( view )
        }


    sequences = []

    page = None

    for ( label, steps ) in SEQUENCES:

        fresh_panel()

        if page is None:

            page = controller.CallBlockingToQt( gui, geometry )


        def step( kind, what, ctrl, shift ):

            view = holder[ 'panel' ]._paths_list

            press( view, kind, what, ctrl, shift )

            result = { 'kind' : kind, 'what' : what, 'ctrl' : ctrl, 'shift' : shift }

            result.update( state_of( view ) )

            return result


        out = [ controller.CallBlockingToQt( gui, lambda k = k, w = w, c = c, s = s: step( k, w, c, s ) ) for ( k, w, c, s ) in steps ]

        sequences.append( { 'name' : label, 'steps' : out } )


    # Ctrl+C on a list with no copy callable: Qt's own copy of the current
    # cell, if anything

    fresh_panel()

    def no_callable_copy():

        view = holder[ 'panel' ]._paths_list

        clipboard = QW.QApplication.clipboard()

        clipboard.setText( 'before' )

        press( view, 'click', 3, False, False )
        press( view, 'click', 5, False, True )

        QT.QTest.keyClick( view, keys[ 'c' ], modifiers_of( True, False ) )

        QW.QApplication.processEvents()

        current = view.currentIndex()

        return {
            'selected' : sorted( i.row() for i in view.selectionModel().selectedRows() ),
            'current' : current.row(),
            'current_cells' : [ view.model().index( current.row(), c ).data() for c in range( view.model().columnCount() ) ],
            'clipboard' : clipboard.text()
        }


    no_copy = controller.CallBlockingToQt( gui, no_callable_copy )

    controller.CallBlockingToQt( gui, lambda: holder[ 'panel' ].deleteLater() )

    # Enter and Ctrl+C on the file log

    time.sleep( 1 )

    hashes = sorted( api.get( '/get_files/search_files', tags = json.dumps( [ 'system:everything' ] ), return_hashes = 'true' )[ 'hashes' ] )

    pubs = []

    real_pub = controller.pub

    def pub( topic, *args, **kwargs ):

        if topic == 'clipboard':

            pubs.append( { 'topic' : topic, 'args' : [ a if isinstance( a, str ) else repr( a ) for a in args ] } )

            return


        if topic == 'new_page_query':

            pubs.append( { 'topic' : topic, 'hashes' : [ h.hex() for h in kwargs.get( 'initial_hashes', [] ) ] } )

            return


        return real_pub( topic, *args, **kwargs )


    controller.pub = pub

    seeds_plan = [ ( 'https://example.com/post/{}'.format( i + 1 ), hashes[ i ] if i < 3 else None ) for i in range( 5 ) ]

    def make_log():

        seeds = []

        for ( url, hash_hex ) in seeds_plan:

            s = S.FileSeed( S.FILE_SEED_TYPE_URL, url )

            if hash_hex is not None:

                s.SetStatus( CC.STATUS_SUCCESSFUL_AND_NEW )
                s.SetHash( bytes.fromhex( hash_hex ) )


            seeds.append( s )


        cache = S.FileSeedCache()

        cache.AddFileSeeds( seeds )

        panel = ClientGUIFileSeedCache.EditFileSeedCachePanel( gui, cache )
        panel.resize( 900, 600 )
        panel.show()

        holder[ 'log' ] = panel

        view = panel._list_ctrl
        view.activateWindow()
        view.setFocus()

        QW.QApplication.processEvents()


    LOG_SEQUENCES = [
        ( 'enter on a range', [ ( 'click', 0, False, False ), ( 'key', 'down', False, True ), ( 'key', 'enter', False, False ) ] ),
        ( 'keypad enter', [ ( 'click', 1, False, False ), ( 'key', 'keypad enter', False, False ) ] ),
        ( 'enter with nothing selected', [ ( 'click', 1, False, False ), ( 'click', 1, True, False ), ( 'key', 'enter', False, False ) ] ),
        ( 'enter on rows with no file', [ ( 'click', 3, False, False ), ( 'key', 'down', False, True ), ( 'key', 'enter', False, False ) ] ),
        ( 'ctrl+c', [ ( 'click', 1, False, False ), ( 'click', 3, True, False ), ( 'key', 'c', True, False ) ] ),
        ( 'ctrl+insert', [ ( 'click', 2, False, False ), ( 'key', 'down', False, True ), ( 'key', 'insert', True, False ) ] ),
        ( 'ctrl+c with nothing selected', [ ( 'click', 1, False, False ), ( 'click', 1, True, False ), ( 'key', 'c', True, False ) ] ),
        ( 'double-click', [ ( 'click', 0, False, False ), ( 'click', 2, False, True ), ( 'double', 1, False, False ) ] ),
        ( 'right-click a row', [ ( 'click', 0, False, False ), ( 'click', 2, False, True ), ( 'right', 1, False, False ), ( 'right', 4, False, False ) ] ),
    ]

    log_sequences = []

    for ( label, steps ) in LOG_SEQUENCES:

        controller.CallBlockingToQt( gui, make_log )

        time.sleep( 0.5 )

        def step( kind, what, ctrl, shift ):

            view = holder[ 'log' ]._list_ctrl

            del pubs[ : ]
            del menus[ : ]

            QW.QApplication.clipboard().setText( 'before' )

            press( view, kind, what, ctrl, shift )

            result = { 'kind' : kind, 'what' : what, 'ctrl' : ctrl, 'shift' : shift }

            result.update( state_of( view ) )

            result[ 'pubs' ] = list( pubs )
            result[ 'menus' ] = list( menus )
            result[ 'clipboard' ] = QW.QApplication.clipboard().text()

            return result


        out = [ controller.CallBlockingToQt( gui, lambda k = k, w = w, c = c, s = s: step( k, w, c, s ) ) for ( k, w, c, s ) in steps ]

        log_sequences.append( { 'name' : label, 'steps' : out } )

        controller.CallBlockingToQt( gui, lambda: holder[ 'log' ].deleteLater() )


    controller.pub = real_pub

    return {
        'files' : [ os.path.basename( p ) for p in paths ],
        'page' : page,
        'sequences' : sequences,
        'no_copy_callable' : no_copy,
        'log' : { 'seeds' : [ { 'url' : url, 'hash' : h } for ( url, h ) in seeds_plan ], 'sequences' : log_sequences }
    }


def main():

    import hydrus_driver
    import record_api

    if len( sys.argv ) > 1 and sys.argv[ 1 ] == '--child':

        Path( sys.argv[ 2 ] ).write_text( json.dumps( hydrus_driver.run_client( record_api.unpack_fixture( 'basic' ), record, port = PORT ) ) )

        return


    with tempfile.TemporaryDirectory() as directory:

        out = os.path.join( directory, 'out.json' )

        hydrus_driver.run_in_subprocess( os.path.abspath( __file__ ), '--child', out )

        result = json.loads( Path( out ).read_text() )


    target = HERE / 'fixtures' / 'list_keys.json'

    target.write_text( json.dumps( result, indent = 1 ) + '\n' )

    print( 'wrote', target )


if __name__ == '__main__':

    main()

#!/usr/bin/env python3
"""Record Qt's extended selection on a reference list (`BetterListCtrl`, the
"review files to import" list of `ReviewLocalFileImports`) for click
sequences that mix ctrl and shift: ctrl-click then shift-click, a shift range
shrunk by a second shift-click, ctrl+shift after a range, a toggled-off row
then shift.

In the running client, on the `basic` fixture, the panel is given six files.
Each sequence starts with a plain click. After every click: the rows
selected (sorted) and the order the list hands them out in
(`selectedRows`, which `GetData( only_selected = True )` follows).

Usage: QT_QPA_PLATFORM=offscreen ~/pyenv/bin/python oracle/record_list_selection.py
       (writes fixtures/list_selection.json)
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

NAMES = [ 'bmp_24.bmp', 'apng_rgba.png', 'gif_static.gif', 'png_rgba.png', 'bmp_8bit.bmp', 'bmp_gray.bmp' ]

# ( label, [ ( row, ctrl, shift ) ... ] )
SEQUENCES = [
    ( 'ctrl then shift', [ ( 0, False, False ), ( 2, True, False ), ( 0, True, False ), ( 1, False, True ) ] ),
    ( 'ctrl-click adds, shift extends from it', [ ( 1, False, False ), ( 3, True, False ), ( 5, False, True ) ] ),
    ( 'second shift shrinks the range', [ ( 0, False, False ), ( 3, False, True ), ( 1, False, True ) ] ),
    ( 'ctrl+shift replaces the range too', [ ( 0, False, False ), ( 3, False, True ), ( 1, True, True ) ] ),
    ( 'ctrl+shift from a ctrl-click', [ ( 2, False, False ), ( 4, True, False ), ( 0, True, True ) ] ),
    ( 'upward range', [ ( 5, False, False ), ( 2, False, True ) ] ),
    ( 'toggled off then shift', [ ( 1, False, False ), ( 3, True, False ), ( 3, True, False ), ( 5, False, True ) ] ),
    ( 'range, ctrl, shift', [ ( 0, False, False ), ( 2, True, False ), ( 4, False, True ), ( 1, True, False ), ( 3, False, True ) ] ),
    ( 'range, ctrl off, shift', [ ( 0, False, False ), ( 3, False, True ), ( 1, True, False ), ( 5, False, True ) ] ),
    ( 'ctrl off then shift back', [ ( 0, False, False ), ( 4, False, True ), ( 2, True, False ), ( 1, False, True ) ] ),
]


def record( session ):

    from qtpy import QtCore as QC
    from qtpy import QtWidgets as QW
    from qtpy import QtTest as QT

    from hydrus.client.gui.panels import ClientGUILocalFileImports

    controller = session.controller
    gui = controller.gui

    folder = tempfile.mkdtemp()

    paths = []

    for name in NAMES:

        path = os.path.join( folder, name )

        shutil.copy( HERE / 'fixtures' / 'media' / name, path )

        paths.append( path )


    holder = {}

    def make():

        panel = ClientGUILocalFileImports.ReviewLocalFileImports( gui, paths = paths )
        panel.resize( 900, 600 )
        panel.show()
        holder[ 'panel' ] = panel


    controller.CallBlockingToQt( gui, make )

    def row_count():

        return holder[ 'panel' ]._paths_list.model().rowCount()


    for _ in range( 100 ):

        if controller.CallBlockingToQt( gui, row_count ) == len( NAMES ):

            break


        time.sleep( 0.1 )


    time.sleep( 0.5 )

    def click( row, ctrl, shift ):

        view = holder[ 'panel' ]._paths_list

        modifiers = QC.Qt.KeyboardModifier.NoModifier

        if ctrl:

            modifiers |= QC.Qt.KeyboardModifier.ControlModifier


        if shift:

            modifiers |= QC.Qt.KeyboardModifier.ShiftModifier


        index = view.model().index( row, 0 )

        QT.QTest.mouseClick( view.viewport(), QC.Qt.MouseButton.LeftButton, modifiers, view.visualRect( index ).center() )

        QW.QApplication.processEvents()

        rows = [ i.row() for i in view.selectionModel().selectedRows() ]

        return { 'row' : row, 'ctrl' : ctrl, 'shift' : shift, 'selected' : sorted( rows ), 'order' : rows }


    sequences = []

    for ( label, clicks ) in SEQUENCES:

        steps = [ controller.CallBlockingToQt( gui, lambda r = r, c = c, s = s: click( r, c, s ) ) for ( r, c, s ) in clicks ]

        sequences.append( { 'name' : label, 'steps' : steps } )


    controller.CallBlockingToQt( gui, lambda: holder[ 'panel' ].deleteLater() )

    return { 'files' : NAMES, 'sequences' : sequences }


def main():

    import hydrus_driver
    import record_api

    if len( sys.argv ) > 1 and sys.argv[ 1 ] == '--child':

        Path( sys.argv[ 2 ] ).write_text( json.dumps( hydrus_driver.run_client( record_api.unpack_fixture( 'basic' ), record ) ) )

        return


    with tempfile.TemporaryDirectory() as directory:

        out = os.path.join( directory, 'out.json' )

        hydrus_driver.run_in_subprocess( os.path.abspath( __file__ ), '--child', out )

        result = json.loads( Path( out ).read_text() )


    target = HERE / 'fixtures' / 'list_selection.json'

    target.write_text( json.dumps( result, indent = 1 ) + '\n' )

    print( 'wrote', target )


if __name__ == '__main__':

    main()

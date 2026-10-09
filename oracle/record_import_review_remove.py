#!/usr/bin/env python3
"""Record removing paths from the reference's "review files to import" list
(`ReviewLocalFileImports.RemovePaths`).

In the running client, on the `basic` fixture, the panel is given four
files (copies of `fixtures/media` files in a folder of their own) and left
to parse them. The second row is selected, then:

* the Delete key pressed on the list, the question answered "no";
* the "remove files" button clicked, the question answered "yes".

After each step: the questions asked (`GetYesNo`), and the list's rows
(the index shown and the file's name), in order.

Usage: QT_QPA_PLATFORM=offscreen ~/pyenv/bin/python oracle/record_import_review_remove.py
       (writes fixtures/import_review_remove.json)
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

NAMES = [ 'bmp_24.bmp', 'apng_rgba.png', 'gif_static.gif', 'png_rgba.png' ]


def record( session ):

    from qtpy import QtCore as QC
    from qtpy import QtWidgets as QW
    from qtpy import QtTest as QT

    from hydrus.client.gui import ClientGUIDialogsQuick
    from hydrus.client.gui.panels import ClientGUILocalFileImports

    controller = session.controller
    gui = controller.gui

    folder = tempfile.mkdtemp()

    paths = []

    for name in NAMES:

        path = os.path.join( folder, name )

        shutil.copy( HERE / 'fixtures' / 'media' / name, path )

        paths.append( path )


    asked = []

    answer = { 'yes' : False }

    def get_yes_no( parent, message, *args, **kwargs ):

        asked.append( message )

        return QW.QDialog.DialogCode.Accepted if answer[ 'yes' ] else QW.QDialog.DialogCode.Rejected


    ClientGUIDialogsQuick.GetYesNo = get_yes_no

    holder = {}

    def make():

        holder[ 'panel' ] = ClientGUILocalFileImports.ReviewLocalFileImports( gui, paths = paths )


    controller.CallBlockingToQt( gui, make )

    def rows():

        view = holder[ 'panel' ]._paths_list

        model = view.model()

        out = []

        for r in range( model.rowCount() ):

            index = model.data( model.index( r, 0 ) )
            path = model.data( model.index( r, 1 ) )

            out.append( [ index, os.path.basename( path ) ] )


        return out


    for _ in range( 100 ):

        if len( controller.CallBlockingToQt( gui, rows ) ) == len( NAMES ):

            break


        time.sleep( 0.1 )


    time.sleep( 0.5 )

    steps = [ { 'rows' : controller.CallBlockingToQt( gui, rows ) } ]

    def select_second():

        view = holder[ 'panel' ]._paths_list

        model = view.model()

        view.selectionModel().select( model.index( 1, 0 ), QC.QItemSelectionModel.SelectionFlag.ClearAndSelect | QC.QItemSelectionModel.SelectionFlag.Rows )

        return rows()[ 1 ]


    selected = controller.CallBlockingToQt( gui, select_second )

    def delete_key():

        answer[ 'yes' ] = False

        view = holder[ 'panel' ]._paths_list

        QT.QTest.keyClick( view, QC.Qt.Key.Key_Delete )

        QW.QApplication.processEvents()


    def remove_button():

        answer[ 'yes' ] = True

        buttons = [ b for b in holder[ 'panel' ].findChildren( QW.QPushButton ) if b.text() == 'remove files' ]

        buttons[ 0 ].click()

        QW.QApplication.processEvents()


    for ( name, action, yes ) in [ ( 'delete key', delete_key, False ), ( 'remove files', remove_button, True ) ]:

        del asked[:]

        controller.CallBlockingToQt( gui, action )

        time.sleep( 0.3 )

        steps.append( { 'do' : name, 'answer' : yes, 'asked' : list( asked ), 'rows' : controller.CallBlockingToQt( gui, rows ) } )


    controller.CallBlockingToQt( gui, lambda: holder[ 'panel' ].deleteLater() )

    return { 'files' : NAMES, 'selected' : selected, 'steps' : steps }


def main():

    import hydrus_driver
    import record_api

    if len( sys.argv ) > 1 and sys.argv[ 1 ] == '--child':

        Path( sys.argv[ 2 ] ).write_text( json.dumps( hydrus_driver.run_client( record_api.unpack_fixture( 'basic' ), record ) ) )

        return


    with tempfile.TemporaryDirectory() as directory:

        path = Path( directory ) / 'result.json'

        hydrus_driver.run_in_subprocess( str( Path( __file__ ).resolve() ), '--child', str( path ) )

        result = json.loads( path.read_text() )


    ( HERE / 'fixtures' / 'import_review_remove.json' ).write_text( json.dumps( result, indent = 2 ) + '\n' )

    print( 'wrote import_review_remove.json' )


if __name__ == '__main__':

    main()

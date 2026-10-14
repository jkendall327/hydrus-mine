#!/usr/bin/env python3
"""Record the reference's manage favourite searches list selecting rows.

`EditFavouriteSearchesPanel` is built on four favourite searches (two in no
folder, two in the folder "art"; the rows of the hydrus-rs test) and driven with
real mouse clicks (`QTest`) on its tree view and header: a plain click, ctrl
and shift clicks, a click on the name header (the list sorts by it), a range
chosen after the sort, and the delete button (its question is answered yes).
After each step the selected rows' names (in the order shown), the order shown
and whether the edit and delete buttons are enabled are recorded.

Usage: QT_QPA_PLATFORM=offscreen python oracle/record_favourite_list_selection.py
       (writes fixtures/favourite_list_selection.json)
"""

import json
import os
import sys

HERE = os.path.dirname( os.path.abspath( __file__ ) )

sys.path.insert( 0, HERE )

OUT = os.path.join( HERE, 'fixtures', 'favourite_list_selection.json' )

ROWS = [ ( None, 'delta' ), ( None, 'alpha' ), ( 'art', 'charlie' ), ( 'art', 'bravo' ) ]

# ( what, row ): a click is on a displayed row; 'header' is the name column
STEPS = [
    [ 'initial' ],
    [ 'click', 1 ],
    [ 'ctrl', 3 ],
    [ 'ctrl', 3 ],
    [ 'click', 0 ],
    [ 'shift', 2 ],
    [ 'header', 1 ],
    [ 'click', 0 ],
    [ 'shift', 1 ],
    [ 'ctrl', 3 ],
    [ 'shift', 0 ],
    [ 'delete' ],
]


def record( session ):

    from qtpy import QtCore as QC
    from qtpy import QtTest
    from qtpy import QtWidgets as QW

    from hydrus.core import HydrusConstants as HC
    from hydrus.client import ClientConstants as CC
    from hydrus.client.gui import ClientGUIDialogsQuick as Q
    from hydrus.client.gui import ClientGUITopLevelWindowsPanels
    from hydrus.client.gui.search import ClientGUISearchPanels
    from hydrus.client.search import ClientSearchFileSearchContext

    controller = session.controller
    gui = controller.gui

    def drive():

        context = ClientSearchFileSearchContext.FileSearchContext( location_context = controller.new_options.GetDefaultLocalLocationContext() )

        rows = [ ( folder, name, context.Duplicate(), True, None, None ) for ( folder, name ) in ROWS ]

        dlg = ClientGUITopLevelWindowsPanels.DialogEdit( gui, 'edit favourite searches' )

        panel = ClientGUISearchPanels.EditFavouriteSearchesPanel( dlg, rows )

        dlg.SetPanel( panel )

        dlg.resize( 900, 500 )
        dlg.show()

        QW.QApplication.processEvents()

        view = panel._favourite_searches

        model = view.model()

        questions = []

        def yes( parent, message, *args, **kwargs ):

            questions.append( message )

            return QW.QDialog.DialogCode.Accepted


        Q.GetYesNo = yes

        def names():

            return [ model.data( model.index( r, 1 ), QC.Qt.ItemDataRole.DisplayRole ) for r in range( model.rowCount() ) ]


        def selected():

            indexes = view.selectionModel().selectedRows()

            return [ model.data( model.index( i.row(), 1 ), QC.Qt.ItemDataRole.DisplayRole ) for i in sorted( indexes, key = lambda i: i.row() ) ]


        def button( text ):

            return next( b for b in panel._favourite_searches_panel.findChildren( QW.QPushButton ) if b.text() == text )


        def state( step ):

            return {
                'step' : step,
                'shown' : names(),
                'selected' : selected(),
                'edit_enabled' : button( 'edit' ).isEnabled(),
                'delete_enabled' : button( 'delete' ).isEnabled()
            }


        def click( row, modifier ):

            index = model.index( row, 1 )

            QW.QApplication.processEvents()

            pos = view.visualRect( index ).center()

            QtTest.QTest.mouseClick( view.viewport(), QC.Qt.MouseButton.LeftButton, modifier, pos )

            QW.QApplication.processEvents()


        def header( column ):

            h = view.header()

            x = h.sectionViewportPosition( column ) + 5

            QtTest.QTest.mouseClick( h.viewport(), QC.Qt.MouseButton.LeftButton, QC.Qt.KeyboardModifier.NoModifier, QC.QPoint( x, h.height() // 2 ) )

            QW.QApplication.processEvents()


        out = []

        for step in STEPS:

            kind = step[ 0 ]

            if kind == 'click':

                click( step[ 1 ], QC.Qt.KeyboardModifier.NoModifier )

            elif kind == 'ctrl':

                click( step[ 1 ], QC.Qt.KeyboardModifier.ControlModifier )

            elif kind == 'shift':

                click( step[ 1 ], QC.Qt.KeyboardModifier.ShiftModifier )

            elif kind == 'header':

                header( step[ 1 ] )

            elif kind == 'delete':

                button( 'delete' ).click()

                QW.QApplication.processEvents()


            out.append( state( step ) )


        dlg.reject()

        return { 'rows' : ROWS, 'steps' : out, 'questions' : questions }


    return controller.CallBlockingToQt( gui, drive )


def main():

    import hydrus_driver
    import record_api

    db_dir = record_api.unpack_fixture( 'basic' )

    result = hydrus_driver.run_client( db_dir, record )

    with open( OUT, 'w' ) as f:

        json.dump( result, f, indent = 1, sort_keys = True, ensure_ascii = False )
        f.write( '\n' )


    print( f'wrote {OUT}' )


if __name__ == '__main__':

    main()

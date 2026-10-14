#!/usr/bin/env python3
"""Record the reference's favourite search editor taking typed predicates.

`EditFavouriteSearchPanel`, on a search holding the tag "blue eyes" (as the
page a search is saved from does), has text typed into its autocomplete box and
the Return key pressed (real key events, `QTest`), and its list of search
predicates double-clicked to remove one. After each step the predicates the
editor would save (`GetValue`, as their text) and any message the reference
showed are recorded.

Usage: QT_QPA_PLATFORM=offscreen python oracle/record_favourite_predicates.py
       (writes fixtures/favourite_predicates.json)
"""

import json
import os
import sys

HERE = os.path.dirname( os.path.abspath( __file__ ) )

sys.path.insert( 0, HERE )

OUT = os.path.join( HERE, 'fixtures', 'favourite_predicates.json' )

STEPS = [
    [ 'initial' ],
    [ 'type', 'series:metroid' ],
    [ 'type', 'series:metroid' ],
    [ 'type', 'Blue Eyes' ],
    [ 'type', '-creator:samus aran' ],
    [ 'type', 'zz*' ],
    [ 'type', 'character:*' ],
    [ 'type', '  spaced   tag  ' ],
    [ 'type', 'system:limit = 64' ],
    [ 'type', 'system:everything' ],
    [ 'double click', 0 ],
    [ 'double click', 2 ],
    [ 'type', 'blue eyes' ],
    [ 'double click', 0 ],
    [ 'type', 'system:nonsense words' ],
    [ 'type', 'system:' ],
    [ 'type', 'sam*' ],
    [ 'type', 'sam' ],
]


def record( session ):

    from qtpy import QtCore as QC
    from qtpy import QtTest
    from qtpy import QtWidgets as QW

    from hydrus.core import HydrusConstants as HC
    from hydrus.client import ClientConstants as CC
    from hydrus.client import ClientLocation
    from hydrus.client.gui import ClientGUIDialogsMessage
    from hydrus.client.gui import ClientGUIDialogsQuick
    from hydrus.client.gui.search import ClientGUISearchPanels
    from hydrus.client.search import ClientSearchFileSearchContext
    from hydrus.client.search import ClientSearchPredicate

    controller = session.controller
    gui = controller.gui

    def drive():

        controller.new_options.SetBoolean( 'advanced_mode', True )

        search = ClientSearchFileSearchContext.FileSearchContext(
            location_context = ClientLocation.LocationContext.STATICCreateSimple( CC.LOCAL_FILE_SERVICE_KEY ),
            predicates = [ ClientSearchPredicate.Predicate( ClientSearchPredicate.PREDICATE_TYPE_TAG, 'blue eyes' ) ]
        )

        dlg = QW.QDialog( gui )

        panel = ClientGUISearchPanels.EditFavouriteSearchPanel( dlg, {}, None, 'recorded', search, True, None, None )

        layout = QW.QVBoxLayout( dlg )
        layout.addWidget( panel )

        dlg.resize( 1000, 700 )
        dlg.show()

        QW.QApplication.processEvents()

        messages = []

        ClientGUIDialogsMessage.ShowInformation = lambda win, message: messages.append( [ 'information', message ] )
        ClientGUIDialogsMessage.ShowWarning = lambda win, message: messages.append( [ 'warning', message ] )
        ClientGUIDialogsMessage.ShowCritical = lambda win, title, message: messages.append( [ 'critical', title, message ] )

        ac = panel._tag_autocomplete

        listbox = ac._predicates_listbox

        def predicates():

            return sorted( p.ToString( with_count = False ) for p in panel.GetValue()[ 2 ].GetPredicates() )


        def shown():

            # the list's rows in the order it draws them
            return [ term.GetPredicate().ToString( with_count = False ) for term in listbox._ordered_terms ]


        out = []

        for step in STEPS:

            del messages[:]

            suggestions = None
            before_return = None
            in_step = None

            if step[ 0 ] == 'type':

                ac._text_ctrl.setText( step[ 1 ] )

                # wait until the suggestions the text finds are listed, as they
                # are for a user who pauses before pressing Return
                for _ in range( 200 ):

                    QtTest.QTest.qWait( 50 )

                    loading = any( 'loading' in term.GetPredicate().ToString( with_count = False ) for term in ac._search_results_list._ordered_terms )

                    if ac._GetParsedAutocompleteText() == ac._current_list_parsed_autocomplete_text and not loading:

                        break


                else:

                    raise Exception( 'the result list never caught up with ' + repr( step[ 1 ] ) )


                QW.QApplication.processEvents()

                results = ac._search_results_list

                suggestions = [ term.GetPredicate().ToString( with_count = False ) for term in results._ordered_terms ]

                before_return = [ term.GetPredicate().ToString( with_count = False ) for term in results._selected_terms ]

                in_step = ac._GetParsedAutocompleteText() == ac._current_list_parsed_autocomplete_text

                QtTest.QTest.keyClick( ac._text_ctrl, QC.Qt.Key.Key_Return )

                QW.QApplication.processEvents()

                QtTest.QTest.qWait( 50 )

            elif step[ 0 ] == 'double click':

                QW.QApplication.processEvents()

                row = step[ 1 ]

                # the listbox draws its rows itself, one text height each
                text_height = listbox.fontMetrics().height()

                pos = QC.QPoint( 20, row * text_height + text_height // 2 )

                QtTest.QTest.mouseClick( listbox.widget(), QC.Qt.MouseButton.LeftButton, QC.Qt.KeyboardModifier.NoModifier, pos )

                QtTest.QTest.mouseDClick( listbox.widget(), QC.Qt.MouseButton.LeftButton, QC.Qt.KeyboardModifier.NoModifier, pos )

                QW.QApplication.processEvents()

                QtTest.QTest.qWait( 50 )


            out.append( { 'step' : step, 'predicates' : predicates(), 'shown' : shown(), 'messages' : list( messages ), 'typed_left' : ac._text_ctrl.text(), 'suggestions' : suggestions, 'highlighted' : before_return, 'list_in_step' : in_step } )


        dlg.reject()

        return { 'steps' : out }


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

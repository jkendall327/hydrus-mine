#!/usr/bin/env python3
"""Record the auto-resolution "test A or B using search terms" comparator editor.

`EditPairComparatorOneFileMetadataConditionalPanel`, the reference's real
panel, on a new comparator: its "A/B/either will match these" choice and its
embedded metadata-conditional search (`AutoCompleteDropdownMetadataConditional`,
a read autocomplete that lists only what can be tested on one file). Text is
typed into its box and Return or Shift+Return pressed (real key events, after
the result list has caught up with the text); rows of its active list are
double-clicked; its paste button is given clipboard lines and its empty-search
button pressed. After each step the list as drawn, its highlight, the active
predicates as drawn, the OR being built and the text left are recorded, and
the comparator the panel would give back (its summary and predicates).

Usage: QT_QPA_PLATFORM=offscreen python oracle/record_metadata_conditional_editor.py
       (writes fixtures/metadata_conditional_editor.json)
"""

import json
import os
import sys

HERE = os.path.dirname( os.path.abspath( __file__ ) )

sys.path.insert( 0, HERE )

OUT = os.path.join( HERE, 'fixtures', 'metadata_conditional_editor.json' )

STEPS = [
    [ 'initial' ],
    [ 'type', 'system:inbox' ],
    [ 'type', 'system:width > 100' ],
    [ 'type', 'system:filesize < 5MB' ],
    [ 'type', 'system:limit = 5' ],
    [ 'type', 'system:filetype is image' ],
    [ 'type', 'blue' ],
    [ 'type', 'series:metroid' ],
    [ 'type', 'sam*' ],
    [ 'type', 'series:*' ],
    [ 'type', '-character:samus aran' ],
    [ 'type', 'nothing here at all' ],
    [ 'shift type', 'system:archive' ],
    [ 'shift type', 'system:has audio' ],
    [ 'type', 'system:height < 50' ],
    [ 'double click', 0 ],
    [ 'double click', 0 ],
    [ 'looking at', 1 ],
    [ 'paste', 'system:inbox\nsystem:width = 50\nnonsense:here\nsystem:not a thing' ],
    [ 'looking at', 2 ],
    [ 'empty search' ],
    [ 'type', '' ],
    [ 'activate row', 0, None ],
    [ 'activate row', 1, None ],
    [ 'shift activate row', 2, 0 ],
    [ 'shift activate row', 6, 0 ],
    [ 'activate row', 0, None ],
    [ 'activate row', 4, 'cancel' ],
    [ 'activate row', 6, 'system:untagged' ],
    [ 'shift activate row', 0, None ],
    [ 'type', 'blue' ],
    [ 'shift type', 'system:width > 100' ],
    [ 'shift type', 'series:metroid' ],
    [ 'type', 'system:has audio' ],
    [ 'type', 'system:limit = 5' ],
    [ 'escape' ],
    [ 'shift activate row', 1, None ],
    [ 'type', 'system:height < 50' ],
    [ 'or button', None ],
    [ 'or button', [ 'blue', 'series:metroid' ] ],
    [ 'shift activate row', 1, None ],
    [ 'or button', [ 'system:width > 100', 'sam*' ] ],
    [ 'looking at', 0 ]
]


def record( session ):

    from qtpy import QtCore as QC
    from qtpy import QtTest
    from qtpy import QtWidgets as QW

    from hydrus.client.duplicates import ClientDuplicatesAutoResolutionComparators as C
    from hydrus.client.gui import ClientGUIDialogsMessage
    from hydrus.client.gui.duplicates import ClientGUIDuplicatesAutoResolution as R
    from hydrus.client.gui import ClientGUITopLevelWindowsPanels
    from hydrus.client.gui.search import ClientGUISearch

    controller = session.controller
    gui = controller.gui

    def drive():

        dlg = QW.QDialog( gui )

        panel = R.EditPairComparatorOneFileMetadataConditionalPanel( dlg, C.PairComparatorOneFileMetadataConditional() )

        layout = QW.QVBoxLayout( dlg )
        layout.addWidget( panel )

        dlg.resize( 900, 700 )
        dlg.show()

        QW.QApplication.processEvents()

        messages = []

        ClientGUIDialogsMessage.ShowInformation = lambda win, message: messages.append( [ 'information', message ] )
        ClientGUIDialogsMessage.ShowWarning = lambda win, message: messages.append( [ 'warning', message ] )
        ClientGUIDialogsMessage.ShowCritical = lambda win, title, message: messages.append( [ 'critical', title, message ] )

        clipboard = [ '' ]

        controller.GetClipboardText = lambda: clipboard[ 0 ]

        ac = panel._metadata_conditional._ac_panel

        listbox = ac._predicates_listbox

        def rows_of( box ):

            return [ [ ''.join( t for ( t, _ ) in row ) for row in box._GetRowsOfTextsAndColours( term ) ] for term in box._ordered_terms ]


        def state():

            box = ac._dropdown_notebook.currentWidget()

            construction = ac._under_construction_or_predicate

            comparator = panel.GetValue()

            return {
                'looking_at_choices' : [ panel._looking_at.itemText( i ) for i in range( panel._looking_at.count() ) ],
                'looking_at' : panel._looking_at.currentIndex(),
                'tabs' : [ ac._dropdown_notebook.tabText( i ) for i in range( ac._dropdown_notebook.count() ) ],
                'tab' : ac._dropdown_notebook.currentIndex(),
                'rows_drawn' : rows_of( box ),
                'highlighted' : sorted( box._terms_to_logical_indices[ t ] for t in box._selected_terms ),
                'shown' : [ term.GetPredicate().ToString() for term in listbox._ordered_terms ],
                'construction' : None if construction is None else sorted( p.ToString( with_count = False ) for p in construction.GetValue() ),
                'cancel_shown' : ac._or_cancel.isVisibleTo( ac._dropdown_window ),
                'rewind_shown' : ac._or_rewind.isVisibleTo( ac._dropdown_window ),
                'typed_left' : ac._text_ctrl.text(),
                'summary' : comparator.GetSummary(),
                'saved' : sorted( p.ToString() for p in comparator.GetMetadataConditional().GetFileSearchContext().GetPredicates() )
            }


        def key( widget, k, modifier = QC.Qt.KeyboardModifier.NoModifier ):

            QtTest.QTest.keyClick( widget, k, modifier )

            QW.QApplication.processEvents()

            QtTest.QTest.qWait( 50 )


        def settle( what, ac = ac ):

            # wait until the list has caught up with the text and is loaded
            for _ in range( 1200 ):

                QtTest.QTest.qWait( 50 )

                loading = any( 'loading' in term.GetPredicate().ToString( with_count = False ) for term in ac._search_results_list._ordered_terms )

                if ac._GetParsedAutocompleteText() == ac._current_list_parsed_autocomplete_text and not loading:

                    return


                # (the reference sometimes never delivers a fetch's full
                # results, leaving "loading full results": type it again)
                if _ % 400 == 399:

                    typed = ac._text_ctrl.text()

                    ac._text_ctrl.setText( '' )

                    QtTest.QTest.qWait( 50 )

                    ac._text_ctrl.setText( typed )


            raise Exception( 'the result list never caught up with ' + what + repr( ( str( ac._GetParsedAutocompleteText() ), str( ac._current_list_parsed_autocomplete_text ), [ t.GetPredicate().ToString() for t in ac._search_results_list._ordered_terms ], ac._current_fetch_job_status ) ) )


        out = []

        for step in STEPS:

            del messages[:]

            what = step[ 0 ]

            listed = None

            if what in ( 'type', 'shift type' ):

                ac._text_ctrl.setText( step[ 1 ] )

                settle( 'the text ' + repr( step[ 1 ] ) )

                QW.QApplication.processEvents()

                # (the list as it is when Return is pressed)
                listed = { 'rows_drawn' : rows_of( ac._search_results_list ), 'highlighted' : sorted( ac._search_results_list._terms_to_logical_indices[ t ] for t in ac._search_results_list._selected_terms ) }

                if step[ 1 ] != '':

                    modifier = QC.Qt.KeyboardModifier.ShiftModifier if what == 'shift type' else QC.Qt.KeyboardModifier.NoModifier

                    key( ac._text_ctrl, QC.Qt.Key.Key_Return, modifier )


            elif what == 'double click':

                QW.QApplication.processEvents()

                row = step[ 1 ]

                text_height = listbox.fontMetrics().height()

                pos = QC.QPoint( 20, row * text_height + text_height // 2 )

                QtTest.QTest.mouseClick( listbox.widget(), QC.Qt.MouseButton.LeftButton, QC.Qt.KeyboardModifier.NoModifier, pos )

                QtTest.QTest.mouseDClick( listbox.widget(), QC.Qt.MouseButton.LeftButton, QC.Qt.KeyboardModifier.NoModifier, pos )

                QW.QApplication.processEvents()

                QtTest.QTest.qWait( 50 )

            elif what in ( 'activate row', 'shift activate row' ):

                # (a row that needs a value opens its editor, the real
                # FleshOutPredicatePanel; it is cancelled, or the ok button of
                # one of its panels is pressed, its value as it opens)
                editors = []

                class Dialog( QW.QDialog ):

                    def __init__( self, parent, title, hide_buttons = False ):

                        super().__init__( parent )

                        self._title = title

                    def __enter__( self ):

                        return self

                    def __exit__( self, *args ):

                        self.deleteLater()

                        return False

                    def SetPanel( self, panel ):

                        self._panel = panel

                        self._accepted = False

                        def accept():

                            self._accepted = True


                        panel.okSignal.connect( accept )

                    def exec( self ):

                        oks = [ b for b in self._panel.findChildren( QW.QPushButton ) if b.text() == 'ok' ]

                        editors.append( { 'title' : self._title, 'ok_buttons' : len( oks ) } )

                        statics = [ b.text() for b in self._panel.findChildren( QW.QPushButton ) if b.text().startswith( 'system:' ) ]

                        editors[ -1 ][ 'static_buttons' ] = statics

                        if step[ 2 ] == 'cancel':

                            return QW.QDialog.DialogCode.Rejected

                        elif isinstance( step[ 2 ], int ):

                            oks[ step[ 2 ] ].click()

                        else:

                            # (one of its fixed predicates' buttons)
                            [ b for b in self._panel.findChildren( QW.QPushButton ) if b.text() == step[ 2 ] ][ 0 ].click()


                        QW.QApplication.processEvents()

                        return QW.QDialog.DialogCode.Accepted if self._accepted else QW.QDialog.DialogCode.Rejected



                real_dialog = ClientGUITopLevelWindowsPanels.DialogEdit

                ClientGUITopLevelWindowsPanels.DialogEdit = Dialog

                box = ac._dropdown_notebook.currentWidget()

                activated = ''.join( t for ( t, _ ) in box._GetRowsOfTextsAndColours( box._ordered_terms[ step[ 1 ] ] )[ 0 ] )

                box._Hit( False, False, step[ 1 ] )

                box._Activate( False, what == 'shift activate row' )

                ClientGUITopLevelWindowsPanels.DialogEdit = real_dialog

                listed = { 'activated' : activated, 'editors' : editors }

            elif what == 'or button':

                # (the OR editor it opens, the real EditPredicatesPanel, is
                # cancelled, or has text typed into its own search and Return
                # pressed for each, then OK)
                inner = {}

                class Dialog( QW.QDialog ):

                    def __init__( self, parent, title, hide_buttons = False ):

                        super().__init__( parent )

                        inner[ 'title' ] = title

                    def __enter__( self ):

                        return self

                    def __exit__( self, *args ):

                        self.deleteLater()

                        return False

                    def SetPanel( self, panel ):

                        self._panel = panel

                    def exec( self ):

                        if step[ 1 ] is None:

                            return QW.QDialog.DialogCode.Rejected


                        [ control ] = self._panel._editable_pred_panels

                        inner_ac = control._search_control

                        inner[ 'kind' ] = type( inner_ac ).__name__

                        inner[ 'typed' ] = []

                        for text in step[ 1 ]:

                            inner_ac._text_ctrl.setText( text )

                            settle( 'the OR editor ' + repr( text ), inner_ac )

                            listed_rows = rows_of( inner_ac._search_results_list )

                            key( inner_ac._text_ctrl, QC.Qt.Key.Key_Return )

                            settle( 'the OR editor after ' + repr( text ), inner_ac )

                            inner[ 'typed' ].append( { 'text' : text, 'rows_drawn' : listed_rows, 'shown' : [ term.GetPredicate().ToString() for term in inner_ac._predicates_listbox._ordered_terms ] } )


                        inner[ 'value' ] = [ p.ToString() for p in self._panel.GetValue() ]

                        return QW.QDialog.DialogCode.Accepted



                real_dialog = ClientGUITopLevelWindowsPanels.DialogEdit

                ClientGUITopLevelWindowsPanels.DialogEdit = Dialog

                ac._or_basic.click()

                ClientGUITopLevelWindowsPanels.DialogEdit = real_dialog

                listed = inner

            elif what == 'escape':

                key( ac._text_ctrl, QC.Qt.Key.Key_Escape )

            elif what == 'looking at':

                panel._looking_at.setCurrentIndex( step[ 1 ] )

            elif what == 'paste':

                clipboard[ 0 ] = step[ 1 ]

                ac._paste_button.click()

            elif what == 'empty search':

                ac._empty_search_button.click()


            QW.QApplication.processEvents()

            settle( repr( step ) )

            out.append( { 'step' : step, 'messages' : list( messages ), 'listed' : listed, 'state' : state() } )


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

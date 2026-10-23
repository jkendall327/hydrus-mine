#!/usr/bin/env python3
"""Record the favourite search editor's OR construction and tabs.

`EditFavouriteSearchPanel` on a search holding "blue eyes", given Shift+Return
and Return on typed text (real key events), the OR construction's rewind and
cancel buttons, and the favourites and children tabs (with synthetic tags,
parents and favourite tags, as `record_read_tag_tabs.py` makes). After each step
the predicates the editor would save, the list as drawn, the construction (the
OR predicate being built as text, or None), whether its cancel and rewind
buttons are shown, the current tab and its rows are recorded.

Usage: QT_QPA_PLATFORM=offscreen python oracle/record_favourite_editor_tabs_or.py
       (writes fixtures/favourite_editor_tabs_or.json)
"""

import json
import os
import sys

HERE = os.path.dirname( os.path.abspath( __file__ ) )

sys.path.insert( 0, HERE )

OUT = os.path.join( HERE, 'fixtures', 'favourite_editor_rows.json' )

FAVOURITES = [ 'parity:tabs root', 'parity:tabs alpha old', 'parity:tabs unseen' ]

CORPUS = [ ( 'parity:tabs alpha', 3 ), ( 'parity:tabs beta', 1 ), ( 'parity:tabs alpha old', 1 ) ]
PARENTS = [ [ 'parity:tabs alpha', 'parity:tabs root' ], [ 'parity:tabs beta', 'parity:tabs root' ], [ 'parity:tabs zero', 'parity:tabs root' ], [ 'parity:tabs deep', 'parity:tabs alpha' ] ]
SIBLINGS = [ [ 'parity:tabs alpha old', 'parity:tabs alpha' ] ]

STEPS = [
    [ 'initial' ],
    [ 'text', 'parity:tabs' ],
    [ 'text', 'parity:tabs alpha old' ],
    [ 'text', 'parity:tabs a*' ],
    [ 'text', 'blue' ],
    [ 'text', 'system:limit = 5' ],
    [ 'text', '' ],
    [ 'tab', 'favourites' ],
    [ 'key', 'down' ],
    [ 'key', 'return' ],
    [ 'tab', 'favourites' ],
    [ 'key', 'end' ],
    [ 'key', 'up' ],
    [ 'key', 'shift return' ],
    [ 'key', 'home' ],
    [ 'key', 'up' ],
    [ 'key', 'down' ],
    [ 'key', 'shift return' ],
    [ 'key', 'escape' ],
    [ 'tab', 'children' ],
    [ 'key', 'down' ],
    [ 'key', 'return' ],
    [ 'tab', 'children' ],
    [ 'key', 'page down' ],
    [ 'key', 'shift return' ],
    [ 'key', 'return' ],
    [ 'key', 'shift return' ],
    [ 'advanced', None ],
    [ 'advanced', 'parity:tabs alpha || parity:tabs beta' ],
    [ 'advanced', '( series:metroid && parity:tabs beta ) || -character:samus aran' ]
]


def record( session ):

    from qtpy import QtCore as QC
    from qtpy import QtTest
    from qtpy import QtWidgets as QW

    from hydrus.client import ClientConstants as CC
    from hydrus.client import ClientLocation
    from hydrus.client.gui import ClientGUIAsync
    from hydrus.client.gui import ClientGUIDialogsMessage
    from hydrus.client.gui.search import ClientGUIACDropdown as A
    from hydrus.client.gui.search import ClientGUISearch
    from hydrus.client.gui.search import ClientGUISearchPanels
    from hydrus.core import HydrusExceptions
    from hydrus.client.metadata import ClientContentUpdates as U
    from hydrus.client.search import ClientSearchFileSearchContext
    from hydrus.client.search import ClientSearchPredicate
    from hydrus.core import HydrusConstants as HC

    controller = session.controller
    gui = controller.gui

    service = next( s.GetServiceKey() for s in controller.services_manager.GetServices( ( HC.LOCAL_TAG, ) ) if s.GetName() == 'my tags' )

    manifest = json.load( open( os.path.join( HERE, 'fixtures', 'legacy_db', 'basic.manifest.json' ) ) )

    hashes = [ bytes.fromhex( f[ 'hash' ] ) for f in manifest[ 'files' ][ :4 ] ]

    updates = [ U.ContentUpdate( HC.CONTENT_TYPE_MAPPINGS, HC.CONTENT_UPDATE_ADD, ( tag, set( hashes[ :n ] ) ) ) for ( tag, n ) in CORPUS ]

    updates.extend( U.ContentUpdate( HC.CONTENT_TYPE_TAG_SIBLINGS, HC.CONTENT_UPDATE_ADD, tuple( pair ) ) for pair in SIBLINGS )
    updates.extend( U.ContentUpdate( HC.CONTENT_TYPE_TAG_PARENTS, HC.CONTENT_UPDATE_ADD, tuple( pair ) ) for pair in PARENTS )

    controller.WriteSynchronous( 'content_updates', U.ContentUpdatePackage.STATICCreateFromContentUpdates( service, updates ) )

    for _ in range( 12 ):

        if not controller.WriteSynchronous( 'sync_tag_display_maintenance', service, .5 ):

            break


    controller.new_options.SetStringList( 'favourite_tags', FAVOURITES )

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

        dlg.resize( 1000, 800 )
        dlg.show()

        QW.QApplication.processEvents()

        messages = []

        ClientGUIDialogsMessage.ShowInformation = lambda win, message: messages.append( [ 'information', message ] )
        ClientGUIDialogsMessage.ShowWarning = lambda win, message: messages.append( [ 'warning', message ] )
        ClientGUIDialogsMessage.ShowCritical = lambda win, title, message: messages.append( [ 'critical', title, message ] )

        # children are fetched in a thread; run it where we stand
        ClientGUIAsync.AsyncQtJob.start = lambda job: job._publish_callable( job._work_callable() )

        ac = panel._tag_autocomplete

        ac.RefreshFavouriteTags()

        tabs = { 'results' : ac._search_results_list, 'favourites' : ac._favourites_list, 'children' : ac._children_list }

        def rows_of( box ):

            # (each term's rows: one, or more where parents are drawn under it)
            return [ [ ''.join( t for ( t, _ ) in row ) for row in box._GetRowsOfTextsAndColours( term ) ] for term in box._ordered_terms ]


        def state():

            box = ac._dropdown_notebook.currentWidget()

            construction = ac._under_construction_or_predicate

            return {
                'predicates' : sorted( p.ToString( with_count = False ) for p in panel.GetValue()[ 2 ].GetPredicates() ),
                'shown' : [ term.GetPredicate().ToString( with_count = False ) for term in ac._predicates_listbox._ordered_terms ],
                'construction' : None if construction is None else sorted( p.ToString( with_count = False ) for p in construction.GetValue() ),
                'cancel_shown' : ac._or_cancel.isVisibleTo( ac._dropdown_window ),
                'rewind_shown' : ac._or_rewind.isVisibleTo( ac._dropdown_window ),
                'tab' : ac._dropdown_notebook.currentIndex(),
                'tab_text' : ac._dropdown_notebook.tabText( ac._dropdown_notebook.currentIndex() ),
                'rows' : [ term.GetPredicate().GetValue() if term.GetPredicate().GetType() == ClientSearchPredicate.PREDICATE_TYPE_TAG else term.GetPredicate().ToString( with_count = False ) for term in box._ordered_terms ],
                'typed_left' : ac._text_ctrl.text(),
                'highlighted' : sorted( box._terms_to_logical_indices[ t ] for t in box._selected_terms ),
                'rows_drawn' : rows_of( box )
            }


        def key( widget, k, modifier = QC.Qt.KeyboardModifier.NoModifier ):

            QtTest.QTest.keyClick( widget, k, modifier )

            QW.QApplication.processEvents()

            QtTest.QTest.qWait( 50 )


        def settle( what ):

            # wait until the list has caught up with the text and is loaded
            for _ in range( 200 ):

                QtTest.QTest.qWait( 50 )

                loading = any( 'loading' in term.GetPredicate().ToString( with_count = False ) for term in ac._search_results_list._ordered_terms )

                if ac._GetParsedAutocompleteText() == ac._current_list_parsed_autocomplete_text and not loading:

                    return


            raise Exception( 'the result list never caught up with ' + what )


        dialogs = []
        previews = []

        out = []

        for step in STEPS:

            del messages[:]

            what = step[ 0 ]

            if what == 'text':

                # (typed, with no Return pressed)
                ac._dropdown_notebook.setCurrentWidget( ac._search_results_list )

                ac._text_ctrl.setText( step[ 1 ] )

            elif what == 'key':

                ( name, modifier ) = { 'down' : ( QC.Qt.Key.Key_Down, None ), 'up' : ( QC.Qt.Key.Key_Up, None ), 'end' : ( QC.Qt.Key.Key_End, None ), 'home' : ( QC.Qt.Key.Key_Home, None ), 'page down' : ( QC.Qt.Key.Key_PageDown, None ), 'return' : ( QC.Qt.Key.Key_Return, None ), 'shift return' : ( QC.Qt.Key.Key_Return, QC.Qt.KeyboardModifier.ShiftModifier ), 'escape' : ( QC.Qt.Key.Key_Escape, None ) }[ step[ 1 ] ]

                key( ac._text_ctrl, name, modifier or QC.Qt.KeyboardModifier.NoModifier )

            elif what == 'advanced':

                # (the dialog the button opens is answered: cancelled, or
                # its input typed and OK pressed)
                class Dialog( QW.QDialog ):

                    def __init__( self, parent, title ):

                        super().__init__( parent )

                        dialogs.append( title )

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


                        self._panel._input_text.setText( step[ 1 ] )

                        QW.QApplication.processEvents()

                        previews.append( self._panel._result_preview.toPlainText() )

                        return QW.QDialog.DialogCode.Accepted



                A.ClientGUITopLevelWindowsPanels.DialogEdit = Dialog

                ac._or_advanced.click()

            elif what == 'tab':

                ac._dropdown_notebook.setCurrentWidget( tabs[ step[ 1 ] ] )

                if step[ 1 ] == 'children':

                    ac._children_list.NotifyNeedsUpdating()

                    ac._UpdateChildrenListIfNeeded()


            QW.QApplication.processEvents()

            settle( repr( step ) )

            entry = { 'step' : step, 'messages' : list( messages ), 'state' : state(), 'dialogs' : list( dialogs ), 'previews' : list( previews ) }

            del dialogs[:]
            del previews[:]

            out.append( entry )


        dlg.reject()

        return { 'favourites' : FAVOURITES, 'corpus' : [ { 'tag' : tag, 'hashes' : [ h.hex() for h in hashes[ :n ] ] } for ( tag, n ) in CORPUS ], 'parents' : PARENTS, 'siblings' : SIBLINGS, 'steps' : out }


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

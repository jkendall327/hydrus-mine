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

OUT = os.path.join( HERE, 'fixtures', 'favourite_editor_tabs_or.json' )

FAVOURITES = [ 'parity:tabs root', 'parity:tabs alpha old', 'parity:tabs unseen' ]

CORPUS = [ ( 'parity:tabs alpha', 3 ), ( 'parity:tabs beta', 1 ), ( 'parity:tabs alpha old', 1 ) ]
PARENTS = [ [ 'parity:tabs alpha', 'parity:tabs root' ], [ 'parity:tabs beta', 'parity:tabs root' ], [ 'parity:tabs zero', 'parity:tabs root' ], [ 'parity:tabs deep', 'parity:tabs alpha' ] ]
SIBLINGS = [ [ 'parity:tabs alpha old', 'parity:tabs alpha' ] ]

STEPS = [
    [ 'initial' ],
    [ 'shift type', 'series:metroid' ],
    [ 'shift type', 'character:samus aran' ],
    [ 'shift type', 'series:metroid' ],
    [ 'type', 'parity:tabs beta' ],
    [ 'type', 'series:metroid' ],
    [ 'shift type', 'parity:tabs alpha' ],
    [ 'shift type', 'parity:tabs beta' ],
    [ 'rewind' ],
    [ 'rewind' ],
    [ 'shift type', 'parity:tabs alpha' ],
    [ 'cancel' ],
    [ 'shift type', 'character:samus aran' ],
    [ 'type', 'series:metroid' ],
    [ 'shift type', 'parity:tabs alpha' ],
    [ 'shift type', 'parity:tabs beta' ],
    [ 'escape' ],
    [ 'escape' ],
    [ 'tab', 'favourites' ],
    [ 'activate row', 0 ],
    [ 'tab', 'children' ],
    [ 'activate row', 0 ],
    [ 'tab', 'children' ],
    [ 'text', 'draft' ],
    [ 'tab', 'favourites' ],
    [ 'shift activate row', 1 ],
    [ 'shift activate row', 2 ],
    [ 'activate row', 0 ],
    [ 'or button', None ],
    [ 'or button', [ 'parity:tabs root', 'parity:tabs unseen' ] ]
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

            return [ ''.join( t for ( t, _ ) in box._GetRowsOfTextsAndColours( term )[ 0 ] ) for term in box._ordered_terms ]


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
                'typed_left' : ac._text_ctrl.text()
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


        out = []

        for step in STEPS:

            del messages[:]

            what = step[ 0 ]

            if what in ( 'type', 'shift type' ):

                ac._dropdown_notebook.setCurrentWidget( ac._search_results_list )

                ac._text_ctrl.setText( step[ 1 ] )

                settle( 'the text ' + repr( step[ 1 ] ) )

                QW.QApplication.processEvents()

                modifier = QC.Qt.KeyboardModifier.ShiftModifier if what == 'shift type' else QC.Qt.KeyboardModifier.NoModifier

                key( ac._text_ctrl, QC.Qt.Key.Key_Return, modifier )

            elif what == 'text':

                # (typed, with no Return pressed)
                ac._text_ctrl.setText( step[ 1 ] )

            elif what == 'or button':

                # (the dialog the button opens is answered: cancelled, or
                # with an OR of these tags)
                if step[ 1 ] is None:

                    def edit_predicates( *args, **kwargs ):

                        raise HydrusExceptions.CancelledException( 'cancelled' )

                else:

                    def edit_predicates( *args, **kwargs ):

                        terms = [ ClientSearchPredicate.Predicate( ClientSearchPredicate.PREDICATE_TYPE_TAG, t ) for t in step[ 1 ] ]

                        return [ ClientSearchPredicate.Predicate( ClientSearchPredicate.PREDICATE_TYPE_OR_CONTAINER, value = terms ) ]


                ClientGUISearch.EditPredicates = edit_predicates

                ac._or_basic.click()

            elif what == 'rewind':

                ac._or_rewind.click()

            elif what == 'cancel':

                ac._or_cancel.click()

            elif what == 'escape':

                key( ac._text_ctrl, QC.Qt.Key.Key_Escape )

            elif what == 'tab':

                ac._dropdown_notebook.setCurrentWidget( tabs[ step[ 1 ] ] )

                if step[ 1 ] == 'children':

                    ac._children_list.NotifyNeedsUpdating()

                    ac._UpdateChildrenListIfNeeded()


            elif what in ( 'activate row', 'shift activate row' ):

                box = ac._dropdown_notebook.currentWidget()

                box._Hit( False, False, step[ 1 ] )

                box._Activate( False, what == 'shift activate row' )

            QW.QApplication.processEvents()

            settle( repr( step ) )

            entry = { 'step' : step, 'messages' : list( messages ), 'state' : state() }

            if what == 'tab':

                entry[ 'rows_drawn' ] = rows_of( ac._dropdown_notebook.currentWidget() )


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

#!/usr/bin/env python3
"""Record what the active predicate list selects after an edit, and the mixed
edit dialog's embedded OR control.

A real search page's `ListBoxTagsActiveSearchPredicates`. Each case sets the
search and the selection, then edits the selection (`_EditPredicates`, its
real `EditPredicatesPanel` answered through its own widgets) or enters
predicates as the autocomplete does (`EnterPredicates`). After each, the
search and the selected rows are recorded. In the mixed edits holding an OR,
the panel's embedded `ORPredicateControl` (a read autocomplete) has text
typed and Return pressed (real key events, after its result list has caught
up) and a predicate removed, and its predicates are recorded before and after.

Usage: QT_QPA_PLATFORM=offscreen python oracle/record_active_predicate_selection.py
       (writes fixtures/active_predicate_selection.json)
"""
import json
import os
import sys
import time

HERE = os.path.dirname( os.path.abspath( __file__ ) )

sys.path.insert( 0, HERE )

OUT = os.path.join( HERE, 'fixtures', 'active_predicate_selection.json' )


def record( session ):

    from qtpy import QtCore as QC
    from qtpy import QtTest
    from qtpy import QtWidgets as QW

    from hydrus.client import ClientConstants as CC
    from hydrus.client import ClientLocation
    from hydrus.client.gui import ClientGUITopLevelWindowsPanels as Windows
    from hydrus.client.search import ClientSearchFileSearchContext as F
    from hydrus.client.search import ClientSearchPredicate as P
    from hydrus.client.search import ClientSearchTagContext as T

    c = session.controller
    gui = c.gui
    qt = lambda f: c.CallBlockingToQt( gui, f )

    context = F.FileSearchContext( location_context = ClientLocation.LocationContext.STATICCreateSimple( CC.LOCAL_FILE_SERVICE_KEY ), tag_context = T.TagContext() )

    page = qt( lambda: gui._notebook.NewPageQueryFileSearchContext( context, page_name = 'active selection' ) )

    for _ in range( 500 ):

        sidebar = qt( page.GetSidebar )

        if sidebar is not None and hasattr( sidebar, '_tag_autocomplete' ):

            break

        time.sleep( .01 )


    ac = sidebar._tag_autocomplete
    box = ac._predicates_listbox

    tag = lambda name: P.Predicate( P.PREDICATE_TYPE_TAG, name )
    alpha, beta = tag( 'series:metroid' ), tag( 'blue eyes' )
    samus = tag( 'character:samus aran' )
    inbox = P.Predicate( P.PREDICATE_TYPE_SYSTEM_INBOX )
    archive = P.Predicate( P.PREDICATE_TYPE_SYSTEM_ARCHIVE )
    group = P.Predicate( P.PREDICATE_TYPE_OR_CONTAINER, [ alpha, samus ] )

    plan = {}

    def settle( inner ):

        for _ in range( 1200 ):

            QtTest.QTest.qWait( 50 )

            loading = any( 'loading' in term.GetPredicate().ToString( with_count = False ) for term in inner._search_results_list._ordered_terms )

            if inner._GetParsedAutocompleteText() == inner._current_list_parsed_autocomplete_text and not loading:

                return


            if _ % 400 == 399:

                typed = inner._text_ctrl.text()
                inner._text_ctrl.setText( '' )
                QtTest.QTest.qWait( 50 )
                inner._text_ctrl.setText( typed )


        raise Exception( 'the OR control never caught up' )


    def dialog_exec( dialog ):

        panel = dialog._panel
        panels = panel._editable_pred_panels

        plan[ 'title' ] = dialog.windowTitle()
        plan[ 'panels' ] = [ type( p ).__name__ for p in panels ]
        plan[ 'invertible_before' ] = [ b.text() for b in panel._invertible_pred_buttons ]

        for ( simple, text ) in zip( [ p for p in panels if type( p ).__name__ == 'PanelPredicateSimpleTagTypes' ], plan.get( 'simple', [] ) ):

            simple._simple_tag_text.setText( text )


        if plan.get( 'flip' ):

            for button in panel._invertible_pred_buttons:

                button.click()



        for control in [ p for p in panels if type( p ).__name__ == 'ORPredicateControl' ]:

            inner = control._search_control

            listbox = inner._predicates_listbox

            plan[ 'or_before' ] = [ term.GetPredicate().ToString() for term in listbox._ordered_terms ]

            for text in plan.get( 'or_type', [] ):

                inner._text_ctrl.setText( text )

                settle( inner )

                QtTest.QTest.keyClick( inner._text_ctrl, QC.Qt.Key.Key_Return )

                QW.QApplication.processEvents()

                settle( inner )


            for text in plan.get( 'or_remove', [] ):

                [ predicate ] = [ term.GetPredicate() for term in listbox._ordered_terms if term.GetPredicate().ToString() == text ]

                listbox.EnterPredicates( [ predicate ], permit_add = False )


            plan[ 'or_after' ] = [ term.GetPredicate().ToString() for term in listbox._ordered_terms ]


        plan[ 'invertible_after' ] = [ b.text() for b in panel._invertible_pred_buttons ]

        plan[ 'value' ] = sorted( p.ToString() for p in panel.GetValue() )

        return QW.QDialog.DialogCode.Accepted if plan[ 'accepted' ] else QW.QDialog.DialogCode.Rejected


    old_exec = Windows.DialogEdit.exec

    Windows.DialogEdit.exec = dialog_exec

    def set_values( values, selected ):

        ctx = context.Duplicate()
        ctx.SetPredicates( values )
        ac.SetFileSearchContext( ctx )
        ac.SetSynchronised( False )

        box._selected_terms = { box._GenerateTermFromPredicate( p ) for p in selected }


    def state():

        return {
            'shown' : [ term.GetPredicate().ToString() for term in box._ordered_terms ],
            'selected' : sorted( term.GetPredicate().ToString() for term in box._selected_terms )
        }


    cases = [
        { 'name' : 'edit a tag', 'values' : [ alpha, beta, inbox ], 'selected' : [ alpha ], 'edit' : True, 'accepted' : True, 'simple' : [ 'series:edited' ] },
        { 'name' : 'edit two tags', 'values' : [ alpha, beta, inbox ], 'selected' : [ alpha, beta ], 'edit' : True, 'accepted' : True, 'simple' : [ 'series:one', 'series:two' ] },
        { 'name' : 'edit a tag, the other unchanged', 'values' : [ alpha, beta, inbox ], 'selected' : [ alpha, beta ], 'edit' : True, 'accepted' : True, 'simple' : [ 'blue eyes', 'series:changed' ] },
        { 'name' : 'edit a tag and flip inbox', 'values' : [ alpha, beta, inbox ], 'selected' : [ alpha, inbox ], 'edit' : True, 'accepted' : True, 'simple' : [ 'series:other' ], 'flip' : True },
        { 'name' : 'edit cancelled', 'values' : [ alpha, beta, inbox ], 'selected' : [ alpha ], 'edit' : True, 'accepted' : False, 'simple' : [ 'series:never' ] },
        { 'name' : 'enter what replaces a selected row', 'values' : [ inbox, beta ], 'selected' : [ inbox ], 'enter' : [ archive ] },
        { 'name' : 'enter what replaces an unselected row', 'values' : [ inbox, beta ], 'selected' : [ beta ], 'enter' : [ archive ] },
        { 'name' : 'enter a new row', 'values' : [ inbox, beta ], 'selected' : [ beta ], 'enter' : [ alpha ] },
        { 'name' : 'mixed edit with an OR', 'values' : [ group, inbox, beta ], 'selected' : [ group, inbox ], 'edit' : True, 'accepted' : True, 'flip' : True, 'or_type' : [ 'blue eyes' ], 'or_remove' : [ 'series:metroid' ] },
        { 'name' : 'mixed edit with an OR, the OR unchanged', 'values' : [ group, inbox, beta ], 'selected' : [ group, inbox ], 'edit' : True, 'accepted' : True, 'flip' : True },
        { 'name' : 'mixed edit with an OR, cancelled', 'values' : [ group, inbox, beta ], 'selected' : [ group, inbox ], 'edit' : True, 'accepted' : False, 'flip' : True, 'or_type' : [ 'blue eyes' ] },
        { 'name' : 'mixed edit with a tag and an OR', 'values' : [ group, alpha, beta ], 'selected' : [ group, beta ], 'edit' : True, 'accepted' : True, 'simple' : [ 'series:new' ], 'or_remove' : [ 'character:samus aran' ] }
    ]

    def replay():

        out = []

        for case in cases:

            plan.clear()
            plan.update( { k : v for ( k, v ) in case.items() if k not in ( 'values', 'selected', 'enter' ) } )

            set_values( case[ 'values' ], case[ 'selected' ] )

            before = state()

            if case.get( 'edit' ):

                box._EditPredicates( set( case[ 'selected' ] ) )

            else:

                box.EnterPredicates( case[ 'enter' ] )


            QW.QApplication.processEvents()

            entry = { 'name' : case[ 'name' ], 'before' : before, 'after' : state(), 'enter' : [ p.ToString() for p in case.get( 'enter', [] ) ] }

            entry.update( { k : v for ( k, v ) in plan.items() if k != 'name' } )

            out.append( entry )


        return out


    try:

        return { 'cases' : qt( replay ) }

    finally:

        Windows.DialogEdit.exec = old_exec



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

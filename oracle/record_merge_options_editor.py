#!/usr/bin/env python3
"""Record the reference's duplicate metadata merge options editor
(`EditDuplicateContentMergeOptionsWidget`: the duplicates page's "edit
default duplicate metadata merge options", and a duplicates auto-resolution
rule's custom merge options).

The editor says which decision it edits for ("Editing for "this is
better"."), lists tag services (name, action, tag filter) with "add",
"edit action", "edit filter" and "delete", and rating services (name,
action) with "add", "edit action" and "delete", and has choices for
syncing archived status, file modified time, known urls and notes, and a
"note merge settings" button. Only "this is better" offers actions other
than "copy in both directions"; for alternates and false positives (not a
custom action) the url, modified time and notes choices are off.

On the `basic` fixture, for each case of `CASES` (the decision, whether a
custom action, the options to start from: the client's for the decision,
or none), this makes the widget and records, after each step, its note,
its lists' rows in order, whether "edit action" shows for each list, each
choice's options and its value and whether enabled, whether "note merge
settings" is enabled, and the value (`GetValue`), with what was asked
(the select dialogs' titles, choices and preselection, the edit dialogs'
titles, and warnings). Steps: `["add_tag", service, action, rules]` (the
service and action chosen by label, `null` when not asked, the filter
given back), `["add_rating", service, action]`, `["edit_tag_action",
service, action]`, `["edit_tag_filter", service, rules]`,
`["edit_rating", service, action]`, `["delete_tags", [services]]`,
`["delete_ratings", [services]]`, `["set", choice, label]` (choice one of
`archive`, `urls`, `file_modified`, `notes`) and `["note_settings",
extend, conflict]`.

Usage: QT_QPA_PLATFORM=offscreen python oracle/record_merge_options_editor.py
       (writes fixtures/merge_options_editor.json)
"""

import json
import os
import sys

HERE = os.path.dirname( os.path.abspath( __file__ ) )

sys.path.insert( 0, HERE )

OUT = os.path.join( HERE, 'fixtures', 'merge_options_editor.json' )

CASES = [
    { 'decision' : 'better', 'custom' : False, 'start' : 'client', 'steps' : [
        [ 'add_tag', None, None, [] ],
        [ 'delete_tags', [ 'downloader tags' ] ],
        [ 'add_tag', 'downloader tags', 'copy from worse to better', [ [ 'creator:', 'black' ] ] ],
        [ 'edit_tag_action', 'my tags', 'copy in both directions' ],
        [ 'edit_tag_filter', 'my tags', [ [ '', 'black' ], [ ':', 'black' ], [ 'series:', 'white' ] ] ],
        [ 'add_rating', None, None ],
        [ 'delete_ratings', [ 'favourites' ] ],
        [ 'add_rating', 'favourites', 'move from worse to better' ],
        [ 'edit_rating', 'favourites', 'copy in both directions' ],
        [ 'set', 'archive', 'if one is archived, archive the other' ],
        [ 'set', 'urls', 'make no change' ],
        [ 'set', 'file_modified', 'both get earliest' ],
        [ 'set', 'notes', 'make no change' ],
        [ 'set', 'notes', 'move from worse to better' ],
        [ 'note_settings', False, 'append' ],
    ] },
    { 'decision' : 'same quality', 'custom' : False, 'start' : 'client', 'steps' : [
        [ 'delete_tags', [ 'my tags', 'downloader tags' ] ],
        [ 'add_tag', 'my tags', None, [] ],
        [ 'set', 'urls', 'make no change' ],
    ] },
    { 'decision' : 'alternates', 'custom' : False, 'start' : 'client', 'steps' : [
        [ 'add_rating', 'favourites', None ],
        [ 'set', 'archive', 'always archive both' ],
    ] },
    { 'decision' : 'alternates', 'custom' : True, 'start' : 'client', 'steps' : [
        [ 'set', 'notes', 'copy in both directions' ],
    ] },
    { 'decision' : 'better', 'custom' : False, 'start' : 'empty', 'steps' : [
    ] },
]

CONFLICTS = { 'replace' : 0, 'ignore' : 1, 'append' : 2, 'rename' : 3 }


class Index:

    def __init__( self, row ):

        self._row = row


    def row( self ):

        return self._row



def record( session ):

    controller = session.controller
    gui = controller.gui

    def f():

        from qtpy import QtWidgets as QW

        from hydrus.client.duplicates import ClientDuplicates
        from hydrus.client.gui import ClientGUIDialogsMessage
        from hydrus.client.gui import ClientGUIDialogsQuick
        from hydrus.client.gui.duplicates import ClientGUIDuplicatesContentMergeOptions as M
        from hydrus.client.importing.options import NoteImportOptions
        from hydrus.core import HydrusConstants as HC
        from hydrus.core import HydrusExceptions
        from hydrus.core import HydrusTags

        services_manager = controller.services_manager

        said = []
        answers = []

        def select_from_list( win, title, choice_tuples, value_to_select = None, sort_tuples = True, allow_insta_one_item_select = True ):

            if len( choice_tuples ) == 1 and allow_insta_one_item_select:

                said.append( { 'auto' : title, 'choices' : [ t for ( t, d ) in choice_tuples ] } )

                return choice_tuples[0][1]


            texts = [ t for ( t, d ) in choice_tuples ]

            preselected = [ t for ( t, d ) in choice_tuples if d == value_to_select ]

            said.append( { 'select' : title, 'choices' : texts, 'sorted' : sort_tuples, 'preselected' : preselected[0] if preselected else None } )

            answer = answers.pop( 0 )

            if answer is None:

                raise HydrusExceptions.CancelledException()


            return [ d for ( t, d ) in choice_tuples if t == answer ][0]


        def warning( win, message, **kwargs ):

            said.append( { 'warning' : message } )


        def yes_no( win, message, **kwargs ):

            said.append( { 'asked' : message } )

            return QW.QDialog.DialogCode.Accepted


        ClientGUIDialogsQuick.SelectFromList = select_from_list
        ClientGUIDialogsQuick.GetYesNo = yes_no
        ClientGUIDialogsMessage.ShowWarning = warning

        class Dialog:

            def __init__( self, win, title, **kwargs ):

                said.append( { 'dialog' : title } )


            def __enter__( self ):

                return self


            def __exit__( self, *args ):

                return False


            def SetPanel( self, panel ):

                pass


            def exec( self ):

                return QW.QDialog.DialogCode.Accepted



        class SingleCtrl:

            def __init__( self, parent ):

                pass


            def SetControl( self, ctrl ):

                pass



        class FakeTagFilterPanel:

            def __init__( self, parent, tag_filter, namespaces = None, **kwargs ):

                said.append( { 'filter given' : rules_of( tag_filter ) } )

                self._value = answers.pop( 0 )


            def GetValue( self ):

                return self._value



        class FakeNotesPanel:

            def __init__( self, parent, note_import_options, simple_mode = False ):

                said.append( { 'notes given' : [ note_import_options.GetExtendExistingNoteIfPossible(), note_import_options.GetConflictResolution() ], 'simple' : simple_mode } )

                self._value = answers.pop( 0 )


            def GetValue( self ):

                return self._value



        M.ClientGUITopLevelWindowsPanels.DialogEdit = Dialog
        M.ClientGUIScrolledPanels.EditSingleCtrlPanel = SingleCtrl
        M.ClientGUITagFilter.EditTagFilterPanel = FakeTagFilterPanel
        M.ClientGUIImportOptionsPanels.EditNoteImportOptionsPanel = FakeNotesPanel

        def rules_of( tag_filter ):

            return sorted( [ [ s, 'black' if r == HC.FILTER_BLACKLIST else 'white' ] for ( s, r ) in tag_filter.GetTagSlicesToRules().items() ] )


        def make_filter( rules ):

            tag_filter = HydrusTags.TagFilter()

            for ( s, r ) in rules:

                tag_filter.SetRule( s, HC.FILTER_BLACKLIST if r == 'black' else HC.FILTER_WHITELIST )


            return tag_filter


        def name( service_key ):

            return services_manager.GetNameSafe( service_key )


        def key( service_name ):

            for service in services_manager.GetServices():

                if service.GetName() == service_name:

                    return service.GetServiceKey()



        decisions = {
            'better' : HC.DUPLICATE_BETTER,
            'same quality' : HC.DUPLICATE_SAME_QUALITY,
            'alternates' : HC.DUPLICATE_ALTERNATE,
            'false positive' : HC.DUPLICATE_FALSE_POSITIVE,
        }

        out = []

        for case in CASES:

            duplicate_type = decisions[ case[ 'decision' ] ]

            if case[ 'start' ] == 'client':

                options = controller.new_options.GetDuplicateContentMergeOptions( duplicate_type )

            else:

                options = ClientDuplicates.DuplicateContentMergeOptions()


            widget = M.EditDuplicateContentMergeOptionsWidget( gui, duplicate_type, options, for_custom_action = case[ 'custom' ] )

            def choice( c ):

                return {
                    'options' : [ c.itemText( i ) for i in range( c.count() ) ],
                    'value' : c.currentText(),
                    'enabled' : c.isEnabled(),
                }


            def value():

                v = widget.GetValue()

                note_options = v.GetSyncNoteImportOptions()

                return {
                    'tags' : [ [ name( k ), HC.content_merge_string_lookup[ a ], rules_of( f ) ] for ( k, a, f ) in v.GetTagServiceActions() ],
                    'ratings' : [ [ name( k ), HC.content_merge_string_lookup[ a ] ] for ( k, a ) in v.GetRatingServiceActions() ],
                    'archive' : v.GetSyncArchiveAction(),
                    'urls' : HC.content_merge_string_lookup[ v.GetSyncURLsAction() ],
                    'file_modified' : HC.content_merge_string_lookup[ v.GetSyncFileModifiedDateAction() ],
                    'notes' : HC.content_merge_string_lookup[ v.GetSyncNotesAction() ],
                    'note_extend' : note_options.GetExtendExistingNoteIfPossible(),
                    'note_conflict' : note_options.GetConflictResolution(),
                }


            def rows( ctrl ):

                model = ctrl.model()

                return [ [ model.data( model.index( r, c ) ) for c in range( model.columnCount() ) ] for r in range( model.rowCount() ) ]


            def state():

                return {
                    'note' : widget._not_better_note_st.text(),
                    'tag_rows' : rows( widget._tag_service_actions ),
                    'rating_rows' : rows( widget._rating_service_actions ),
                    'tag_edit_action_shown' : not widget._edit_tag_action_button.isHidden(),
                    'rating_edit_action_shown' : not widget._edit_rating_button.isHidden(),
                    'archive' : choice( widget._sync_archive_action ),
                    'file_modified' : choice( widget._sync_file_modified_date_action ),
                    'urls' : choice( widget._sync_urls_action ),
                    'notes' : choice( widget._sync_notes_action ),
                    'note_settings_enabled' : widget._sync_note_import_options_button.isEnabled(),
                    'value' : value(),
                }


            def select( ctrl, keys ):

                ctrl.SelectDatas( keys, deselect_others = True )


            states = [ { 'step' : None, 'state' : state() } ]

            for step in case[ 'steps' ]:

                del said[:]
                del answers[:]

                ( kind, *args ) = step

                if kind == 'add_tag':

                    answers.extend( [ args[ 0 ], args[ 1 ] ] if args[ 1 ] is not None else [ args[ 0 ] ] )
                    answers.append( make_filter( args[ 2 ] ) )

                    widget._AddTag()

                elif kind == 'add_rating':

                    answers.extend( [ args[ 0 ], args[ 1 ] ] if args[ 1 ] is not None else [ args[ 0 ] ] )

                    widget._AddRating()

                elif kind == 'edit_tag_action':

                    select( widget._tag_service_actions, [ key( args[ 0 ] ) ] )

                    answers.append( args[ 1 ] )

                    widget._EditTagAction()

                elif kind == 'edit_tag_filter':

                    select( widget._tag_service_actions, [ key( args[ 0 ] ) ] )

                    answers.append( make_filter( args[ 1 ] ) )

                    widget._EditTagFilter()

                elif kind == 'edit_rating':

                    select( widget._rating_service_actions, [ key( args[ 0 ] ) ] )

                    answers.append( args[ 1 ] )

                    widget._EditRating()

                elif kind == 'delete_tags':

                    select( widget._tag_service_actions, [ key( n ) for n in args[ 0 ] ] )

                    widget._DeleteTag()

                elif kind == 'delete_ratings':

                    select( widget._rating_service_actions, [ key( n ) for n in args[ 0 ] ] )

                    widget._DeleteRating()

                elif kind == 'set':

                    c = { 'archive' : widget._sync_archive_action, 'urls' : widget._sync_urls_action, 'file_modified' : widget._sync_file_modified_date_action, 'notes' : widget._sync_notes_action }[ args[ 0 ] ]

                    c.setCurrentIndex( [ c.itemText( i ) for i in range( c.count() ) ].index( args[ 1 ] ) )

                elif kind == 'note_settings':

                    note_options = NoteImportOptions.NoteImportOptions()
                    note_options.SetExtendExistingNoteIfPossible( args[ 0 ] )
                    note_options.SetConflictResolution( CONFLICTS[ args[ 1 ] ] )

                    answers.append( note_options )

                    widget._EditNoteImportOptions()


                states.append( { 'step' : step, 'said' : list( said ), 'state' : state() } )


            out.append( { 'decision' : case[ 'decision' ], 'custom' : case[ 'custom' ], 'start' : case[ 'start' ], 'states' : states } )

            widget.deleteLater()


        tag_services = [ s.GetName() for s in services_manager.GetServices( HC.REAL_TAG_SERVICES ) ]
        rating_services = [ s.GetName() for s in services_manager.GetServices( HC.RATINGS_SERVICES ) ]

        return { 'tag_services' : tag_services, 'rating_services' : rating_services, 'cases' : out }


    return controller.CallBlockingToQt( gui, f )


def child( out ):

    import hydrus_driver
    import record_api

    db_dir = record_api.unpack_fixture( 'basic' )

    result = hydrus_driver.run_client( db_dir, record )

    with open( out, 'w' ) as f:

        json.dump( result, f, ensure_ascii = False )



def main():

    if len( sys.argv ) > 1 and sys.argv[ 1 ] == '--child':

        child( sys.argv[ 2 ] )

        return


    import tempfile

    import hydrus_driver

    with tempfile.TemporaryDirectory() as work:

        path = os.path.join( work, 'merge_options_editor.json' )

        hydrus_driver.run_in_subprocess( os.path.abspath( __file__ ), '--child', path )

        with open( path ) as f:

            result = json.load( f )



    with open( OUT, 'w' ) as f:

        json.dump( result, f, indent = 1, ensure_ascii = False )
        f.write( '\n' )


    print( f'wrote {OUT}: {len( result[ "cases" ] )} cases' )


if __name__ == '__main__':

    main()

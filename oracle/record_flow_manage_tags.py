#!/usr/bin/env python3
"""Flow: manage tags across many files, with siblings and parents in play.

On the `basic` fixture (no site; the flow is all local), on a search page
of every file, with four of its files A, B, C and D (the first four the
fixture lists that the page shows):

1. "relationships": tags > manage tag siblings, `kitty` → `cat` on
   "my tags", then tags > manage tag parents, `cat` → `animal` (the real
   dialogs, as `record_flow_tag_relationships.py` drives them); tag display
   maintenance run to the end.
2. "add": A, B and C selected; manage tags (the real `ManageTagsPanel`),
   "my tags": `cat` and `fluffy` typed and entered (`AddTags`, what the
   autocomplete gives on enter); apply. Then D alone: `kitty`.
3. "mixed": all four selected; manage tags: `fluffy`, which three of them
   have, activated in the tag list (typed, it would only add, as hydrus's
   "allow remove on manage tags input" is off): the reference asks what to
   do (`SelectFromListButtons`, answered "delete"); then `striped` typed
   (added to all four); apply.
4. "remove": A alone; manage tags: `striped` selected in the list and the
   remove button pressed; apply.
5. "copy": A alone; manage tags: the copy button with nothing selected
   (all its tags); cancel. B alone: manage tags, the autocomplete's paste
   button; apply. (Not D: tags with the same ideal, D's `kitty` and a
   pasted `cat`, tie in the list's sort, which the reference breaks by the
   order they came in.)
6. "undo": what the undo menu offers after the tag edits (nothing: the
   reference's manage tags commits past its undo manager); A and B (in
   the archive) sent to the inbox from the thumbnails, which asks first;
   undo > the undo item, then undo > redo
   (what they say, and the files' inbox after each).
7. "search": search pages for `cat`, `kitty`, `animal`, `striped`,
   `fluffy` and `-fluffy` (each page's files).

After each step: what the dialogs showed (the tag list's rows as opened,
the question with its choices, what was copied, the undo menu's items),
and each of the four files' tags in "my tags", as stored and as displayed.

Usage: QT_QPA_PLATFORM=offscreen ~/pyenv/bin/python oracle/record_flow_manage_tags.py
       (writes oracle/fixtures/flow_manage_tags.json)
"""

import json
import os
import sys
import time

HERE = os.path.dirname( os.path.abspath( __file__ ) )
sys.path.insert( 0, HERE )

import flows

OUT = os.path.join( HERE, 'fixtures', 'flow_manage_tags.json' )


def record( session ):

    from qtpy import QtWidgets as QW
    from hydrus.client import ClientConstants as CC, ClientLocation
    from hydrus.client.gui import ClientGUIDialogsQuick
    from hydrus.client.media import ClientMediaFileFilter
    from hydrus.client.metadata import ClientTags
    from hydrus.client.search import ClientSearchPredicate as P
    from hydrus.core import HydrusConstants as HC

    controller = session.controller
    gui = controller.gui
    services = controller.services_manager

    def qt( f ):

        return controller.CallBlockingToQt( gui, f )


    manifest = json.load( open( os.path.join( HERE, 'fixtures', 'legacy_db', 'basic.manifest.json' ) ) )
    manifest_hashes = [ f[ 'hash' ] for f in manifest[ 'files' ] ]

    local = next( s.GetServiceKey() for s in services.GetServices( ( HC.LOCAL_TAG, ) ) if s.GetName() == 'my tags' )
    location = ClientLocation.LocationContext.STATICCreateSimple( CC.LOCAL_FILE_SERVICE_KEY )

    # the clipboard, as the client's copy and paste reach it
    clipboard = { 'text' : None }
    real_pub = controller.pub

    def pub( topic, *args, **kwargs ):

        if topic == 'clipboard' and args[0] == 'text':

            clipboard[ 'text' ] = args[1]

            return


        return real_pub( topic, *args, **kwargs )


    controller.pub = pub
    controller.GetClipboardText = lambda: clipboard[ 'text' ]

    # the question manage tags asks, answered by the label of the choice
    asked = []
    answers = []

    def select_from_list_buttons( win, title, choices, message = '' ):

        asked.append( { 'title' : title, 'message' : message, 'choices' : [ c[0] for c in choices ] } )

        wanted = answers.pop( 0 )

        return next( data for ( text, data, tooltip ) in choices if text.startswith( wanted ) )


    ClientGUIDialogsQuick.SelectFromListButtons = select_from_list_buttons

    # its yes/no questions (removing tags asks first), answered yes
    def get_yes_no( win, message, title = 'Are you sure?', yes_label = 'yes', no_label = 'no', **kwargs ):

        asked.append( { 'title' : title, 'message' : message, 'choices' : [ yes_label, no_label ] } )

        return QW.QDialog.DialogCode.Accepted


    ClientGUIDialogsQuick.GetYesNo = get_yes_no

    from hydrus.client.gui.metadata import ClientGUIManageTags

    ClientGUIManageTags.ClientGUIDialogsQuick.SelectFromListButtons = select_from_list_buttons

    steps = []

    def pump( condition, what, timeout = 30 ):
        """Let the Qt event loop run (on the Qt thread) until `condition`."""

        deadline = time.time() + timeout

        while not condition():

            if time.time() > deadline:

                raise Exception( 'timed out: ' + what )


            QW.QApplication.processEvents()
            time.sleep( 0.02 )



    def pump_off_thread( condition, what, timeout = 60 ):

        deadline = time.time() + timeout

        while not condition():

            if time.time() > deadline:

                raise Exception( 'timed out: ' + what )


            time.sleep( 0.1 )



    def sync_tag_display():

        for _ in range( 60 ):

            if not controller.WriteSynchronous( 'sync_tag_display_maintenance', local, 1.0 ):

                return


        raise Exception( 'tag display maintenance did not finish' )


    def new_page( predicates ):

        page = qt( lambda: gui._notebook.NewPageQuery( location, initial_predicates = predicates, do_sort = False ) )

        last = [ None, 0 ]

        def loaded():

            panel = qt( page.GetMediaResultsPanel )

            if not hasattr( panel, '_sorted_media' ):

                return False


            n = len( panel._sorted_media )

            if n != last[0]:

                last[0] = n
                last[1] = time.time()

                return False


            return time.time() - last[1] > 1.5


        pump_off_thread( loaded, 'the page to load' )

        return page


    everything = P.Predicate( P.PREDICATE_TYPE_SYSTEM_EVERYTHING )

    page = new_page( [ everything ] )

    listed = { m.GetHash().hex() for m in qt( lambda: page.GetMediaResultsPanel().GetSortedMedia() ) }
    files = [ h for h in manifest_hashes if h in listed ][ : 4 ]

    ( A, B, C, D ) = files

    def tags_of( hash_hex ):

        media_result = session.read( 'media_result', bytes.fromhex( hash_hex ) )
        tags_manager = media_result.GetTagsManager()

        return {
            'stored' : sorted( tags_manager.GetCurrent( local, ClientTags.TAG_DISPLAY_STORAGE ) ),
            'displayed' : sorted( tags_manager.GetCurrent( local, ClientTags.TAG_DISPLAY_DISPLAY_ACTUAL ) ),
        }


    def step( name, seen, extra = None ):

        flows.wait_until_idle( controller )

        steps.append( {
            'step' : name,
            'dialog' : seen,
            'files' : { h : tags_of( h ) for h in files },
            # (everything else each file shows: every local tag service's
            # tags, URLs, inbox and domains, so a stray write shows)
            'state' : flows.files_state( session, files, lambda t: t ),
            **( extra or {} ),
        } )


    def media_panel():

        return page.GetMediaResultsPanel()


    def select( hashes ):

        panel = media_panel()

        panel._Select( ClientMediaFileFilter.FileFilter( ClientMediaFileFilter.FILE_FILTER_NONE ) )

        for ( i, h ) in enumerate( hashes ):

            media = next( m for m in panel.GetSortedMedia() if m.GetHash().hex() == h )

            panel._HitMedia( media, i > 0, False )



    def mine_of( panel ):

        tabs = panel._tag_services
        mine = next( tabs.widget( i ) for i in range( tabs.count() ) if tabs.widget( i ).GetServiceKey() == local )

        tabs.setCurrentWidget( mine )

        return mine


    def tag_rows( mine ):

        box = mine._tags_box

        # (each tag's sibling and parent decorators arrive in the background)
        def settled():

            with box._async_text_info_lock:

                return len( box._pending_async_text_info_terms ) == 0 and len( box._currently_fetching_async_text_info_terms ) == 0


        pump( settled, 'the tag list\'s sibling and parent info' )

        deadline = time.time() + 0.5

        while time.time() < deadline:

            QW.QApplication.processEvents()
            time.sleep( 0.02 )


        out = []

        for term in box._ordered_terms:

            ( first, *parents ) = [ ''.join( text for ( text, colour ) in row ) for row in box._GetRowsOfTextsAndColours( term ) ]

            # (a tag's parents in the set's order, which varies run to run)
            out.extend( [ first ] + sorted( parents ) )


        return out


    def manage( hashes, script, accept = True ):
        """Manage tags on `hashes` selected: `script( dialog, mine, seen )` acts
        on the "my tags" tab; apply (or cancel). What it saw."""

        seen = {}

        def scripted( dlg, panel ):

            mine = mine_of( panel )

            # (the list fills as it opens)
            pump( lambda: mine._tags_box._ordered_terms is not None, 'the tag list' )

            seen[ 'title' ] = dlg.windowTitle()
            seen[ 'rows' ] = tag_rows( mine )

            script( dlg, mine, seen )

            seen[ 'rows after' ] = tag_rows( mine )

            return accept


        qt( lambda: select( hashes ) )

        with flows.scripted_dialogs( { 'ManageTagsPanel' : [ scripted ] } ):

            qt( lambda: media_panel()._ManageTags() )


        return seen


    # 1. relationships ---------------------------------------------------

    def pairs_dialog( enter, table_attr, pertinent, seen ):

        def script( dlg, panel ):

            tabs = panel._tag_services
            pairs = next( tabs.widget( i ) for i in range( tabs.count() ) if tabs.widget( i ).GetServiceKey() == local )

            tabs.setCurrentWidget( pairs )

            pump( lambda: pairs._sync_status_st.text() not in ( '', 'initialising sync data…' ), 'the dialog\'s sync status' )

            enter( pairs )

            pump( lambda: pertinent <= pairs._current_pertinent_tags, 'the entered tags in the workspace' )

            pairs._AddButton()

            table = getattr( pairs, table_attr )

            pump( lambda: len( table.GetData() ) > 0, 'the pair in the list' )

            seen[ 'rows' ] = [ list( pairs._ConvertPairToDisplayTuple( pair ) ) for pair in table.GetData() ]

            return True

        return script


    seen = { 'siblings' : {}, 'parents' : {} }

    def enter_sibling( pairs ):

        pairs.EnterOlds( { 'kitty' } )
        pairs.SetNew( { 'cat' } )


    def enter_parent( pairs ):

        pairs.EnterChildren( { 'cat' } )
        pairs.EnterParents( { 'animal' } )


    with flows.scripted_dialogs( { 'ManageTagSiblings' : [ pairs_dialog( enter_sibling, '_tag_siblings', { 'kitty', 'cat' }, seen[ 'siblings' ] ) ] } ):

        qt( lambda: gui._ManageTagSiblings() )


    sync_tag_display()

    with flows.scripted_dialogs( { 'ManageTagParents' : [ pairs_dialog( enter_parent, '_tag_parents', { 'cat', 'animal' }, seen[ 'parents' ] ) ] } ):

        qt( lambda: gui._ManageTagParents() )


    sync_tag_display()

    step( 'relationships', seen )

    # 2. add -------------------------------------------------------------

    def enter( *tags ):

        def script( dlg, mine, seen ):

            # (typed and entered: the autocomplete's broadcast, `AddTags`)
            for tag in tags:

                mine.AddTags( [ tag ] )


        return script


    seen = {
        'A B C' : manage( [ A, B, C ], enter( 'cat', 'fluffy' ) ),
        'D' : manage( [ D ], enter( 'kitty' ) ),
    }

    sync_tag_display()

    step( 'add', seen )

    # 3. mixed -----------------------------------------------------------

    answers.append( 'delete' )

    def mixed( dlg, mine, seen ):

        # `fluffy` activated in the list (typed, it would only add)
        box = mine._tags_box

        term = next( t for t in box._ordered_terms if t.GetTag() == 'fluffy' )

        box._Hit( False, False, box._ordered_terms.index( term ) )
        box._Activate( False, False )

        mine.AddTags( [ 'striped' ] )


    seen = manage( [ A, B, C, D ], mixed )

    seen[ 'asked' ] = list( asked )

    asked.clear()

    sync_tag_display()

    step( 'mixed', seen )

    # 4. remove ----------------------------------------------------------

    def remove_striped( dlg, mine, seen ):

        box = mine._tags_box

        term = next( t for t in box._ordered_terms if t.GetTag() == 'striped' )

        box._Hit( False, False, box._ordered_terms.index( term ) )

        mine._RemoveTagsButton()


    seen = manage( [ A ], remove_striped )

    seen[ 'asked' ] = list( asked )

    asked.clear()

    sync_tag_display()

    step( 'remove', seen )

    # 5. copy ------------------------------------------------------------

    def copy( dlg, mine, seen ):

        mine._Copy()

        seen[ 'copied' ] = clipboard[ 'text' ]


    def paste( dlg, mine, seen ):

        mine._add_tag_box._Paste()


    seen = {
        'A' : manage( [ A ], copy, accept = False ),
        'B' : manage( [ B ], paste ),
    }

    seen[ 'asked' ] = list( asked )

    sync_tag_display()

    step( 'copy', seen )

    # 6. undo ------------------------------------------------------------

    undo_manager = controller.GetManager( 'undo' )

    def inbox():

        return { h : session.read( 'media_result', bytes.fromhex( h ) ).GetLocationsManager().inbox for h in files }


    # (the tag edits above are not undoable in the reference: manage tags
    # commits them past its undo manager)
    seen = { 'after tagging' : list( undo_manager.GetUndoRedoStrings() ) }

    qt( lambda: select( [ A, B ] ) )
    qt( lambda: media_panel()._Inbox() )

    flows.wait_until_idle( controller )

    seen[ 'asked' ] = list( asked )
    seen[ 'inboxed' ] = inbox()
    seen[ 'after inbox' ] = list( undo_manager.GetUndoRedoStrings() )

    qt( lambda: gui._menubar_undo_undo.trigger() )

    flows.wait_until_idle( controller )

    seen[ 'undone' ] = inbox()
    seen[ 'after undo' ] = list( undo_manager.GetUndoRedoStrings() )

    qt( lambda: gui._menubar_undo_redo.trigger() )

    flows.wait_until_idle( controller )

    seen[ 'redone' ] = inbox()
    seen[ 'after redo' ] = list( undo_manager.GetUndoRedoStrings() )

    step( 'undo', seen )

    # 7. search ----------------------------------------------------------

    def tag_predicate( text ):

        if text.startswith( '-' ):

            return P.Predicate( P.PREDICATE_TYPE_TAG, value = text[1:], inclusive = False )


        return P.Predicate( P.PREDICATE_TYPE_TAG, value = text )


    searches = {}

    for text in ( 'cat', 'kitty', 'animal', 'striped', 'fluffy', '-fluffy' ):

        predicates = [ tag_predicate( text ) ]

        if text.startswith( '-' ):

            predicates.insert( 0, everything )


        searched = new_page( predicates )

        found = sorted( m.GetHash().hex() for m in qt( lambda: searched.GetMediaResultsPanel().GetSortedMedia() ) )

        # (the fixture's own files are not this flow's: only the four)
        searches[ text ] = [ h for h in found if h in files ]


    step( 'search', {}, { 'searches' : searches } )

    return { 'files' : files, 'steps' : steps }


def child( out ):

    import hydrus_driver
    import record_api

    result = hydrus_driver.run_client( record_api.unpack_fixture( 'basic' ), record )

    flows.write_fixture( out, result )


def main():

    if len( sys.argv ) > 2 and sys.argv[1] == '--child':

        child( sys.argv[2] )

        return


    import hydrus_driver

    hydrus_driver.run_in_subprocess( os.path.abspath( __file__ ), '--child', OUT )


if __name__ == '__main__':

    main()

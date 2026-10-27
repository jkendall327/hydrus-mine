#!/usr/bin/env python3
"""Record a search page's "selection tags" box: its clicks, keys and menu.

Fresh temporary basic client. 'my tags' gets a small corpus over three files
(a parent: alpha:first has group:alpha; a sibling: old:sibling shows as
new:sibling), synced. A real search page (my files, searching box:anchor) is
opened; its sidebar's real `ListBoxTagsMediaSidebar` lists the three files'
tags. It is driven with real Qt mouse presses and key presses on the list
widget; after each step the selected tags are recorded:

- left, ctrl, shift and ctrl+shift clicks, a click below the rows, a right
  press on an unselected row;
- the keys (arrows, home/end, page up/down, ctrl+n/p, escape, ctrl+a, ctrl+c);
- middle and shift+middle clicks (the new search pages they open, captured
  rather than opened);
- double clicks (plain, ctrl, shift with two selected) and enter: what they
  do to the page's search, which is then put back;
- the right-click menu as `ShowMenuFromSignal` builds it once its sibling and
  parent lookup has come back, for one namespaced, one unnamespaced, one
  searched-for, two of a namespace, two of different namespaces, two
  unnamespaced and all tags, for none, and in advanced mode; every copy entry's
  payload and every open entry's pages; every search entry's effect on the
  page's search (put back after each); select (the files then selected);
  the siblings/parents dialogs (their titles and tags); favourites;
  maintenance (its question and write); and last the hide entries (their
  questions and the list after).

Usage: QT_QPA_PLATFORM=offscreen ~/pyenv/bin/python oracle/record_selection_tags_box.py
       (writes fixtures/selection_tags_box.json)
"""
import json
import os
import sys
import tempfile
import time

HERE = os.path.dirname( os.path.abspath( __file__ ) )

sys.path.insert( 0, HERE )

OUT = os.path.join( HERE, 'fixtures', 'selection_tags_box.json' )

# tag, the indices of the files that have it
CORPUS = [
    ( 'box:anchor', [ 0, 1, 2 ] ),
    ( 'alpha:first', [ 0, 1 ] ),
    ( 'alpha:second', [ 2 ] ),
    ( 'series:spaced name', [ 0, 1 ] ),
    ( 'plain tag', [ 0 ] ),
    ( 'solo', [ 1, 2 ] ),
    ( 'old:sibling', [ 1 ] ),
    ( 'zeta:last', [ 2 ] )
]
PARENTS = [ [ 'alpha:first', 'group:alpha' ] ]
SIBLINGS = [ [ 'old:sibling', 'new:sibling' ] ]
SEARCH = 'box:anchor'


def record( session ):

    from qtpy import QtCore as QC
    from qtpy import QtWidgets as QW
    from qtpy.QtTest import QTest

    from hydrus.client import ClientConstants as CC
    from hydrus.client import ClientLocation
    from hydrus.client.gui import ClientGUIAsync
    from hydrus.client.gui import ClientGUICore as Core
    from hydrus.client.gui import ClientGUIDialogsQuick
    from hydrus.client.gui import ClientGUITopLevelWindowsPanels
    from hydrus.client.gui.metadata import ClientGUIManageTagParents, ClientGUIManageTagSiblings
    from hydrus.client.gui.search import ClientGUISearch
    from hydrus.client.media import ClientMediaFileFilter
    from hydrus.client.metadata import ClientContentUpdates as U
    from hydrus.client.search import ClientSearchFileSearchContext as F
    from hydrus.client.search import ClientSearchPredicate as P
    from hydrus.client.search import ClientSearchTagContext as T
    from hydrus.core import HydrusConstants as HC

    c = session.controller
    gui = c.gui
    qt = lambda fn: c.CallBlockingToQt( gui, fn )

    service = next( s.GetServiceKey() for s in c.services_manager.GetServices( ( HC.LOCAL_TAG, ) ) if s.GetName() == 'my tags' )

    manifest = json.load( open( os.path.join( HERE, 'fixtures', 'legacy_db', 'basic.manifest.json' ) ) )

    # (the second of the basic fixture's files is not in 'my files')
    hashes = [ bytes.fromhex( manifest[ 'files' ][ i ][ 'hash' ] ) for i in ( 0, 2, 3 ) ]

    updates = [ U.ContentUpdate( HC.CONTENT_TYPE_MAPPINGS, HC.CONTENT_UPDATE_ADD, ( tag, { hashes[ i ] for i in files } ) ) for ( tag, files ) in CORPUS ]
    updates.extend( U.ContentUpdate( HC.CONTENT_TYPE_TAG_SIBLINGS, HC.CONTENT_UPDATE_ADD, tuple( pair ) ) for pair in SIBLINGS )
    updates.extend( U.ContentUpdate( HC.CONTENT_TYPE_TAG_PARENTS, HC.CONTENT_UPDATE_ADD, tuple( pair ) ) for pair in PARENTS )

    c.WriteSynchronous( 'content_updates', U.ContentUpdatePackage.STATICCreateFromContentUpdates( service, updates ) )

    for _ in range( 20 ):

        if not c.WriteSynchronous( 'sync_tag_display_maintenance', service, .5 ):

            break



    c.new_options.SetStringList( 'favourite_tags', [] )
    c.new_options.SetBoolean( 'advanced_mode', False )

    location = ClientLocation.LocationContext.STATICCreateSimple( CC.LOCAL_FILE_SERVICE_KEY )
    context = F.FileSearchContext( location_context = location, tag_context = T.TagContext(), predicates = [ P.Predicate( P.PREDICATE_TYPE_TAG, SEARCH ) ] )

    page = qt( lambda: gui._notebook.NewPageQueryFileSearchContext( context, page_name = 'selection tags' ) )

    def ready():

        sidebar = page.GetSidebar()

        if sidebar is None or getattr( sidebar, '_current_selection_tags_list', None ) is None:

            return False


        return len( sidebar._current_selection_tags_list._ordered_terms ) > 0


    for _ in range( 1000 ):

        if qt( ready ):

            break


        time.sleep( .01 )


    time.sleep( .5 )

    sidebar = qt( page.GetSidebar )
    box = sidebar._current_selection_tags_list
    ac = sidebar._tag_autocomplete

    menus = []
    questions = []
    dialogs = []
    writes = []
    launched = []
    copied = []
    or_edits = []

    old = dict(
        popup = Core.core().PopupMenu,
        yes_no = ClientGUIDialogsQuick.GetYesNo,
        manage = ClientGUITopLevelWindowsPanels.DialogManage,
        siblings = ClientGUIManageTagSiblings.ManageTagSiblings,
        parents = ClientGUIManageTagParents.ManageTagParents,
        write = c.Write,
        pub = c.pub,
        start = ClientGUIAsync.AsyncQtJob.start,
        edit = ClientGUISearch.EditPredicates
    )

    Core.core().PopupMenu = lambda _widget, menu: menus.append( menu )

    def yes_no( win, message, title = 'Are you sure?', yes_label = 'yes', no_label = 'no', **kwargs ):

        questions.append( { 'message' : message, 'title' : title, 'yes' : yes_label, 'no' : no_label } )

        return QW.QDialog.DialogCode.Accepted


    ClientGUIDialogsQuick.GetYesNo = yes_no

    class Manage( QW.QDialog ):

        def __init__( self, parent, title, *args, **kwargs ):

            super().__init__( parent )

            self._title = title


        def __enter__( self ):

            return self


        def __exit__( self, *args ):

            self.deleteLater()

            return False


        def SetPanel( self, panel ):

            dialogs.append( { 'title' : self._title, 'tags' : sorted( panel ) } )


        def exec( self ):

            return QW.QDialog.DialogCode.Rejected



    ClientGUITopLevelWindowsPanels.DialogManage = Manage
    # (the panels are stood in for by the tags they are given)
    ClientGUIManageTagSiblings.ManageTagSiblings = lambda dlg, tags: list( tags )
    ClientGUIManageTagParents.ManageTagParents = lambda dlg, tags: list( tags )

    def write( action, *args, **kwargs ):

        if action == 'regenerate_tag_mappings_tags':

            writes.append( { 'action' : action, 'tags' : sorted( args[ 0 ] ) } )

            return


        return old[ 'write' ]( action, *args, **kwargs )


    c.Write = write

    def pub( topic, *args, **kwargs ):

        if topic == 'clipboard':

            copied.append( args[ 1 ] )

        elif topic in ( 'new_page_query', 'new_page_duplicates' ):

            launched.append( {
                'topic' : topic,
                'location' : args[ 0 ].ToString( c.services_manager.GetName ),
                'predicates' : sorted( p.ToString() for p in kwargs[ 'initial_predicates' ] ),
                'page_name' : kwargs.get( 'page_name' ),
                'activate_window' : kwargs.get( 'activate_window' )
            } )

        else:

            old[ 'pub' ]( topic, *args, **kwargs )



    c.pub = pub

    # (the sibling/parent lookup runs where we stand, so the menu is complete)
    ClientGUIAsync.AsyncQtJob.start = lambda job: job._publish_callable( job._work_callable() )

    def edit_or( win, predicates, **kwargs ):

        or_edits.append( sorted( p.ToString() for p in predicates ) )

        return predicates


    ClientGUISearch.EditPredicates = edit_or

    emitted = []

    old_emit = box._EmitPredicates

    def emit( predicates, **kwargs ):

        emitted.append( dict( predicates = sorted( p.ToString() for p in predicates ), **kwargs ) )

        old_emit( predicates, **kwargs )


    box._EmitPredicates = emit

    def settle( seconds = .05 ):

        end = time.time() + seconds

        while time.time() < end:

            QW.QApplication.processEvents()

            time.sleep( .005 )



    h = box.fontMetrics().height()

    def tags():

        return [ t.GetTag() for t in box._ordered_terms ]


    def drawn():

        return [ ''.join( text for ( text, colour ) in box._GetRowsOfTextsAndColours( t )[ 0 ] ) for t in box._ordered_terms ]


    def selected():

        return [ t.GetTag() for t in box._ordered_terms if t in box._selected_terms ]


    def searched():

        return sorted( p.ToString() for p in ac.GetPredicates() )


    original_tags = None
    original_search = None

    def wait_for( condition ):

        for _ in range( 400 ):

            settle( .01 )

            if condition():

                return True



        return False


    def restore():

        # put the search back as it was, then wait for the list to be refilled
        current = list( ac.GetPredicates() )
        wanted = [ P.Predicate( P.PREDICATE_TYPE_TAG, SEARCH ) ]

        extra = [ p for p in current if p not in wanted ]
        missing = [ p for p in wanted if p not in current ]

        if len( extra ) > 0:

            ac._predicates_listbox.EnterPredicates( extra, permit_add = False )


        if len( missing ) > 0:

            ac._predicates_listbox.EnterPredicates( missing, permit_remove = False )


        wait_for( lambda: tags() == original_tags )

        settle( .2 )


    def at( tag ):

        term = next( ( t for t in box._ordered_terms if t.GetTag() == tag ), None )
        
        if term is None:
            
            raise Exception( f'{tag!r} is not listed: {tags()}' )
            
        

        return QC.QPoint( 10, box._terms_to_positional_indices[ term ] * h + h // 2 )


    def below():

        return QC.QPoint( 10, ( box._total_positional_rows + 2 ) * h )


    NONE = QC.Qt.KeyboardModifier.NoModifier
    CTRL = QC.Qt.KeyboardModifier.ControlModifier
    SHIFT = QC.Qt.KeyboardModifier.ShiftModifier
    LEFT = QC.Qt.MouseButton.LeftButton

    def click( tag, button = LEFT, mods = NONE ):

        QTest.mouseClick( box.widget(), button, mods, at( tag ) if tag is not None else below() )

        settle()


    def dclick( tag, mods = NONE ):

        # as a widget gets a real double-click: press, release, double-click,
        # release (QTest.mouseDClick lost some, sent straight after a click)
        from qtpy import QtGui as QG

        point = QC.QPointF( at( tag ) )
        global_point = QC.QPointF( box.widget().mapToGlobal( at( tag ) ) )

        for ( kind, buttons ) in [
            ( QC.QEvent.Type.MouseButtonPress, LEFT ),
            ( QC.QEvent.Type.MouseButtonRelease, QC.Qt.MouseButton.NoButton ),
            ( QC.QEvent.Type.MouseButtonDblClick, LEFT ),
            ( QC.QEvent.Type.MouseButtonRelease, QC.Qt.MouseButton.NoButton )
        ]:

            QW.QApplication.sendEvent( box.widget(), QG.QMouseEvent( kind, point, global_point, LEFT, buttons, mods ) )


        settle()


    def key( k, mods = NONE ):

        box.setFocus()

        QTest.keyClick( box, k, mods )

        settle()


    def select( *wanted ):

        key( QC.Qt.Key.Key_Escape )

        for ( i, tag ) in enumerate( wanted ):

            click( tag, mods = CTRL )



    def tree( menu ):

        out = []

        for action in menu.actions():

            if action.isSeparator():

                out.append( '---' )

            elif action.menu() is not None:

                out.append( { action.menu().title().replace( '&&', '&' ) or action.text().replace( '&&', '&' ) : tree( action.menu() ) } )

            else:

                text = action.text().replace( '&&', '&' )

                if not action.isEnabled():

                    out.append( [ 'label', text ] )

                elif action.isCheckable():

                    out.append( [ 'check', text, action.isChecked() ] )

                else:

                    out.append( text )




        return out


    def find( menu, path ):

        for action in menu.actions():

            text = ( action.menu().title() if action.menu() is not None else action.text() ).replace( '&&', '&' )

            if text == path[ 0 ]:

                return action if len( path ) == 1 else find( action.menu(), path[ 1: ] )



        return None


    def right_click( tag ):

        point = at( tag ) if tag is not None else below()

        QTest.mouseClick( box.widget(), QC.Qt.MouseButton.RightButton, NONE, point )

        menus.clear()

        box.ShowMenuFromSignal( point )

        settle()

        return menus[ -1 ] if len( menus ) > 0 else None


    def leaves( menu, path = () ):

        for action in menu.actions():

            if action.isSeparator():

                continue


            if action.menu() is not None:

                yield from leaves( action.menu(), path + ( action.menu().title().replace( '&&', '&' ), ) )

            else:

                yield ( path + ( action.text().replace( '&&', '&' ), ), action )




    def work():

        nonlocal original_tags, original_search

        original_tags = tags()
        original_search = searched()

        out = {
            'corpus' : [ { 'tag' : tag, 'hashes' : [ hashes[ i ].hex() for i in files ] } for ( tag, files ) in CORPUS ],
            'parents' : PARENTS,
            'siblings' : SIBLINGS,
            'search' : SEARCH,
            'location' : location.ToString( c.services_manager.GetName ),
            'tags' : tags(),
            'drawn' : drawn(),
            'searched' : original_search,
            'rows_per_page' : box._num_rows_per_page,
            'activate_window_on_tag_search_page_activation' : c.new_options.GetBoolean( 'activate_window_on_tag_search_page_activation' )
        }

        steps = []

        def step( name, **extra ):

            settle()

            steps.append( dict( step = name, selected = selected(), **extra ) )


        # clicks

        step( 'initial' )
        click( 'series:spaced name' ); step( 'click series:spaced name' )
        click( 'solo', mods = CTRL ); step( 'ctrl click solo adds it' )
        click( 'series:spaced name', mods = CTRL ); step( 'ctrl click series:spaced name takes it away' )
        click( 'zeta:last', mods = SHIFT ); step( 'shift click zeta:last' )
        click( 'alpha:first', mods = CTRL | SHIFT ); step( 'ctrl shift click alpha:first' )
        click( 'zeta:last' ); step( 'plain click on a selected row keeps the selection' )
        click( 'plain tag' ); step( 'plain click on an unselected row selects only it' )
        click( 'box:anchor', button = QC.Qt.MouseButton.RightButton ); step( 'right press on an unselected row selects only it' )
        click( 'plain tag', mods = CTRL ); click( 'plain tag', button = QC.Qt.MouseButton.RightButton ); step( 'right press on a selected row keeps the selection' )
        click( None ); step( 'click below the rows' )
        click( 'solo', mods = SHIFT ); step( 'shift click with nothing before' )
        click( None, mods = SHIFT ); step( 'shift click below the rows selects to the end' )

        # keys

        click( 'box:anchor' ); step( 'click box:anchor' )
        key( QC.Qt.Key.Key_Down ); step( 'down' )
        key( QC.Qt.Key.Key_Down, SHIFT ); step( 'shift down' )
        key( QC.Qt.Key.Key_Down, CTRL ); step( 'ctrl down' )
        key( QC.Qt.Key.Key_Up ); step( 'up' )
        key( QC.Qt.Key.Key_Home ); step( 'home' )
        key( QC.Qt.Key.Key_Up ); step( 'up wraps to the end' )
        key( QC.Qt.Key.Key_Down ); step( 'down wraps to the start' )
        key( QC.Qt.Key.Key_End ); step( 'end' )
        key( QC.Qt.Key.Key_N, CTRL ); step( 'ctrl n' )
        key( QC.Qt.Key.Key_P, CTRL ); step( 'ctrl p' )
        key( QC.Qt.Key.Key_PageUp ); step( 'page up' )
        key( QC.Qt.Key.Key_PageDown ); step( 'page down' )
        key( QC.Qt.Key.Key_Escape ); step( 'escape' )
        n = len( copied )
        key( QC.Qt.Key.Key_C, CTRL ); step( 'ctrl c with none selected copies all', copied = copied[ n: ] )
        key( QC.Qt.Key.Key_A, CTRL ); step( 'ctrl a' )
        select( 'alpha:first', 'plain tag' )
        n = len( copied )
        key( QC.Qt.Key.Key_C, CTRL ); step( 'ctrl c copies the selection', copied = copied[ n: ] )
        n = len( copied )
        key( QC.Qt.Key.Key_C, CTRL | SHIFT ); step( 'ctrl shift c', copied = copied[ n: ] )
        key( QC.Qt.Key.Key_Delete ); step( 'delete does nothing', searched = searched() )

        # middle clicks

        key( QC.Qt.Key.Key_Escape )
        n = len( launched )
        click( 'series:spaced name', button = QC.Qt.MouseButton.MiddleButton ); step( 'middle click series:spaced name', launched = launched[ n: ] )
        select( 'plain tag', 'solo' )
        n = len( launched )
        click( 'solo', button = QC.Qt.MouseButton.MiddleButton ); step( 'middle click on a selected row with two selected', launched = launched[ n: ] )
        n = len( launched )
        click( 'plain tag', button = QC.Qt.MouseButton.MiddleButton, mods = SHIFT ); step( 'shift middle click with two selected', launched = launched[ n: ] )
        key( QC.Qt.Key.Key_Escape )
        n = len( launched )
        click( 'zeta:last', button = QC.Qt.MouseButton.MiddleButton, mods = SHIFT ); step( 'shift middle click with one selected', launched = launched[ n: ] )
        n = len( launched )
        click( None, button = QC.Qt.MouseButton.MiddleButton ); step( 'middle click below the rows', launched = launched[ n: ] )

        # activation, which changes the search (put back after each)

        def activated( name, action ):

            n = len( emitted )

            action()

            settle( .1 )

            step( name, searched = searched(), emitted = emitted[ n: ] )

            restore()


        key( QC.Qt.Key.Key_Escape )
        activated( 'double click series:spaced name', lambda: dclick( 'series:spaced name' ) )
        key( QC.Qt.Key.Key_Escape )
        activated( 'ctrl double click zeta:last', lambda: dclick( 'zeta:last', CTRL ) )
        select( 'plain tag', 'solo' )
        activated( 'ctrl double click on a selected row', lambda: dclick( 'plain tag', CTRL ) )
        select( 'plain tag', 'solo' )
        activated( 'shift double click with two selected', lambda: dclick( 'solo', SHIFT ) )
        select( 'box:anchor' )
        activated( 'double click on the searched tag', lambda: dclick( 'box:anchor' ) )
        select( 'alpha:first' )
        activated( 'return', lambda: key( QC.Qt.Key.Key_Return ) )
        select( 'alpha:first', 'solo' )
        activated( 'ctrl return with two selected', lambda: key( QC.Qt.Key.Key_Return, CTRL ) )

        out[ 'steps' ] = steps

        # the menus

        cases = [
            ( 'one namespaced', [ 'series:spaced name' ] ),
            ( 'one unnamespaced', [ 'plain tag' ] ),
            ( 'the searched tag', [ 'box:anchor' ] ),
            ( 'a tag with a parent', [ 'alpha:first' ] ),
            ( 'a sibling', [ 'new:sibling' ] ),
            ( 'two of a namespace', [ 'alpha:first', 'alpha:second' ] ),
            ( 'two namespaces', [ 'alpha:first', 'series:spaced name' ] ),
            ( 'two unnamespaced', [ 'plain tag', 'solo' ] ),
            ( 'the searched tag and another', [ 'box:anchor', 'solo' ] ),
            ( 'all', None ),
            ( 'none', [] )
        ]

        def show( wanted ):

            if wanted is None:

                key( QC.Qt.Key.Key_A, CTRL )

                return right_click( 'zeta:last' )


            select( *wanted )

            return right_click( wanted[ -1 ] if len( wanted ) > 0 else None )


        menus_out = []

        for ( name, wanted ) in cases:

            menu = show( wanted )

            entry = { 'case' : name, 'selected' : selected(), 'menu' : tree( menu ) if menu is not None else None, 'copies' : [], 'launches' : [] }

            if menu is not None:

                for ( path, action ) in leaves( menu ):

                    if path[ 0 ] == 'copy':

                        n = len( copied )
                        action.trigger()
                        entry[ 'copies' ].append( { 'label' : path[ 1 ], 'payload' : copied[ n: ] } )

                    elif path[ 0 ] == 'open':

                        n = len( launched )
                        action.trigger()
                        entry[ 'launches' ].append( { 'label' : path[ 1 ], 'launched' : launched[ n: ] } )




            menus_out.append( entry )


        out[ 'menus' ] = menus_out

        # advanced mode adds the tag display switch

        c.new_options.SetBoolean( 'advanced_mode', True )
        menu = show( [ 'plain tag' ] )
        out[ 'advanced' ] = { 'selected' : selected(), 'menu' : tree( menu ) }

        # what each switch shows (then back to the selection list's tags)
        switches = []

        for label in [ 'switch to "single media view tags"', 'switch to "display tags"', 'switch to "stored tags"', 'switch to "multiple media view tags"' ]:

            menu = show( [ 'plain tag' ] )

            find( menu, [ 'tag display (experimental)', label ] ).trigger()

            settle( .3 )

            switches.append( { 'label' : label, 'drawn' : drawn() } )


        out[ 'advanced' ][ 'switches' ] = switches
        c.new_options.SetBoolean( 'advanced_mode', False )

        # the search entries, each from a fresh menu, the search put back after

        searches = []

        for ( name, wanted ) in cases:

            if wanted is None or len( wanted ) == 0:

                continue


            menu = show( wanted )

            search = find( menu, [ 'search' ] )

            labels = [ a.text().replace( '&&', '&' ) for a in search.menu().actions() if not a.isSeparator() ] if search is not None else []

            for label in labels:

                menu = show( wanted )

                n = len( or_edits )
                e = len( emitted )

                find( menu, [ 'search', label ] ).trigger()

                settle( .1 )

                searches.append( { 'case' : name, 'selected' : wanted, 'label' : label, 'searched' : searched(), 'or_editor' : or_edits[ n: ], 'emitted' : emitted[ e: ] } )

                restore()



        out[ 'searches' ] = searches

        # select files with the tags

        media_panel = page.GetMediaResultsPanel()

        def selected_files():

            return sorted( h.hex() for h in media_panel._GetSelectedHashes() )


        selects = []

        for ( wanted, label ) in [
            ( [ 'alpha:first' ], 'files with "alpha:first"' ),
            ( [ 'alpha:first', 'solo' ], 'files with all of "alpha:first, solo"' ),
            ( [ 'alpha:first', 'solo' ], 'files with any of "alpha:first, solo"' )
        ]:

            menu = show( wanted )

            find( menu, [ 'select', label ] ).trigger()

            settle( .3 )

            selects.append( { 'selected' : wanted, 'label' : label, 'files' : selected_files(), 'tags_after' : tags() } )

            media_panel._Select( ClientMediaFileFilter.FileFilter( ClientMediaFileFilter.FILE_FILTER_NONE ) )

            wait_for( lambda: tags() == original_tags )


        out[ 'selects' ] = selects

        # the entries it shares with the other tag lists

        triggers = []

        for ( wanted, path ) in [
            ( [ 'alpha:first' ], [ '1 parents, 0 children', 'add parents to alpha:first' ] ),
            ( [ 'alpha:first' ], [ 'no siblings', 'add siblings to alpha:first' ] ),
            ( [ 'plain tag', 'solo' ], [ 'parents', 'add parents to selection' ] ),
            ( [ 'plain tag', 'solo' ], [ 'siblings', 'add siblings to selection' ] ),
            ( [ 'new:sibling' ], [ '1 siblings', 'old:sibling' ] ),
            ( [ 'plain tag' ], [ 'favourites', 'add "plain tag" to favourites' ] ),
            ( [ 'plain tag' ], [ 'favourites', 'remove "plain tag" from favourites' ] ),
            ( [ 'plain tag', 'solo' ], [ 'maintenance', 'regenerate tag display' ] )
        ]:

            menu = show( wanted )

            del questions[:]
            del dialogs[:]
            del writes[:]

            n = len( copied )

            action = find( menu, path )

            if action is not None:

                action.trigger()


            settle()

            triggers.append( {
                'selected' : wanted,
                'path' : path,
                'found' : action is not None,
                'questions' : list( questions ),
                'dialogs' : list( dialogs ),
                'writes' : list( writes ),
                'copied' : copied[ n: ],
                'favourite_tags' : list( c.new_options.GetStringList( 'favourite_tags' ) )
            } )


        out[ 'triggers' ] = triggers

        # last, hiding (which changes the tag display rules for good)

        hides = []

        for ( wanted, path ) in [
            ( [ 'plain tag' ], [ 'hide', '"plain tag" from here' ] ),
            ( [ 'alpha:first', 'solo' ], [ 'hide', '2 selected namespaces from here' ] ),
            ( [ 'series:spaced name' ], [ 'hide', '"series" tags from here' ] )
        ]:

            menu = show( wanted )

            hide_menu = tree( find( menu, [ 'hide' ] ).menu() )

            del questions[:]

            find( menu, path ).trigger()

            wait_for( lambda: tags() != original_tags )

            settle( .3 )

            hides.append( { 'selected' : wanted, 'menu' : hide_menu, 'path' : path, 'questions' : list( questions ), 'tags_after' : tags() } )

            original_tags = tags()


        out[ 'hides' ] = hides

        return out


    try:

        return qt( work )

    finally:

        Core.core().PopupMenu = old[ 'popup' ]
        ClientGUIDialogsQuick.GetYesNo = old[ 'yes_no' ]
        ClientGUITopLevelWindowsPanels.DialogManage = old[ 'manage' ]
        ClientGUIManageTagSiblings.ManageTagSiblings = old[ 'siblings' ]
        ClientGUIManageTagParents.ManageTagParents = old[ 'parents' ]
        c.Write = old[ 'write' ]
        c.pub = old[ 'pub' ]
        ClientGUIAsync.AsyncQtJob.start = old[ 'start' ]
        ClientGUISearch.EditPredicates = old[ 'edit' ]



def main():

    import hydrus_driver

    if len( sys.argv ) > 1 and sys.argv[ 1 ] == '--child':

        import record_api

        out = sys.argv[ 2 ]

        result = hydrus_driver.run_client( record_api.unpack_fixture( 'basic' ), record )

        with open( out, 'w' ) as f:

            json.dump( result, f )


        return


    with tempfile.TemporaryDirectory() as folder:

        path = os.path.join( folder, 'result.json' )

        hydrus_driver.run_in_subprocess( os.path.abspath( __file__ ), '--child', path )

        with open( path ) as f:

            result = json.load( f )



    with open( OUT, 'w' ) as f:

        json.dump( result, f, indent = 1, ensure_ascii = False )
        f.write( '\n' )


    print( 'wrote', OUT )


if __name__ == '__main__':

    main()

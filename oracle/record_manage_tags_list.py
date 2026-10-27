#!/usr/bin/env python3
"""Record the manage tags dialog's tag list: its clicks, keys,
middle-clicks, double-clicks and right-click menu.

Fresh temporary basic client; 'my tags' gets the selection tags box's corpus
(oracle/record_selection_tags_box.py: alpha:first has the parent group:alpha,
old:sibling shows as new:sibling), synced. A real ManageTagsPanel (as from a
search page, my files) is opened on the corpus's three files; its 'my tags'
page's real `ListBoxTagsMediaTagsDialog` (stored tags, with counts and
sibling/parent decorators) is driven with real Qt mouse and key events on
the list widget. Its enter and delete callables (the panel's entering and
removing, recorded elsewhere) are replaced by recorders of the tags they are
given. After each step the selected tags are recorded; middle-clicks record
the search pages they open (captured, not opened); the right-click menu is
recorded, as `ShowMenuFromSignal` builds it once its sibling and parent
lookup has come back, for one, several and no tags, with every copy entry's
payload and every open entry's pages, and its decorator, siblings/parents,
favourites, most used and maintenance entries are triggered.

Usage: QT_QPA_PLATFORM=offscreen ~/pyenv/bin/python oracle/record_manage_tags_list.py
       (writes fixtures/manage_tags_list.json)
"""
import json
import os
import sys
import tempfile
import time

HERE = os.path.dirname( os.path.abspath( __file__ ) )

sys.path.insert( 0, HERE )

import record_selection_tags_box as box_recorder

OUT = os.path.join( HERE, 'fixtures', 'manage_tags_list.json' )


def record( session ):

    from qtpy import QtCore as QC
    from qtpy import QtGui as QG
    from qtpy import QtWidgets as QW
    from qtpy.QtTest import QTest

    from hydrus.client import ClientConstants as CC
    from hydrus.client import ClientLocation
    from hydrus.client.gui import ClientGUIAsync
    from hydrus.client.gui import ClientGUICore as Core
    from hydrus.client.gui import ClientGUIDialogsQuick
    from hydrus.client.gui import ClientGUITopLevelWindowsPanels
    from hydrus.client.gui.metadata import ClientGUIManageTagParents, ClientGUIManageTagSiblings
    from hydrus.client.gui.metadata.ClientGUIManageTags import ManageTagsPanel
    from hydrus.client.media import ClientMediaSingle
    from hydrus.client.metadata import ClientContentUpdates as U
    from hydrus.core import HydrusConstants as HC

    c = session.controller

    service = next( s.GetServiceKey() for s in c.services_manager.GetServices( ( HC.LOCAL_TAG, ) ) if s.GetName() == 'my tags' )

    manifest = json.load( open( os.path.join( HERE, 'fixtures', 'legacy_db', 'basic.manifest.json' ) ) )

    # (the second of the basic fixture's files is not in 'my files')
    hashes = [ bytes.fromhex( manifest[ 'files' ][ i ][ 'hash' ] ) for i in ( 0, 2, 3 ) ]

    updates = [ U.ContentUpdate( HC.CONTENT_TYPE_MAPPINGS, HC.CONTENT_UPDATE_ADD, ( tag, { hashes[ i ] for i in files } ) ) for ( tag, files ) in box_recorder.CORPUS ]
    updates.extend( U.ContentUpdate( HC.CONTENT_TYPE_TAG_SIBLINGS, HC.CONTENT_UPDATE_ADD, tuple( pair ) ) for pair in box_recorder.SIBLINGS )
    updates.extend( U.ContentUpdate( HC.CONTENT_TYPE_TAG_PARENTS, HC.CONTENT_UPDATE_ADD, tuple( pair ) ) for pair in box_recorder.PARENTS )

    c.WriteSynchronous( 'content_updates', U.ContentUpdatePackage.STATICCreateFromContentUpdates( service, updates ) )

    for _ in range( 20 ):

        if not c.WriteSynchronous( 'sync_tag_display_maintenance', service, .5 ):

            break



    c.new_options.SetStringList( 'favourite_tags', [] )
    c.new_options.SetSuggestedTagsMostUsed( service, [] )
    c.new_options.SetBoolean( 'advanced_mode', False )

    location = ClientLocation.LocationContext.STATICCreateSimple( CC.LOCAL_FILE_SERVICE_KEY )

    def work():

        medias = [ ClientMediaSingle.MediaSingle( r ) for r in c.Read( 'media_results', hashes ) ]

        panel = ManageTagsPanel( c.gui, location, CC.TAG_PRESENTATION_SEARCH_PAGE_MANAGE_TAGS, medias )
        panel.show()
        panel.resize( 900, 700 )

        page = next( p for p in panel._tag_services.GetPages() if p.GetServiceKey() == service )
        panel._tag_services.setCurrentWidget( page )

        box = page._tags_box

        entered = []
        deleted = []

        box._enter_func = lambda tags: entered.append( sorted( tags ) )
        box._delete_func = lambda tags: deleted.append( sorted( tags ) )

        menus = []
        questions = []
        launched = []
        copied = []

        old_popup, old_yes_no, old_pub, old_start = Core.core().PopupMenu, ClientGUIDialogsQuick.GetYesNo, c.pub, ClientGUIAsync.AsyncQtJob.start

        Core.core().PopupMenu = lambda _widget, menu: menus.append( menu )

        def yes_no( win, message, title = 'Are you sure?', yes_label = 'yes', no_label = 'no', **kwargs ):

            questions.append( { 'message' : message, 'title' : title, 'yes' : yes_label, 'no' : no_label } )

            return QW.QDialog.DialogCode.Accepted


        ClientGUIDialogsQuick.GetYesNo = yes_no

        def pub( topic, *args, **kwargs ):

            if topic == 'clipboard':

                copied.append( args[ 1 ] )

            elif topic in ( 'new_page_query', 'new_page_duplicates' ):

                launched.append( {
                    'topic' : topic,
                    'location' : args[ 0 ].ToString( c.services_manager.GetName ),
                    'predicates' : sorted( p.ToString() for p in kwargs[ 'initial_predicates' ] ),
                    'page_name' : kwargs.get( 'page_name' )
                } )

            else:

                old_pub( topic, *args, **kwargs )



        c.pub = pub

        ClientGUIAsync.AsyncQtJob.start = lambda job: job._publish_callable( job._work_callable() )

        dialogs = []
        writes = []

        old_manage, old_siblings, old_parents, old_write = ClientGUITopLevelWindowsPanels.DialogManage, ClientGUIManageTagSiblings.ManageTagSiblings, ClientGUIManageTagParents.ManageTagParents, c.Write

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


            return old_write( action, *args, **kwargs )


        c.Write = write

        def settle( seconds = .05 ):

            end = time.time() + seconds

            while time.time() < end:

                QW.QApplication.processEvents()

                time.sleep( .005 )



        settle( .3 )

        h = box.fontMetrics().height()

        tags = lambda: [ t.GetTag() for t in box._ordered_terms ]
        # (every row drawn: a tag's, then its parents' when they are expanded)
        drawn = lambda: [ ''.join( text for ( text, colour ) in row ) for t in box._ordered_terms for row in box._GetRowsOfTextsAndColours( t ) ]
        selected = lambda: [ t.GetTag() for t in box._ordered_terms if t in box._selected_terms ]

        def at( tag ):

            if tag is None:

                return QC.QPoint( 10, ( box._total_positional_rows + 2 ) * h )


            term = next( t for t in box._ordered_terms if t.GetTag() == tag )

            return QC.QPoint( 10, box._terms_to_positional_indices[ term ] * h + h // 2 )


        NONE = QC.Qt.KeyboardModifier.NoModifier
        CTRL = QC.Qt.KeyboardModifier.ControlModifier
        SHIFT = QC.Qt.KeyboardModifier.ShiftModifier
        LEFT = QC.Qt.MouseButton.LeftButton

        def click( tag, button = LEFT, mods = NONE ):

            QTest.mouseClick( box.widget(), button, mods, at( tag ) )

            settle()


        def dclick( tag, mods = NONE ):

            # as a widget gets a real double-click: press, release, double-click, release
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


        def select( *wanted ):

            click( None )

            for tag in wanted:

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


        def leaves( menu, path = () ):

            for action in menu.actions():

                if action.isSeparator():

                    continue


                if action.menu() is not None:

                    yield from leaves( action.menu(), path + ( action.menu().title().replace( '&&', '&' ), ) )

                else:

                    yield ( path + ( action.text().replace( '&&', '&' ), ), action )




        def show( wanted ):

            select( *wanted )

            point = at( wanted[ -1 ] if len( wanted ) > 0 else None )

            QTest.mouseClick( box.widget(), QC.Qt.MouseButton.RightButton, NONE, point )

            menus.clear()

            box.ShowMenuFromSignal( point )

            settle()

            return menus[ -1 ] if len( menus ) > 0 else None


        out = {
            'options' : {
                'show_parent_decorators' : c.new_options.GetBoolean( 'show_parent_decorators_on_storage_taglists' ),
                'show_sibling_decorators' : c.new_options.GetBoolean( 'show_sibling_decorators_on_storage_taglists' ),
                'expand_parents' : c.new_options.GetBoolean( 'expand_parents_on_storage_taglists' )
            },
            'file' : hashes[ 0 ].hex(),
            'files' : [ x.hex() for x in hashes ],
            'location' : location.ToString( c.services_manager.GetName ),
            'tags' : tags(),
            'drawn' : drawn()
        }

        steps = []

        def step( name, **extra ):

            settle()

            steps.append( dict( step = name, selected = selected(), **extra ) )


        try:

            step( 'initial' )
            click( 'series:spaced name' ); step( 'click series:spaced name' )
            click( 'plain tag', mods = CTRL ); step( 'ctrl click plain tag adds it' )
            click( 'box:anchor', mods = SHIFT ); step( 'shift click box:anchor' )
            click( 'series:spaced name', mods = CTRL ); step( 'ctrl click series:spaced name takes it away' )
            click( 'box:anchor' ); step( 'plain click on a selected row keeps the selection' )
            click( 'solo', button = QC.Qt.MouseButton.RightButton ); step( 'right press on a selected row keeps the selection' )
            click( None ); step( 'click below the rows' )

            # middle clicks

            n = len( launched )
            click( 'series:spaced name', button = QC.Qt.MouseButton.MiddleButton ); step( 'middle click series:spaced name', launched = launched[ n: ] )
            select( 'plain tag', 'box:anchor' )
            n = len( launched )
            click( 'plain tag', button = QC.Qt.MouseButton.MiddleButton, mods = SHIFT ); step( 'shift middle click with two selected', launched = launched[ n: ] )

            # double clicks enter the selected tags (the panel's own entering
            # is recorded elsewhere)

            select( 'plain tag', 'box:anchor' )
            n = len( entered )
            dclick( 'box:anchor' ); step( 'double click with two selected', entered = entered[ n: ] )
            n = len( entered )
            click( None )
            dclick( None ); step( 'double click below the rows', entered = entered[ n: ] )

            # keys, with the list holding the keyboard

            def key( k, mods = NONE ):

                box.setFocus()

                QTest.keyClick( box, k, mods )

                settle()


            click( 'plain tag' ); step( 'click plain tag' )
            key( QC.Qt.Key.Key_Down ); step( 'down' )
            key( QC.Qt.Key.Key_Down, SHIFT ); step( 'shift down' )
            key( QC.Qt.Key.Key_Home ); step( 'home' )
            key( QC.Qt.Key.Key_Up ); step( 'up wraps to the end' )
            key( QC.Qt.Key.Key_End ); step( 'end' )
            key( QC.Qt.Key.Key_A, CTRL ); step( 'ctrl a' )
            key( QC.Qt.Key.Key_Escape ); step( 'escape' )
            n = len( copied )
            key( QC.Qt.Key.Key_C, CTRL ); step( 'ctrl c with none selected copies all', copied = copied[ n: ] )
            select( 'alpha:first', 'safe' )
            n = len( copied )
            key( QC.Qt.Key.Key_C, CTRL ); step( 'ctrl c copies the selection', copied = copied[ n: ] )
            n = len( copied )
            key( QC.Qt.Key.Key_C, CTRL | SHIFT ); step( 'ctrl shift c copies the selection with parents', copied = copied[ n: ] )
            n = len( entered )
            key( QC.Qt.Key.Key_Return ); step( 'return enters the selection', entered = entered[ n: ] )
            n = len( deleted )
            key( QC.Qt.Key.Key_Delete ); step( 'delete removes the selection', deleted = deleted[ n: ] )

            out[ 'steps' ] = steps

            # the menus

            menus_out = []

            for ( name, wanted ) in [
                ( 'one', [ 'series:spaced name' ] ),
                ( 'one unnamespaced', [ 'plain tag' ] ),
                ( 'two', [ 'alpha:first', 'box:anchor' ] ),
                ( 'a tag with a parent', [ 'alpha:first' ] ),
                ( 'a sibling', [ 'old:sibling' ] ),
                ( 'none', [] )
            ]:

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

            # the decorator entries change what is drawn

            decorators = []

            for label in [ 'collapse parent rows', 'expand parent rows', 'hide parent decorators', 'show parent decorators', 'hide sibling decorators', 'show sibling decorators' ]:

                menu = show( [ 'alpha:first' ] )

                offered = [ a.text() for a in menu.actions() if not a.isSeparator() ][ :3 ]

                action = find( menu, [ label ] )

                if action is not None:

                    action.trigger()

                    settle()


                decorators.append( { 'label' : label, 'offered' : offered, 'chosen' : action is not None, 'drawn' : drawn() } )


            out[ 'decorators' ] = decorators

            # advanced mode adds the tag display switch

            c.new_options.SetBoolean( 'advanced_mode', True )
            out[ 'advanced' ] = { 'selected' : [ 'plain tag' ], 'menu' : tree( show( [ 'plain tag' ] ) ) }
            c.new_options.SetBoolean( 'advanced_mode', False )

            # the entries it shares with the other tag lists

            triggers = []

            for ( wanted, path ) in [
                ( [ 'alpha:first' ], [ '1 parents, 0 children', 'add parents to alpha:first' ] ),
                ( [ 'alpha:first' ], [ 'no siblings', 'add siblings to alpha:first' ] ),
                ( [ 'plain tag', 'box:anchor' ], [ 'parents', 'add parents to selection' ] ),
                ( [ 'plain tag', 'box:anchor' ], [ 'siblings', 'add siblings to selection' ] ),
                ( [ 'alpha:first' ], [ '1 parents, 0 children', 'parent: group:alpha' ] ),
                ( [ 'plain tag' ], [ 'favourites', 'add "plain tag" to favourites' ] ),
                ( [ 'plain tag' ], [ 'favourites', 'remove "plain tag" from favourites' ] ),
                ( [ 'plain tag' ], [ 'favourites', 'add "plain tag" to most used for "my tags"' ] ),
                ( [ 'plain tag' ], [ 'favourites', 'remove "plain tag" from most used for "my tags"' ] ),
                ( [ 'plain tag', 'box:anchor' ], [ 'maintenance', 'regenerate tag display' ] )
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
                    'favourite_tags' : list( c.new_options.GetStringList( 'favourite_tags' ) ),
                    'most_used' : list( c.new_options.GetSuggestedTagsMostUsed( service ) )
                } )


            out[ 'triggers' ] = triggers

        finally:

            Core.core().PopupMenu, ClientGUIDialogsQuick.GetYesNo, c.pub = old_popup, old_yes_no, old_pub
            ClientGUITopLevelWindowsPanels.DialogManage, ClientGUIManageTagSiblings.ManageTagSiblings, ClientGUIManageTagParents.ManageTagParents, c.Write = old_manage, old_siblings, old_parents, old_write
            ClientGUIAsync.AsyncQtJob.start = old_start

            panel.hide()
            panel.deleteLater()


        return out


    return c.CallBlockingToQt( c.gui, work )


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

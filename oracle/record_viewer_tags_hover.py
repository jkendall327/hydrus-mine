#!/usr/bin/env python3
"""Record the media viewer's tags hover list: its clicks, middle-clicks,
double-clicks and right-click menu.

Fresh temporary basic client; 'my tags' gets the selection tags box's corpus
(oracle/record_selection_tags_box.py: alpha:first has the parent group:alpha,
old:sibling shows as new:sibling), synced. A real media viewer
(`CanvasMediaListBrowser` in a `CanvasFrame`, my files) is opened on the
first file; its tags hover's real `ListBoxTagsMediaHoverFrame` is driven
with real Qt mouse events on the list widget. After each step the selected
tags are recorded; middle-clicks record the search pages they open
(captured, not opened); double-clicks record the manage tags request they
make; the right-click menu is recorded, as `ShowMenuFromSignal` builds it
once its sibling and parent lookup has come back, for one, several and no
tags, with every copy entry's payload and every open entry's pages; then
hiding a tag (its question and the list after).

Usage: QT_QPA_PLATFORM=offscreen ~/pyenv/bin/python oracle/record_viewer_tags_hover.py
       (writes fixtures/viewer_tags_hover.json)
"""
import json
import os
import sys
import tempfile
import time

HERE = os.path.dirname( os.path.abspath( __file__ ) )

sys.path.insert( 0, HERE )

import record_selection_tags_box as box_recorder

OUT = os.path.join( HERE, 'fixtures', 'viewer_tags_hover.json' )


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
    from hydrus.client.gui.canvas import ClientGUICanvas, ClientGUICanvasFrame
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
    c.new_options.SetBoolean( 'advanced_mode', False )

    location = ClientLocation.LocationContext.STATICCreateSimple( CC.LOCAL_FILE_SERVICE_KEY )

    def work():

        media_results = c.Read( 'media_results', hashes )

        first = next( m for m in media_results if m.GetHash() == hashes[ 0 ] )

        frame = ClientGUICanvasFrame.CanvasFrame( c.gui )
        canvas = ClientGUICanvas.CanvasMediaListBrowser( frame, os.urandom( 32 ), location, media_results, hashes[ 0 ] )
        frame.SetCanvas( canvas )
        frame.showNormal()
        frame.resize( 1000, 750 )

        hover = canvas._tags_hover
        hover.show()
        hover.resize( 300, 600 )
        hover.layout().activate()

        box = hover._tags

        menus = []
        questions = []
        launched = []
        copied = []
        managed = []

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

            elif topic == 'canvas_manage_tags':

                managed.append( args[ 0 ] == canvas._canvas_key )

            else:

                old_pub( topic, *args, **kwargs )



        c.pub = pub

        ClientGUIAsync.AsyncQtJob.start = lambda job: job._publish_callable( job._work_callable() )

        def settle( seconds = .05 ):

            end = time.time() + seconds

            while time.time() < end:

                QW.QApplication.processEvents()

                time.sleep( .005 )



        settle( .3 )

        h = box.fontMetrics().height()

        tags = lambda: [ t.GetTag() for t in box._ordered_terms ]
        drawn = lambda: [ ''.join( text for ( text, colour ) in box._GetRowsOfTextsAndColours( t )[ 0 ] ) for t in box._ordered_terms ]
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

                    out.append( [ 'label', text ] if not action.isEnabled() else text )



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
            'file' : hashes[ 0 ].hex(),
            'files' : [ x.hex() for x in hashes ],
            'location' : location.ToString( c.services_manager.GetName ),
            'display_type' : c.new_options.GetInteger( 'tag_list_tag_display_type_media_viewer_hover' ),
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
            click( 'plain tag' ); step( 'plain click on a selected row keeps the selection' )
            click( 'alpha:first', button = QC.Qt.MouseButton.RightButton ); step( 'right press on an unselected row selects only it' )
            click( None ); step( 'click below the rows' )

            # middle clicks

            n = len( launched )
            click( 'series:spaced name', button = QC.Qt.MouseButton.MiddleButton ); step( 'middle click series:spaced name', launched = launched[ n: ] )
            select( 'plain tag', 'box:anchor' )
            n = len( launched )
            click( 'plain tag', button = QC.Qt.MouseButton.MiddleButton, mods = SHIFT ); step( 'shift middle click with two selected', launched = launched[ n: ] )

            # double clicks ask for the viewer's manage tags

            n = len( managed )
            dclick( 'alpha:first' ); step( 'double click alpha:first', managed = managed[ n: ] )
            n = len( managed )
            click( None )
            dclick( None ); step( 'double click below the rows', managed = managed[ n: ] )

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
            click( 'safe' )
            n = len( copied )
            key( QC.Qt.Key.Key_C, CTRL ); step( 'ctrl c copies the selection', copied = copied[ n: ] )
            n = len( managed )
            key( QC.Qt.Key.Key_Return ); step( 'return', managed = managed[ n: ] )

            out[ 'steps' ] = steps

            # the menus

            menus_out = []

            for ( name, wanted ) in [
                ( 'one', [ 'series:spaced name' ] ),
                ( 'one unnamespaced', [ 'plain tag' ] ),
                ( 'two', [ 'alpha:first', 'box:anchor' ] ),
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

            # hiding a tag from the hover's view of the file's tags

            menu = show( [ 'plain tag' ] )

            del questions[:]

            find( menu, [ 'hide', '"plain tag" from here' ] ).trigger()

            canvas.SetMedia( canvas._current_media )

            settle( .3 )

            out[ 'hide' ] = { 'selected' : [ 'plain tag' ], 'path' : [ 'hide', '"plain tag" from here' ], 'questions' : list( questions ), 'tags_after' : tags() }

        finally:

            Core.core().PopupMenu, ClientGUIDialogsQuick.GetYesNo, c.pub = old_popup, old_yes_no, old_pub
            ClientGUIAsync.AsyncQtJob.start = old_start

            frame.hide()
            frame.deleteLater()


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

#!/usr/bin/env python3
"""Record how the reference's ordinary dialogs keep the `regular_dialog` frame.

Every `DialogEdit`, `DialogManage` and `DialogNullipotent` that names no frame
of its own opens and keeps `regular_dialog`: `NewDialog._TryEndModal` saves it
(`DialogThatResizes._SaveOKPosition`, i.e. `SaveTLWSizeAndPosition`) when the
dialog is accepted (apply, its close button) and its panel is valid, before
the panel commits, and never when it is cancelled (cancel, escape, the
window's own close). A `FrameThatTakesScrollablePanel` on the same frame saves
as it closes, however it is closed.

Two parts, in the running client on the `basic` fixture:

- `lifecycle`: with the frame remembering size and place, each kind of window
  is opened, resized and moved, then ended one way; the frame before, the
  window's own geometry, the frame after, and whether the panel committed.
- `opening`: stale geometry fitted to the screen as the window opens
  (`GetSafePosition`, from `SetInitialTLWSizeAndPosition`): remembered places
  and default places relative to a main window put off the screen, with the
  place the window was given and the message the user was shown.

Usage: QT_QPA_PLATFORM=offscreen ~/pyenv/bin/python oracle/record_regular_dialog.py
       (writes fixtures/regular_dialog.json)
"""

import json
import os
import sys
import tempfile

HERE = os.path.dirname( os.path.abspath( __file__ ) )

sys.path.insert( 0, HERE )

OUT = os.path.join( HERE, 'fixtures', 'regular_dialog.json' )

KEY = 'regular_dialog'

BASELINE = ( True, True, ( 500, 400 ), ( 30, 40 ), ( -1, -1 ), 'topleft', False, False )

# where the window is put by the user before it is ended
MOVED_SIZE = ( 612, 478 )
MOVED_POSITION = ( 55, 65 )

MAIN = ( 120, 80, 600, 500 )

HINT = ( 300, 200 )

# ( name, frame, main window frame top-left ) for the openings
OPENINGS = [
    ( 'remembered_on_screen', ( True, True, ( 300, 200 ), ( 100, 120 ), ( -1, -1 ), 'topleft', False, False ), None ),
    ( 'remembered_off_right', ( True, True, ( 300, 200 ), ( 5000, 100 ), ( -1, -1 ), 'topleft', False, False ), None ),
    ( 'remembered_off_left_corner_visible', ( True, True, ( 300, 200 ), ( -250, 100 ), ( -1, -1 ), 'topleft', False, False ), None ),
    ( 'remembered_off_top', ( True, True, ( 300, 200 ), ( 100, -5000 ), ( -1, -1 ), 'topleft', False, False ), None ),
    ( 'remembered_position_only', ( False, True, None, ( 4000, 4000 ), ( -1, -1 ), 'topleft', False, False ), None ),
    ( 'topleft_parent_off_screen', ( False, False, None, None, ( -1, -1 ), 'topleft', False, False ), ( 3000, 3000 ) ),
    ( 'topleft_parent_near_corner', ( False, False, None, None, ( -1, -1 ), 'topleft', False, False ), ( 700, 700 ) ),
    ( 'center_parent_off_screen', ( False, False, None, None, ( -1, -1 ), 'center', False, False ), ( -3000, 200 ) ),
]


def record( session ):

    controller = session.controller
    gui = controller.gui

    from qtpy import QtCore as QC
    from qtpy import QtGui as QG
    from qtpy import QtWidgets as QW

    from hydrus.core import HydrusData
    from hydrus.core import HydrusExceptions
    from hydrus.client.gui import ClientGUITopLevelWindows
    from hydrus.client.gui import QtPorting as QP
    from hydrus.client.gui import ClientGUITopLevelWindowsPanels as W
    from hydrus.client.gui.panels import ClientGUIScrolledPanels

    shown = []

    original_show_text = HydrusData.ShowText

    def show_text( text, *args, **kwargs ):

        shown.append( str( text ) )

    class Panel( ClientGUIScrolledPanels.EditPanel ):

        def __init__( self, parent, veto = False ):

            super().__init__( parent )

            self.veto = veto
            self.committed = False

            filler = QW.QWidget( self )
            filler.setMinimumSize( HINT[0], HINT[1] )

            layout = QW.QVBoxLayout( self )
            layout.addWidget( filler )

        def CheckValid( self ):

            if self.veto:

                # (an empty message: the reference shows no warning for it,
                # which would wait on its own modal loop here)
                raise HydrusExceptions.VetoException( '' )

        def CommitChanges( self ):

            self.committed = True

        def GetValue( self ):

            return None

    class ReviewPanel( ClientGUIScrolledPanels.ReviewPanel ):

        def __init__( self, parent ):

            super().__init__( parent )

            filler = QW.QWidget( self )
            filler.setMinimumSize( HINT[0], HINT[1] )

            layout = QW.QVBoxLayout( self )
            layout.addWidget( filler )

    def settle( seconds = 0.0 ):

        import time

        end = time.monotonic() + seconds

        QW.QApplication.processEvents()

        while time.monotonic() < end:

            QW.QApplication.processEvents()
            time.sleep( 0.02 )

        QW.QApplication.processEvents()

    def geometry( window ):

        return { 'size' : [ window.width(), window.height() ], 'position' : [ window.frameGeometry().x(), window.frameGeometry().y() ] }

    def frame():

        return list( controller.new_options.GetFrameLocation( KEY ) )

    result = {}

    def work():

        HydrusData.ShowText = show_text
        ClientGUITopLevelWindows.HydrusData.ShowText = show_text

        original = controller.new_options.Duplicate()

        try:

            gui.showNormal()
            gui.resize( MAIN[2], MAIN[3] )
            gui.move( MAIN[0], MAIN[1] )

            settle()

            screens = []

            for screen in QW.QApplication.screens():

                g = screen.geometry()
                a = screen.availableGeometry()

                screens.append( { 'geometry' : [ g.x(), g.y(), g.width(), g.height() ], 'available' : [ a.x(), a.y(), a.width(), a.height() ] } )

            result[ 'screens' ] = screens
            result[ 'main' ] = [ gui.frameGeometry().x(), gui.frameGeometry().y(), gui.frameGeometry().width(), gui.frameGeometry().height() ]
            result[ 'hint' ] = list( HINT )
            result[ 'baseline' ] = list( BASELINE )
            result[ 'moved' ] = { 'size' : list( MOVED_SIZE ), 'position' : list( MOVED_POSITION ) }
            result[ 'rescue' ] = {
                'disabled' : controller.new_options.GetBoolean( 'disable_get_safe_position_test' ),
                'add_padding' : controller.new_options.GetBoolean( 'fuzzy_relocate_on_get_safe_position_test' ),
                'padding' : controller.new_options.GetInteger( 'forgive_frame_gubbins_fuzzy_padding' ),
            }

            # lifecycle

            cases = []

            dialog_actions = [
                ( 'edit', 'apply' ),
                ( 'edit', 'cancel' ),
                ( 'edit', 'escape' ),
                ( 'edit', 'window_close' ),
                ( 'edit', 'apply_vetoed' ),
                ( 'manage', 'apply' ),
                ( 'manage', 'cancel' ),
                ( 'nullipotent', 'close_button' ),
                ( 'nullipotent', 'window_close' ),
            ]

            for ( kind, action ) in dialog_actions:

                print( 'case', kind, action, file = sys.stderr, flush = True )

                controller.new_options.SetFrameLocation( KEY, *BASELINE )

                if kind == 'edit':

                    dialog = W.DialogEdit( gui, 'edit something' )

                elif kind == 'manage':

                    dialog = W.DialogManage( gui, 'manage something' )

                else:

                    dialog = W.DialogNullipotent( gui, 'review something' )

                panel = Panel( dialog, veto = action == 'apply_vetoed' )

                dialog.SetPanel( panel )

                opened = geometry( dialog )

                dialog.setModal( True )
                dialog.show()

                settle()

                dialog.resize( *MOVED_SIZE )
                dialog.move( *MOVED_POSITION )

                settle()

                before = frame()
                actual = geometry( dialog )

                if action in ( 'apply', 'apply_vetoed' ):

                    dialog._apply.click()

                elif action == 'cancel':

                    dialog._cancel.click()

                elif action == 'close_button':

                    dialog._close.click()

                elif action == 'escape':

                    dialog._apply.setFocus()

                    event = QG.QKeyEvent( QC.QEvent.Type.KeyPress, QC.Qt.Key.Key_Escape, QC.Qt.KeyboardModifier.NoModifier )

                    dialog.keyPressEvent( event )

                elif action == 'window_close':

                    dialog.close()

                settle()

                case = {
                    'kind' : kind,
                    'action' : action,
                    'opened' : opened,
                    'before' : before,
                    'actual_window' : actual,
                    'after' : frame(),
                    'still_open' : dialog.isVisible(),
                    'committed' : panel.committed,
                }

                if dialog.isVisible():

                    dialog._cancel.click()

                    settle()

                    case[ 'after_cancel' ] = frame()

                cases.append( case )

                dialog.deleteLater()

                settle()

            for action in ( 'close_button', 'window_close' ):

                controller.new_options.SetFrameLocation( KEY, *BASELINE )

                window = W.FrameThatTakesScrollablePanel( gui, 'review something' )

                panel = ReviewPanel( window )

                window.SetPanel( panel )

                settle()

                opened = geometry( window )

                window.resize( *MOVED_SIZE )
                window.move( *MOVED_POSITION )

                # (its frame saves a moment after each move and resize too)
                settle( 0.5 )

                moved = frame()

                controller.new_options.SetFrameLocation( KEY, *BASELINE )

                before = frame()
                actual = geometry( window )

                if action == 'close_button':

                    window._ok.click()

                else:

                    window.close()

                settle( 0.5 )

                cases.append( {
                    'kind' : 'frame',
                    'action' : action,
                    'opened' : opened,
                    'after_move' : moved,
                    'before' : before,
                    'actual_window' : actual,
                    'after' : frame(),
                    'still_open' : QP.isValid( window ) and window.isVisible(),
                } )

            result[ 'lifecycle' ] = cases

            # opening

            openings = []

            for ( name, location, main_at ) in OPENINGS:

                print( 'opening', name, file = sys.stderr, flush = True )

                controller.new_options.SetFrameLocation( KEY, *location )

                if main_at is None:

                    gui.move( MAIN[0], MAIN[1] )

                else:

                    gui.move( *main_at )

                settle()

                main = [ gui.frameGeometry().x(), gui.frameGeometry().y(), gui.frameGeometry().width(), gui.frameGeometry().height() ]

                del shown[:]

                dialog = W.DialogEdit( gui, 'edit something' )

                panel = Panel( dialog )

                dialog.SetPanel( panel )

                placed = geometry( dialog )

                dialog.show()

                settle()

                openings.append( {
                    'name' : name,
                    'frame' : list( location ),
                    'main' : main,
                    'placed' : placed,
                    'shown' : geometry( dialog ),
                    'messages' : list( shown ),
                } )

                dialog._cancel.click()

                settle()

                dialog.deleteLater()

            gui.move( MAIN[0], MAIN[1] )

            result[ 'opening' ] = openings

        finally:

            HydrusData.ShowText = original_show_text
            ClientGUITopLevelWindows.HydrusData.ShowText = original_show_text

            controller.new_options = original

    controller.CallBlockingToQt( gui, work )

    return result


def main():

    import hydrus_driver

    if len( sys.argv ) > 1 and sys.argv[ 1 ] == '--child':

        import record_api

        output = sys.argv[ 2 ]

        value = hydrus_driver.run_client( record_api.unpack_fixture( 'basic' ), record )

        with open( output, 'w' ) as stream:

            json.dump( value, stream )

        return

    with tempfile.TemporaryDirectory() as directory:

        path = os.path.join( directory, 'out.json' )

        hydrus_driver.run_in_subprocess( os.path.abspath( __file__ ), '--child', path )

        with open( path ) as stream:

            value = json.load( stream )

    with open( OUT, 'w' ) as stream:

        json.dump( value, stream, indent = 1 )
        stream.write( '\n' )

    print( 'wrote', OUT )


if __name__ == '__main__':

    main()

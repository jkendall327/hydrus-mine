#!/usr/bin/env python3
"""Record the reference's thumbnail grid selection.

The thumbnail grid's own selection code (`MediaResultsPanel._HitMedia`,
`_Select`, `_SetFocusedMedia`, `_DeselectSelect`, the shift-select
helpers, and `MediaResultsPanelThumbnails._MoveThumbnailFocus`,
`_ScrollHome` and `_ScrollEnd`) is run on a stand-in grid of twelve files,
four columns wide and two rows a page, through a script of clicks (plain,
ctrl, shift; on a file or on none), select all and none, focus moves and
files leaving the page. After each step, the files selected and the one
focused are recorded.

The stand-in has the attributes those methods use; drawing does nothing,
and a selected file is always taken to be in view (so selecting all
doesn't move the focus). Files leaving the page do as the grid's
`_RemoveMediaDirectly` does: the focus cleared if it leaves, then the
files dropped from the selection and the list, and shift-selecting ended.
A new client's options (ctrl and shift clicks don't focus).

Usage: QT_QPA_PLATFORM=offscreen python oracle/record_thumbnail_selection.py
       (writes fixtures/thumbnail_selection.json)
"""

import json
import os
import sys

HERE = os.path.dirname( os.path.abspath( __file__ ) )

sys.path.insert( 0, os.path.dirname( HERE ) )

OUT = os.path.join( HERE, 'fixtures', 'thumbnail_selection.json' )

NUM_FILES = 12
COLUMNS = 4
PAGE_ROWS = 2

# ( 'click', file or None, ctrl, shift ), ( 'all', ), ( 'none', ),
# ( 'move', direction, shift ), ( 'remove', [ files ] )
SCRIPT = [
    ( 'click', 2, False, False ),
    ( 'click', 5, True, False ),
    ( 'move', 'right', False ),
    ( 'click', 9, False, True ),
    ( 'click', 4, False, True ),
    ( 'click', 5, True, False ),
    ( 'click', 8, False, True ),
    ( 'move', 'down', True ),
    ( 'move', 'up', True ),
    ( 'click', 7, False, False ),
    ( 'click', 7, True, False ),
    ( 'move', 'left', False ),
    ( 'all', ),
    ( 'move', 'home', True ),
    ( 'none', ),
    ( 'move', 'right', False ),
    ( 'click', 3, False, False ),
    ( 'click', 6, False, True ),
    ( 'remove', [ 4, 5 ] ),
    ( 'move', 'right', False ),
    ( 'click', 9, False, False ),
    ( 'click', 10, True, False ),
    ( 'click', 8, True, False ),
    ( 'remove', [ 9 ] ),
    ( 'move', 'right', False ),
    ( 'remove', [ 8 ] ),
    ( 'move', 'left', False ),
    ( 'move', 'page_down', False ),
    ( 'move', 'page_up', True ),
    ( 'move', 'end', True ),
    ( 'click', 0, True, True ),
    ( 'move', 'home', False ),
    ( 'click', None, True, False ),
    ( 'click', None, False, False ),
    ( 'move', 'left', False ),
    ( 'click', 1, True, False ),
    ( 'click', 3, True, False ),
    ( 'click', 2, False, True ),
    ( 'all', ),
    ( 'click', 6, True, False ),
    ( 'none', ),
    ( 'move', 'up', False ),
    # clicking the focused file again leaves the keyboard where a ctrl+click
    # put it
    ( 'click', 2, False, False ),
    ( 'click', 6, True, False ),
    ( 'click', 2, False, False ),
    ( 'move', 'right', False ),
    # selecting all with nothing focused ends shift-selecting
    ( 'click', None, False, False ),
    ( 'click', 3, True, False ),
    ( 'all', ),
    ( 'click', 10, False, True ),
]


class FakeMedia( object ):

    def __init__( self, i ):

        self.i = i
        self._selected = False


    def __repr__( self ):

        return 'file {}'.format( self.i )


    def Deselect( self ):

        self._selected = False


    def GetDisplayMedia( self ):

        return self


    def GetDurationMS( self ):

        return None


    def IsSelected( self ):

        return self._selected


    def Select( self ):

        self._selected = True



class Signal( object ):

    def emit( self, *args ):

        pass



def record():

    from hydrus.core import HydrusLists
    from hydrus.client import ClientGlobals as CG
    from hydrus.client import ClientOptions
    from hydrus.client.gui.pages import ClientGUIMediaResultsPanel
    from hydrus.client.gui.pages import ClientGUIMediaResultsPanelThumbnails
    from hydrus.client.media import ClientMediaFileFilter

    class Controller( object ):

        new_options = ClientOptions.ClientOptions()


    CG.client_controller = Controller()

    base = ClientGUIMediaResultsPanel.MediaResultsPanel
    thumbnails = ClientGUIMediaResultsPanelThumbnails.MediaResultsPanelThumbnails

    class Grid( object ):

        _HitMedia = base._HitMedia
        _DeselectSelect = base._DeselectSelect
        _Select = base._Select
        _SetFocusedMedia = base._SetFocusedMedia
        _StartShiftSelect = base._StartShiftSelect
        _EndShiftSelect = base._EndShiftSelect
        _MoveThumbnailFocus = thumbnails._MoveThumbnailFocus
        _ScrollHome = thumbnails._ScrollHome
        _ScrollEnd = thumbnails._ScrollEnd

        def __init__( self, media ):

            self._sorted_media = HydrusLists.FastIndexUniqueList( media )
            self._selected_media = set()
            self._focused_media = None
            self._last_hit_media = None
            self._next_best_media_if_focuses_removed = None
            self._shift_select_started_with_this_media = None
            self._media_added_in_current_shift_select = set()
            self._num_columns = COLUMNS
            self._num_rows_per_actual_page = PAGE_ROWS
            self.focusMediaCleared = Signal()
            self.focusMediaChanged = Signal()


        def GetSortedMedia( self ):

            return self._sorted_media


        def GetSelectedMedia( self ):

            return self._selected_media


        def _MediaIsVisible( self, media ):

            return True


        def _PublishSelectionChange( self, tags_changed = False ):

            pass


        def _RedrawMedia( self, media ):

            pass


        def _ScrollToMedia( self, media ):

            pass


        def Remove( self, media ):

            # as MediaResultsPanelThumbnails._RemoveMediaDirectly does
            if self._focused_media is not None and self._focused_media in media:

                self._SetFocusedMedia( None )


            for m in media:

                m.Deselect()


            self._selected_media.difference_update( media )

            self._sorted_media = HydrusLists.FastIndexUniqueList( [ m for m in self._sorted_media if m not in media ] )

            self._EndShiftSelect()



    media = [ FakeMedia( i ) for i in range( NUM_FILES ) ]

    grid = Grid( media )

    moves = {
        'left' : ( 0, -1 ),
        'right' : ( 0, 1 ),
        'up' : ( -1, 0 ),
        'down' : ( 1, 0 ),
        'page_up' : ( -PAGE_ROWS, 0 ),
        'page_down' : ( PAGE_ROWS, 0 ),
    }

    steps = []

    for step in SCRIPT:

        ( action, *args ) = step

        if action == 'click':

            ( i, ctrl, shift ) = args

            grid._HitMedia( None if i is None else media[ i ], ctrl, shift )

        elif action == 'all':

            grid._Select( ClientMediaFileFilter.FileFilter( ClientMediaFileFilter.FILE_FILTER_ALL ) )

        elif action == 'none':

            grid._Select( ClientMediaFileFilter.FileFilter( ClientMediaFileFilter.FILE_FILTER_NONE ) )

        elif action == 'move':

            ( direction, shift ) = args

            if direction == 'home':

                grid._ScrollHome( shift )

            elif direction == 'end':

                grid._ScrollEnd( shift )

            else:

                ( rows, columns ) = moves[ direction ]

                grid._MoveThumbnailFocus( rows, columns, shift )


        elif action == 'remove':

            ( files, ) = args

            grid.Remove( { media[ i ] for i in files } )


        steps.append( {
            'step' : list( step ),
            'selected' : sorted( m.i for m in grid._selected_media ),
            'focused' : None if grid._focused_media is None else grid._focused_media.i,
        } )


    return { 'files' : NUM_FILES, 'columns' : COLUMNS, 'page_rows' : PAGE_ROWS, 'steps' : steps }


def main():

    result = record()

    with open( OUT, 'w' ) as f:

        json.dump( result, f, indent = 1, sort_keys = True )
        f.write( '\n' )


    print( f'wrote {OUT}' )


if __name__ == '__main__':

    main()

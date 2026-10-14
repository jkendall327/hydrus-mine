#!/usr/bin/env python3
"""Record the reference's tag filter favourites export and import errors.

`EditTagFilterPanel`, on a tag filter with several rules and two favourites,
is driven through `_ExportFavourite`: every entry of the menu it pops up
("this tag filter" and each favourite) is triggered, and the text it puts on
the clipboard is recorded verbatim. Then `_ImportFavourite` is given clipboard
texts that are not a Tag Filter (not JSON, nothing, a list of another kind of
object, a Tag Filter with an impossible rule); the real error dialogs'
(`ShowCritical`) title and message are recorded, with the favourites after
each.

Usage: QT_QPA_PLATFORM=offscreen python oracle/record_tag_filter_favourite_exchange.py
       (writes fixtures/tag_filter_favourite_exchange.json)
"""

import json
import os
import sys

HERE = os.path.dirname( os.path.abspath( __file__ ) )

sys.path.insert( 0, HERE )

OUT = os.path.join( HERE, 'fixtures', 'tag_filter_favourite_exchange.json' )

# ( slice, rule ): 0 whitelist, 1 blacklist; set in this order
CURRENT = [ ( 'goblin', 1 ), ( 'series:', 1 ), ( 'creator:samus aran', 0 ), ( 'blue eyes', 0 ) ]
FAVOURITES = {
    'zebra' : [ ( ':', 1 ), ( 'orc', 0 ) ],
    'apple' : [ ( 'blue eyes', 0 ), ( 'creator:', 1 ), ( 'series:metroid', 0 ), ( '', 1 ) ],
}

IMPORTS = [ 'not json', '', '[26, 3, []]', '[21, 2, []]', '"just a string"', '{"a": 1}' ]


def record( session ):

    from qtpy import QtWidgets as W

    from hydrus.core import HydrusConstants as HC
    from hydrus.core import HydrusTags as H
    from hydrus.client.gui import ClientGUIAsync as A
    from hydrus.client.gui import ClientGUICore as C
    from hydrus.client.gui import ClientGUIDialogsMessage as M
    from hydrus.client.gui.metadata import ClientGUITagFilter as T

    controller = session.controller

    def qt():

        class Now( object ):

            def __init__( self, parent, work_callable, publish_callable, **kwargs ):

                self.work = work_callable
                self.publish = publish_callable


            def start( self ):

                self.publish( self.work() )



        A.AsyncQtJob = Now

        def make( rules ):

            f = H.TagFilter()

            for ( slice_, rule ) in rules:

                f.SetRule( slice_, HC.FILTER_BLACKLIST if rule == 1 else HC.FILTER_WHITELIST )


            return f


        options = controller.new_options

        options.SetFavouriteTagFilters( { name : make( rules ) for ( name, rules ) in FAVOURITES.items() } )

        panel = T.EditTagFilterPanel( controller.gui, make( CURRENT ), namespaces = [] )

        menus = []

        C.core().PopupMenu = lambda parent, menu: menus.append( menu )

        copied = []
        old_pub = controller.pub

        def pub( *args, **kwargs ):

            if args[ :2 ] == ( 'clipboard', 'text' ):

                copied.append( args[ 2 ] )

            else:

                old_pub( *args, **kwargs )



        controller.pub = pub

        panel._ExportFavourite()

        menu = menus.pop()

        exports = []

        for action in menu.actions():

            if action.isSeparator():

                continue


            del copied[:]

            action.trigger()

            exports.append( { 'label' : action.text(), 'text' : copied[ 0 ] } )


        errors = []

        M.ShowCritical = lambda parent, title, message: errors.append( { 'title' : title, 'message' : message } )

        clipboard = { 'text' : '' }

        controller.GetClipboardText = lambda: clipboard[ 'text' ]

        def favourites():

            return { name : [ list( kv ) for kv in f.GetTagSlicesToRules().items() ] for ( name, f ) in options.GetFavouriteTagFilters().items() }


        imports = []

        for text in IMPORTS:

            clipboard[ 'text' ] = text

            del errors[:]

            panel._ImportFavourite()

            imports.append( { 'text' : text, 'errors' : list( errors ), 'favourites' : favourites() } )


        return { 'current' : CURRENT, 'favourites' : FAVOURITES, 'exports' : exports, 'imports' : imports }


    return controller.CallBlockingToQt( controller.gui, qt )


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

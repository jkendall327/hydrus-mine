#!/usr/bin/env python3
"""Record the reference's tag filter editor (`EditTagFilterPanel`, opened by
a "tag filter" button, an import's "get tags" filter or a blacklist).

The editor has three tabs on one filter: "whitelist" (allow these: the
"unnamespaced tags"/"namespaced tags" boxes, a box per namespace, a list
and an input), "blacklist" (exclude these: the same, "block everything"),
and "advanced" ("exclude these" and "except for these" lists, each with an
input and "delete"); a blacklist-only editor has the blacklist tab alone.
Under the tabs it says when an entry is already blocked or permitted by a
broader rule, and the current filter; a "testing" box says whether typed
tags pass.

On the `basic` fixture, for each case of `CASES` (a filter's rules, whether
blacklist-only, the namespaces offered, then steps), this makes the panel
and records, after each step, the tabs and the one shown; per simple tab
whether it is enabled, its error, its list (tag slices, in order), its
boxes' ticks and whether the namespace boxes are enabled; the advanced
lists; whether the "except for these" input is enabled; the redundant-rule
message; the current filter text; the test result and its colour
(`HydrusValid`, `HydrusInvalid`); and the filter's rules. Steps:
`["white_global", i]`, `["white_ns", i]`, `["black_global", i]`,
`["black_ns", i]` (a box clicked), `["white_add", [slices]]`,
`["black_add", [slices]]`, `["adv_black_add", [slices]]`,
`["adv_white_add", [slices]]` (typed into an input),
`["white_remove", [slices]]`, `["black_remove", [slices]]` (double-clicked
in a simple list), `["adv_black_delete", [slices]]`,
`["adv_white_delete", [slices]]` (selected, "delete", answered yes),
`["block_everything"]` and `["test", text]`.

Usage: QT_QPA_PLATFORM=offscreen python oracle/record_tag_filter_editor.py
       (writes fixtures/tag_filter_editor.json)
"""

import json
import os
import sys

HERE = os.path.dirname( os.path.abspath( __file__ ) )

sys.path.insert( 0, HERE )

OUT = os.path.join( HERE, 'fixtures', 'tag_filter_editor.json' )

NAMESPACES = [ '', 'character', 'creator', 'series' ]

CASES = [
    # an empty filter: everything allowed, built up from the whitelist
    { 'rules' : [], 'blacklist_only' : False, 'steps' : [
        [ 'test', 'blue eyes' ],
        [ 'white_global', 1 ],
        [ 'test', 'blue eyes\ncreator:x\nseries:y' ],
        [ 'white_ns', 1 ],
        [ 'white_add', [ 'Series:*', 'character:samus aran' ] ],
        [ 'white_add', [ 'creator:' ] ],
        [ 'white_global', 0 ],
        [ 'test', 'blue eyes' ],
        [ 'white_remove', [ 'creator:' ] ],
        [ 'white_global', 1 ],
        [ 'white_global', 0 ],
    ] },
    # the blacklist tab, and what the others make of it
    { 'rules' : [], 'blacklist_only' : False, 'steps' : [
        [ 'black_ns', 0 ],
        [ 'black_add', [ 'goblin', '  ORC  ', 'goblin' ] ],
        [ 'black_add', [ 'character:' ] ],
        [ 'black_global', 1 ],
        [ 'black_add', [ 'creator:bob' ] ],
        [ 'black_remove', [ 'goblin' ] ],
        [ 'adv_white_add', [ 'series:metroid', 'series:metroid', '*:*' ] ],
        [ 'adv_white_add', [ 'blue eyes' ] ],
        [ 'test', 'series:metroid\nseries:zelda\norc' ],
        [ 'adv_white_delete', [ 'series:metroid' ] ],
        [ 'block_everything' ],
        [ 'adv_black_delete', [ ':' ] ],
        [ 'adv_black_add', [ '*' ] ],
        [ 'black_global', 0 ],
    ] },
    # a filter too complicated for either simple tab
    { 'rules' : [ [ ':', 'black' ], [ 'goblin', 'black' ], [ 'creator:bob', 'white' ] ], 'blacklist_only' : False, 'steps' : [
        [ 'adv_black_add', [ 'creator:**' ] ],
        [ 'adv_black_delete', [ 'goblin' ] ],
    ] },
    # a fixed blacklist
    { 'rules' : [ [ 'goblin', 'black' ] ], 'blacklist_only' : True, 'steps' : [
        [ 'test', 'goblin' ],
        [ 'test', 'character:goblin\norc' ],
        [ 'black_ns', 2 ],
        [ 'black_global', 1 ],
    ] },
    # whitelisted namespaces, the editor opening on the whitelist
    { 'rules' : [ [ '', 'black' ], [ ':', 'black' ], [ 'creator:', 'white' ], [ 'series:', 'white' ] ], 'blacklist_only' : False, 'steps' : [
        [ 'white_ns', 1 ],
        [ 'white_ns', 1 ],
    ] },
]


class Row:

    def __init__( self, row ):

        self._row = row


    def row( self ):

        return self._row



def record( session ):

    controller = session.controller
    gui = controller.gui

    def f():

        from qtpy import QtWidgets as QW

        from hydrus.client.gui import ClientGUIAsync
        from hydrus.client.gui import ClientGUIDialogsQuick
        from hydrus.client.gui.metadata import ClientGUITagFilter
        from hydrus.core import HydrusConstants as HC
        from hydrus.core import HydrusTags

        said = []

        def yes_no( win, message, **kwargs ):

            said.append( { 'asked' : message } )

            return QW.QDialog.DialogCode.Accepted


        ClientGUIDialogsQuick.GetYesNo = yes_no

        class Now:

            def __init__( self, win, work_callable, publish_callable, **kwargs ):

                self._work = work_callable
                self._publish = publish_callable


            def start( self ):

                self._publish( self._work() )



        ClientGUIAsync.AsyncQtJob = Now

        out = []

        for case in CASES:

            tag_filter = HydrusTags.TagFilter()

            for ( slice_, rule ) in case[ 'rules' ]:

                tag_filter.SetRule( slice_, HC.FILTER_BLACKLIST if rule == 'black' else HC.FILTER_WHITELIST )


            panel = ClientGUITagFilter.EditTagFilterPanel( gui, tag_filter, only_show_blacklist = case[ 'blacklist_only' ], namespaces = NAMESPACES )

            def checks( box ):

                return [ box.IsChecked( i ) for i in range( box.count() ) ]


            def state():

                notebook = panel._notebook

                value = panel.GetValue()

                return {
                    'tabs' : [ notebook.tabText( i ) for i in range( notebook.count() ) ],
                    'tab' : notebook.tabText( notebook.currentIndex() ),
                    'whitelist' : {
                        'enabled' : panel._whitelist_panel.isEnabled(),
                        'error' : panel._simple_whitelist_error_st.text() if not panel._simple_whitelist_error_st.isHidden() else '',
                        'list' : panel._simple_whitelist.GetTagSlices(),
                        'global' : checks( panel._simple_whitelist_global_checkboxes ),
                        'namespaces' : checks( panel._simple_whitelist_namespace_checkboxes ),
                        'namespaces_enabled' : panel._simple_whitelist_namespace_checkboxes.isEnabled(),
                    },
                    'blacklist' : {
                        'enabled' : panel._blacklist_panel.isEnabled(),
                        'error' : panel._simple_blacklist_error_st.text() if not panel._simple_blacklist_error_st.isHidden() else '',
                        'list' : panel._simple_blacklist.GetTagSlices(),
                        'global' : checks( panel._simple_blacklist_global_checkboxes ),
                        'namespaces' : checks( panel._simple_blacklist_namespace_checkboxes ),
                        'namespaces_enabled' : panel._simple_blacklist_namespace_checkboxes.isEnabled(),
                    },
                    'advanced_blacklist' : panel._advanced_blacklist.GetTagSlices(),
                    'advanced_whitelist' : panel._advanced_whitelist.GetTagSlices(),
                    'except_input_enabled' : panel._advanced_whitelist_input.isEnabled(),
                    'redundant' : panel._redundant_st.text() if not panel._redundant_st.isHidden() else '',
                    'current' : panel._current_filter_st.text(),
                    'test' : panel._test_result_st.text(),
                    'test_colour' : panel._test_result_st.objectName(),
                    'rules' : sorted( [ [ s, 'black' if r == HC.FILTER_BLACKLIST else 'white' ] for ( s, r ) in value.GetTagSlicesToRules().items() ] ),
                    'permitted' : value.ToPermittedString(),
                    'filter_string' : value.ToFilterString(),
                    'blacklist_string' : value.ToBlacklistString(),
                }


            states = [ { 'step' : None, 'state' : state() } ]

            for step in case[ 'steps' ]:

                del said[:]

                panel._redundant_st.setVisible( False )

                ( kind, *args ) = step

                if kind == 'white_global':

                    panel.EventSimpleWhitelistGlobalCheck( Row( args[ 0 ] ) )

                elif kind == 'white_ns':

                    panel.EventSimpleWhitelistNamespaceCheck( Row( args[ 0 ] ) )

                elif kind == 'black_global':

                    panel.EventSimpleBlacklistGlobalCheck( Row( args[ 0 ] ) )

                elif kind == 'black_ns':

                    panel.EventSimpleBlacklistNamespaceCheck( Row( args[ 0 ] ) )

                elif kind == 'white_add':

                    panel._SimpleAddWhitelistMultiple( args[ 0 ] )

                elif kind == 'black_add':

                    panel._SimpleAddBlacklistMultiple( args[ 0 ] )

                elif kind == 'adv_black_add':

                    panel._AdvancedAddBlacklistMultiple( args[ 0 ] )

                elif kind == 'adv_white_add':

                    panel._AdvancedAddWhitelistMultiple( args[ 0 ] )

                elif kind == 'white_remove':

                    panel._simple_whitelist.RemoveTagSlices( args[ 0 ] )
                    panel._simple_whitelist.tagsRemoved.emit( args[ 0 ] )

                elif kind == 'black_remove':

                    panel._simple_blacklist.RemoveTagSlices( args[ 0 ] )
                    panel._simple_blacklist.tagsRemoved.emit( args[ 0 ] )

                elif kind == 'adv_black_delete':

                    panel._advanced_blacklist.GetSelectedTagSlices = lambda s = args[ 0 ]: list( s )

                    panel._AdvancedDeleteBlacklistButton()

                    del panel._advanced_blacklist.GetSelectedTagSlices

                elif kind == 'adv_white_delete':

                    panel._advanced_whitelist.GetSelectedTagSlices = lambda s = args[ 0 ]: list( s )

                    panel._AdvancedDeleteWhitelistButton()

                    del panel._advanced_whitelist.GetSelectedTagSlices

                elif kind == 'block_everything':

                    panel._AdvancedBlacklistEverything()

                elif kind == 'test':

                    panel._test_input.setPlainText( args[ 0 ] )


                states.append( { 'step' : step, 'said' : list( said ), 'state' : state() } )


            out.append( { 'rules' : case[ 'rules' ], 'blacklist_only' : case[ 'blacklist_only' ], 'states' : states } )

            panel.deleteLater()


        return {
            'namespaces' : NAMESPACES,
            'test_default' : ClientGUITagFilter.EditTagFilterPanel.TEST_RESULT_DEFAULT,
            'test_blacklist_default' : ClientGUITagFilter.EditTagFilterPanel.TEST_RESULT_BLACKLIST_DEFAULT,
            'cases' : out,
        }


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

        path = os.path.join( work, 'tag_filter_editor.json' )

        hydrus_driver.run_in_subprocess( os.path.abspath( __file__ ), '--child', path )

        with open( path ) as f:

            result = json.load( f )



    with open( OUT, 'w' ) as f:

        json.dump( result, f, indent = 1, ensure_ascii = False )
        f.write( '\n' )


    print( f'wrote {OUT}: {len( result[ "cases" ] )} cases' )


if __name__ == '__main__':

    main()

#!/usr/bin/env python3
"""Record the reference's "manage urls" dialog (`EditURLsPanel`, opened by
the thumbnails' and the viewer's urls > manage).

The dialog lists the files' URLs, sorted, each with how many of the files
have it when there are several ("https://a.com/1 (2)"), under a warning
when editing several files; a box takes a URL (enter adds it, or "ok"s
the dialog when empty); copy and paste buttons; a double-click on a URL
removes it and puts it in the box; delete removes the selected. A URL is
checked and normalised as the client stores it; texts that aren't URLs
are asked about ("--did not parse. Normally I would not recommend
importing invalid URLs, but do you want to force it anyway?"). Its value
is the content updates to apply: each URL added to the files that lacked
it, or deleted from those that had it. "ok" with text in the box asks
first.

On the `basic` fixture, with stand-in files (a hash and their URLs), for
each case of `CASES` this makes the panel and records, after each step,
its rows, whether the warning shows, the box's text, and the updates so
far (`[add or delete, url, hashes]`), with what was asked. Steps:
`["add", text, answer]` (typed and entered; the answer to a question, if
asked), `["paste", text, answer]`, `["select", [urls]]`, `["delete"]`,
`["double", url]`, `["copy"]` (recording what it copied) and `["ok", text,
answer]` (the box's text, then "ok").

Usage: QT_QPA_PLATFORM=offscreen python oracle/record_manage_urls.py
       (writes fixtures/manage_urls.json)
"""

import json
import os
import sys

HERE = os.path.dirname( os.path.abspath( __file__ ) )

sys.path.insert( 0, HERE )

OUT = os.path.join( HERE, 'fixtures', 'manage_urls.json' )

CASES = [
    { 'files' : [ [ 'aa', [ 'https://example.com/post/2', 'https://example.com/post/1' ] ] ], 'steps' : [
        [ 'add', 'https://example.com/post/3', None ],
        [ 'add', 'https://example.com/post/1', None ],
        [ 'add', 'https://example.com/a b?x=é#frag', None ],
        [ 'add', 'not a url', False ],
        [ 'add', 'not a url', True ],
        [ 'select', [ 'https://example.com/post/1' ] ],
        [ 'delete' ],
        [ 'double', 'https://example.com/post/2' ],
        [ 'copy' ],
        [ 'paste', 'https://a.net/1\n\nhttps://a.net/2\nhttps://a.net/1\n', None ],
        [ 'add', 'https://example.com/post/2', None ],
        [ 'ok', 'https://left.over', False ],
        [ 'ok', '', None ],
    ] },
    { 'files' : [ [ 'aa', [ 'https://example.com/post/1', 'https://shared.org/x' ] ], [ 'bb', [ 'https://shared.org/x' ] ], [ 'cc', [] ] ], 'steps' : [
        [ 'add', 'https://shared.org/x', None ],
        [ 'add', 'https://new.org/y', None ],
        [ 'select', [ 'https://shared.org/x', 'https://example.com/post/1' ] ],
        [ 'copy' ],
        [ 'delete' ],
        [ 'select', [] ],
        [ 'copy' ],
        [ 'paste', 'nope\nhttps://z.org/', True ],
    ] },
]


def record( session ):

    controller = session.controller
    gui = controller.gui

    def f():

        from qtpy import QtWidgets as QW

        from hydrus.client.gui import ClientGUIDialogsQuick
        from hydrus.client.gui.panels import ClientGUIScrolledPanelsEdit as M
        from hydrus.core import HydrusConstants as HC

        said = []
        answers = []
        clipboard = []

        def yes_no( win, message, **kwargs ):

            said.append( { 'asked' : message } )

            answer = answers.pop( 0 ) if len( answers ) > 0 else True

            return QW.QDialog.DialogCode.Accepted if answer else QW.QDialog.DialogCode.Rejected


        ClientGUIDialogsQuick.GetYesNo = yes_no

        real_pub = controller.pub

        def pub( topic, *args, **kwargs ):

            if topic == 'clipboard':

                clipboard.append( args[ 1 ] )

                return


            real_pub( topic, *args, **kwargs )


        controller.pub = pub

        pasted = []

        controller.GetClipboardText = lambda: pasted[ -1 ]

        class Media:

            # (a stand-in file: its hash and URLs)
            def __init__( self, hash, urls ):

                self._hash = hash
                self._urls = set( urls )


            def Duplicate( self ):

                return Media( self._hash, self._urls )


            def GetHash( self ):

                return self._hash


            def GetLocationsManager( self ):

                return self


            def GetURLs( self ):

                return set( self._urls )


            def GetMediaResult( self ):

                return self


            def ProcessContentUpdate( self, service_key, content_update ):

                ( ( urls, hashes ) ) = content_update.GetRow()

                if self._hash in hashes:

                    if content_update.GetAction() == HC.CONTENT_UPDATE_ADD:

                        self._urls.update( urls )

                    else:

                        self._urls.difference_update( urls )





        out = []

        for case in CASES:

            medias = [ Media( bytes.fromhex( h ), urls ) for ( h, urls ) in case[ 'files' ] ]

            panel = M.EditURLsPanel( gui, medias )

            box = panel._urls_listbox

            def state():

                updates = []

                for cu in panel.GetValue():

                    ( ( urls, hashes ) ) = cu.GetRow()

                    updates.append( [ 'add' if cu.GetAction() == HC.CONTENT_UPDATE_ADD else 'delete', list( urls )[ 0 ], sorted( h.hex() for h in hashes ) ] )


                return {
                    'rows' : [ box.item( i ).text() for i in range( box.count() ) ],
                    'warning' : not panel._multiple_files_warning.isHidden(),
                    'input' : panel._url_input.text(),
                    'updates' : updates,
                }


            steps = [ { 'state' : state() } ]

            for step in case[ 'steps' ]:

                del said[:]
                del clipboard[:]

                kind = step[ 0 ]

                if kind == 'add':

                    answers.append( step[ 2 ] )

                    panel._url_input.setText( step[ 1 ] )

                    panel.AddURL()

                elif kind == 'paste':

                    answers.append( step[ 2 ] )

                    pasted.append( step[ 1 ] )

                    panel._Paste()

                elif kind == 'select':

                    for i in range( box.count() ):

                        box.item( i ).setSelected( box.item( i ).data( 256 ) in step[ 1 ] )


                elif kind == 'delete':

                    panel.DeleteSelected()

                elif kind == 'double':

                    for i in range( box.count() ):

                        box.item( i ).setSelected( box.item( i ).data( 256 ) == step[ 1 ] )


                    panel.ListDoubleClicked( None )

                elif kind == 'copy':

                    panel._Copy()

                elif kind == 'ok':

                    answers.append( step[ 2 ] )

                    panel._url_input.setText( step[ 1 ] )

                    said.append( { 'ok' : panel.UserIsOKToOK() } )


                del answers[:]

                steps.append( { 'do' : step, 'said' : list( said ), 'copied' : list( clipboard ), 'state' : state() } )


            out.append( { 'files' : case[ 'files' ], 'steps' : steps } )


        return { 'cases' : out }


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

        path = os.path.join( work, 'manage_urls.json' )

        hydrus_driver.run_in_subprocess( os.path.abspath( __file__ ), '--child', path )

        with open( path ) as f:

            result = json.load( f )



    with open( OUT, 'w' ) as f:

        json.dump( result, f, indent = 1, ensure_ascii = False )
        f.write( '\n' )


    print( f'wrote {OUT}: {len( result[ "cases" ] )} cases' )


if __name__ == '__main__':

    main()

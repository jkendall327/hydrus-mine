#!/usr/bin/env python3
"""Record the reference's "manage notes" dialog (`EditFileNotesPanel`,
opened by a thumbnail's "manage > notes" for the focused file).

The dialog has a tab per note, by name (a lone empty "notes" tab when the
file has none), and the buttons "add" (asks a name), "edit current name"
(asks a name, the current one to start), "delete current note" (asks
first), copy (every note as JSON, by default), paste (JSON notes, merged
in) and copy URLs (from the note in view). Its value is the notes to set
(each tab's text cleaned, empty ones left out) and the original names to
delete.

On the `basic` fixture, for each case of `CASES` (a file's notes and the
note to start on, then steps), this makes the dialog and records, after
each step, the tabs (`[name, text]`), the current tab, whether "edit" and
"delete" are enabled, the value (`notes`, `deletees`) and whether
cancelling would ask (`changed`). Steps: `["add", name]`, `["rename",
index, name]`, `["delete"]` (the current tab, answered yes), `["type",
index, text]`, `["select", index]`, `["paste", text]` (recording what the
paste says, or the error), `["copy"]` and `["urls"]` (recording what each
put on the clipboard).

Usage: QT_QPA_PLATFORM=offscreen python oracle/record_manage_notes.py
       (writes fixtures/manage_notes.json)
"""

import json
import os
import sys

HERE = os.path.dirname( os.path.abspath( __file__ ) )

sys.path.insert( 0, HERE )

OUT = os.path.join( HERE, 'fixtures', 'manage_notes.json' )

CASES = [
    { 'notes' : {}, 'start_on' : None, 'steps' : [
        [ 'type', 0, '  hello\r\nworld  \n\n\n' ],
        [ 'add', 'notes' ],
        [ 'type', 1, 'second' ],
        [ 'add', 'notes' ],
        [ 'rename', 2, 'source' ],
        [ 'type', 2, 'see https://example.com/a?b=1, and http://x.org/"quoted" or <https://y.net/z>.' ],
        [ 'urls' ],
        [ 'copy' ],
    ] },
    { 'notes' : { 'b' : 'bee', 'a' : 'ay', 'c' : 'sea' }, 'start_on' : 'b', 'steps' : [
        [ 'rename', 1, 'a' ],
        [ 'rename', 1, 'b' ],
        [ 'rename', 0, 'c' ],
        [ 'delete' ],
        [ 'type', 1, '' ],
        [ 'select', 0 ],
        [ 'delete' ],
        [ 'delete' ],
        [ 'add', 'fresh' ],
    ] },
    { 'notes' : { 'note' : 'some text', 'other' : 'other text' }, 'start_on' : 'other', 'steps' : [
        [ 'paste', '{"note": "some text and more", "new": "brand new", "other": "different"}' ],
        [ 'paste', '[["x", "y"], 5, ["note", "some text and more"]]' ],
        [ 'paste', '[["x", "y", "z"]]' ],
        [ 'paste', '[[1, "y"]]' ],
        [ 'paste', 'not json' ],
        [ 'copy' ],
    ] },
    { 'notes' : { 'only' : 'x' }, 'start_on' : 'missing', 'steps' : [
        [ 'delete' ],
        [ 'add', '' ],
    ] },
]


def record( session ):

    controller = session.controller
    gui = controller.gui

    def f():

        from qtpy import QtWidgets as QW

        from hydrus.client.gui import ClientGUIDialogsMessage
        from hydrus.client.gui import ClientGUIDialogsQuick
        from hydrus.client.gui.widgets import ClientGUICommon
        from hydrus.client.gui.panels import ClientGUIScrolledPanelsEdit

        script = []
        said = []
        clipboard = []

        def enter_text( win, message, default = '', **kwargs ):

            said.append( { 'asked' : message, 'default' : default } )

            return script.pop( 0 )


        def yes_no( win, message, **kwargs ):

            said.append( { 'asked' : message } )

            return QW.QDialog.DialogCode.Accepted


        def parse_error( win, raw_text, expected, e ):

            said.append( { 'parse error' : str( e ) } )


        def critical( win, title, message, **kwargs ):

            said.append( { 'critical' : message } )


        ClientGUIDialogsQuick.EnterText = enter_text
        ClientGUIDialogsQuick.GetYesNo = yes_no
        ClientGUIDialogsQuick.PresentClipboardParseError = parse_error
        ClientGUIDialogsMessage.ShowCritical = critical

        ClientGUICommon.IconButton.ShowMicroNotification = lambda self, text: said.append( { 'notified' : text } )

        real_pub = controller.pub

        def pub( topic, *args, **kwargs ):

            if topic == 'clipboard':

                clipboard.append( args[ 1 ] )

                return


            real_pub( topic, *args, **kwargs )


        controller.pub = pub

        out = []

        for case in CASES:

            panel = ClientGUIScrolledPanelsEdit.EditFileNotesPanel( gui, case[ 'notes' ], case[ 'start_on' ] )

            def state():

                notebook = panel._notebook

                tabs = [ [ notebook.tabText( i ), notebook.widget( i ).toPlainText() ] for i in range( notebook.count() ) ]

                ( notes, deletees ) = panel.GetValue()

                return {
                    'tabs' : tabs,
                    'current' : notebook.currentIndex(),
                    'can_edit' : panel._edit_button.isEnabled(),
                    'notes' : notes,
                    'deletees' : sorted( deletees ),
                    'changed' : notes != panel._original_names_to_notes,
                }


            states = [ { 'step' : None, 'state' : state() } ]

            for step in case[ 'steps' ]:

                del said[:]
                del clipboard[:]

                ( kind, *args ) = step

                if kind == 'add':

                    script.append( args[ 0 ] )

                    panel._AddNote()

                elif kind == 'rename':

                    script.append( args[ 1 ] )

                    panel._EditName( index = args[ 0 ] )

                elif kind == 'delete':

                    if panel._delete_button.isEnabled():

                        panel._DeleteNote()


                elif kind == 'type':

                    panel._notebook.widget( args[ 0 ] ).setPlainText( args[ 1 ] )

                elif kind == 'select':

                    panel._notebook.setCurrentIndex( args[ 0 ] )

                elif kind == 'paste':

                    controller.GetClipboardText = lambda text = args[ 0 ]: text

                    panel._Paste()

                elif kind == 'copy':

                    panel._Copy()

                elif kind == 'urls':

                    panel._CopyURLs()


                states.append( { 'step' : step, 'said' : list( said ), 'clipboard' : list( clipboard ), 'state' : state() } )


            out.append( { 'notes' : case[ 'notes' ], 'start_on' : case[ 'start_on' ], 'states' : states } )

            panel.deleteLater()


        controller.pub = real_pub

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

        path = os.path.join( work, 'manage_notes.json' )

        hydrus_driver.run_in_subprocess( os.path.abspath( __file__ ), '--child', path )

        with open( path ) as f:

            result = json.load( f )



    with open( OUT, 'w' ) as f:

        json.dump( result, f, indent = 1, ensure_ascii = False )
        f.write( '\n' )


    print( f'wrote {OUT}: {len( result[ "cases" ] )} cases' )


if __name__ == '__main__':

    main()

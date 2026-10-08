#!/usr/bin/env python3
"""Record the reference's per-service additional tags button.

In the running client, on the `basic` fixture, the import options tags page
has a box for each tag service (`EditServiceTagImportOptionsPanel`) with an
"N additional tags" button that opens the "input tags" dialog on that
service's tags, and a cog menu with the "apply tags to ..." and "... overwrite
previously deleted tags" checks. This records, driving the real panel with
the dialog scripted (the answers it gives are set up front):

* `labels`: the button's label for service options with 0, 1, 2 and 1,234
  additional tags, before any click
* `dialogs`: for each scripted answer (accept with tags, accept with none,
  cancel): what the dialog was opened with (the tag service's name, the
  display type, the tags it started with, its message), the button's label
  after, and the value's additional tags (sorted as stored)
* `menu`: the cog menu's checks for a downloader's caller and not, their
  states, and the value after flipping "additional tags overwrite
  previously deleted tags"

Usage: QT_QPA_PLATFORM=offscreen ~/pyenv/bin/python oracle/record_additional_tags_button.py
       (writes fixtures/additional_tags_button.json)
"""

import json
import os
import sys

HERE = os.path.dirname( os.path.abspath( __file__ ) )

sys.path.insert( 0, HERE )

OUT = os.path.join( HERE, 'fixtures', 'additional_tags_button.json' )


def record( session ):

    from qtpy import QtWidgets as QW

    from hydrus.client import ClientConstants as CC
    from hydrus.client.gui import ClientGUIDialogs
    from hydrus.client.gui.importing import ClientGUIImportOptionsPanels
    from hydrus.client.importing.options import TagImportOptions

    controller = session.controller
    gui = controller.gui
    services = controller.services_manager

    def qt( f ):

        return controller.CallBlockingToQt( gui, f )

    my_tags = [ s for s in services.GetServices() if s.GetName() == 'my tags' ][ 0 ].GetServiceKey()

    opened = []
    scripted = []

    class FakeDialog:

        def __init__( self, parent, service_key, tag_display_type, tags, message = '' ):

            opened.append( { 'service' : services.GetName( service_key ), 'display_type' : tag_display_type, 'tags' : list( tags ), 'message' : message } )

            self._answer = scripted.pop( 0 )

        def __enter__( self ):

            return self

        def __exit__( self, *args ):

            return False

        def exec( self ):

            return QW.QDialog.DialogCode.Accepted if self._answer is not None else QW.QDialog.DialogCode.Rejected

        def GetTags( self ):

            return list( self._answer )

    ClientGUIDialogs.DialogInputTags = FakeDialog

    def panel_for( additional, downloader = True, overwrite = False ):

        options = TagImportOptions.ServiceTagImportOptions( additional_tags = list( additional ), additional_tags_overwrite_deleted = overwrite )

        return ClientGUIImportOptionsPanels.EditServiceTagImportOptionsPanel( gui, my_tags, options, show_downloader_options = downloader )

    def labels():

        out = []

        for count in ( 0, 1, 2, 1234 ):

            panel = panel_for( [ 'tag {}'.format( i ) for i in range( count ) ] )

            out.append( { 'count' : count, 'label' : panel._additional_button.text() } )

            panel.deleteLater()

        return out

    def dialogs():

        out = []

        cases = [
            ( 'accept with tags', [ 'zebra', 'apple', 'series:thing' ] ),
            ( 'accept with none', [] ),
            ( 'cancel', None ),
        ]

        for ( name, answer ) in cases:

            panel = panel_for( [ 'start tag' ] )

            opened.clear()

            scripted.append( answer )

            panel._additional_button.click()

            value = panel.GetValue()

            out.append( {
                'case' : name,
                'dialog' : opened[ 0 ],
                'label' : panel._additional_button.text(),
                'additional_tags' : list( value.ToTuple()[ 2 ] ),
            } )

            panel.deleteLater()

        return out

    def menu():

        out = {}

        for ( name, downloader ) in [ ( 'downloader', True ), ( 'not downloader', False ) ]:

            panel = panel_for( [], downloader = downloader )

            items = [ i for i in panel._GetCogIconMenuItems() ]

            out[ name ] = [ 'separator' if i.GetTitle() == 'separator' else { 'title' : i.GetTitle(), 'checked' : i.check_manager.GetCurrentValue() if hasattr( i, 'check_manager' ) else None } for i in items ]

            panel.deleteLater()

        panel = panel_for( [] )

        item = [ i for i in panel._GetCogIconMenuItems() if i.GetTitle() == 'additional tags overwrite previously deleted tags' ][ 0 ]

        item.check_manager.Invert()

        value = panel.GetValue()

        out[ 'flipped_overwrite' ] = value.ToTuple()[ 9 ]

        panel.deleteLater()

        return out

    return {
        'labels' : qt( labels ),
        'dialogs' : qt( dialogs ),
        'menu' : qt( menu ),
    }


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

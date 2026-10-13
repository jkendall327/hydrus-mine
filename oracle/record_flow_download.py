#!/usr/bin/env python3
"""Flow 2: download a URL, then tag, archive and delete the file.

On the `basic` fixture, with the fake booru (`flows.Site`) serving one
"blue eyes" post, and its downloader installed:

1. "download": a new URL downloader page (`NewPageImportURLs`), the post's
   URL typed in its input and entered. Wait until its importer is done and
   all is idle.
2. "tag": the page's thumbnail clicked, manage tags (its real
   `DialogManage`): "favourite" typed into "my tags" and entered, then OK.
3. "archive": archive, on the selected thumbnail.
4. "delete": delete, on the selected thumbnail: the delete files dialog
   (`EditDeleteFilesPanel`, the reference's default, not advanced, form)
   accepted as it opens.

After each step: the page's file log and its thumbnails (how many), the
file (tags, URLs, inbox, domains), the popups still shown and the site's
requests; and what the dialogs showed. Texts are normalised by
`flows.normaliser`.

Usage: QT_QPA_PLATFORM=offscreen ~/pyenv/bin/python oracle/record_flow_download.py
       (writes oracle/fixtures/flow_download.json)
"""

import os
import sys

HERE = os.path.dirname( os.path.abspath( __file__ ) )
sys.path.insert( 0, HERE )

import flows

OUT = os.path.join( HERE, 'fixtures', 'flow_download.json' )

SITE_PORT = 46012

PHASES = [
    flows.booru_phase( { 'blue_eyes' : [ 1 ] } ),
]


def record( session, site ):

    controller = session.controller
    gui = controller.gui
    normalise = flows.normaliser( site )

    domain_manager = controller.CallBlockingToQt( gui, lambda: flows.install( controller, site ) )

    steps = []

    def qt( f ):

        return controller.CallBlockingToQt( gui, f )


    page = qt( lambda: gui._notebook.NewPageImportURLs() )
    sidebar = page.GetSidebar()
    urls_import = sidebar._urls_import

    def media_panel():

        return page.GetMediaResultsPanel()


    def importer_idle():

        cache = urls_import.GetFileSeedCache()

        return cache.GetFileSeedCount() > 0 and cache.GetFileSeedCount( 0 ) == 0 and not urls_import.CurrentlyWorking()


    def step( name, seen ):

        files = flows.file_log_state( urls_import.GetFileSeedCache(), normalise )

        steps.append( {
            'step' : name,
            'dialog' : seen,
            'page' : {
                'files' : files,
                'thumbnails' : qt( lambda: len( media_panel().GetSortedMedia() ) ),
            },
            'files' : flows.files_state( session, flows.hashes_in_log( files ), normalise ),
            'popups' : flows.popups_shown( controller, normalise ),
            'hits' : site.take_hits(),
        } )


    def act( scripts, f ):
        """Do `f` on the Qt thread, its dialogs scripted, then wait."""

        with flows.scripted_dialogs( scripts ) as dialogs:

            qt( f )

            flows.wait_until_idle( controller, also = lambda: dialogs.check() and importer_idle() )



    # 1. download --------------------------------------------------------

    def enter_url():

        sidebar._url_input.SetValue( site.base + '/post/1' )
        sidebar._url_input.EnterText()


    act( {}, enter_url )

    step( 'download', {} )

    # 2. tag -------------------------------------------------------------

    seen = {}

    def select():

        panel = media_panel()

        panel._HitMedia( panel.GetSortedMedia()[0], False, False )


    def manage_tags( dlg, panel ):

        from hydrus.client import ClientConstants as CC

        tabs = panel._tag_services

        seen[ 'title' ] = dlg.windowTitle()
        seen[ 'services' ] = [ tabs.tabText( i ) for i in range( tabs.count() ) ]

        mine = next( tabs.widget( i ) for i in range( tabs.count() ) if tabs.widget( i ).GetServiceKey() == CC.DEFAULT_LOCAL_TAG_SERVICE_KEY )

        tabs.setCurrentWidget( mine )

        seen[ 'current tab' ] = tabs.tabText( tabs.currentIndex() )

        mine.EnterTags( { 'favourite' } )

        return True


    qt( select )

    act( { 'ManageTagsPanel' : [ manage_tags ] }, lambda: media_panel()._ManageTags() )

    step( 'tag', seen )

    # 3. archive ---------------------------------------------------------

    act( {}, lambda: media_panel()._Archive() )

    step( 'archive', {} )

    # 4. delete ----------------------------------------------------------

    seen = {}

    def delete_files( dlg, panel ):

        seen[ 'title' ] = dlg.windowTitle()
        seen[ 'description' ] = panel._simple_description.text() if panel._simple_description.isVisible() else None
        seen[ 'actions' ] = [ b.text() for b in panel._action_radio._radio_buttons ] if panel._action_radio.isVisible() else None

        return True


    act( { 'EditDeleteFilesPanel' : [ delete_files ] }, lambda: media_panel()._Delete() )

    step( 'delete', seen )

    return { 'domain_manager' : domain_manager, 'steps' : steps }


def child( out ):

    import hydrus_driver
    import record_api

    with flows.Site( SITE_PORT, PHASES ) as site:

        result = hydrus_driver.run_client( record_api.unpack_fixture( 'basic' ), lambda session: record( session, site ), network = True )


    result[ 'site' ] = site.describe()

    flows.write_fixture( out, result )


def main():

    if len( sys.argv ) > 2 and sys.argv[1] == '--child':

        child( sys.argv[2] )

        return


    import hydrus_driver

    hydrus_driver.run_in_subprocess( os.path.abspath( __file__ ), '--child', OUT )


if __name__ == '__main__':

    main()

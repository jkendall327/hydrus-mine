#!/usr/bin/env python3
"""Record the reference's "filename tagging" dialog for files about to be
imported from disk (`EditLocalImportFilenameTaggingPanel`, opened by
"review files to import"'s "add tags/urls with the import >>").

The dialog has a tab for sidecars and one per real tag service. A tag
service's tab lists the paths ("#", "path", "metadata": the tags each
gets) over its filename tagging options: "simple" (tags for all, tags
just for selected files, and "misc": the filename and directories as
tags, each with a namespace) and "advanced" (quick namespaces, regexes,
and "#": a number per file, from a base by a step, in a namespace).

On the `basic` fixture, this opens the dialog on `PATHS` and, for each
case of `CASES` (settings for the "my tags" tab, made anew each time),
records the tabs' names and each path's row (`rows`, as the list shows
it) and the tags the dialog gives back for each path (`value`: by
service name, paths with none left out).

Case settings: `all` tags for all; `filename` and `directories`
(`{index: namespace}`, index as the reference's: 0, 1, 2 from the top,
-1, -2, -3 from the file) as the "misc" box's ticked boxes; `quick`
`[namespace, regex]` pairs; `regexes`; `number` `[base, step,
namespace]`; `single` `{path index: tags}` (entered with only that path
selected).

Usage: QT_QPA_PLATFORM=offscreen python oracle/record_filename_tagging.py
       (writes fixtures/filename_tagging.json)
"""

import json
import os
import sys

HERE = os.path.dirname( os.path.abspath( __file__ ) )

sys.path.insert( 0, HERE )

OUT = os.path.join( HERE, 'fixtures', 'filename_tagging.json' )

PATHS = [
    '/home/user/pictures/artist name/blue series/0012 - page 3.jpg',
    '/home/user/pictures/artist name/0001.png',
    '/srv/import/Some_File (v2).webm',
]

CASES = [
    {},
    { 'all' : [ 'imported', 'Creator:Someone' ] },
    { 'filename' : '' },
    { 'filename' : 'title', 'directories' : { '0' : '', '-1' : 'series', '-2' : 'creator' } },
    { 'directories' : { '1' : 'second', '2' : 'third', '-3' : 'third last' } },
    { 'quick' : [ [ 'page', r'(?<=page )\d+' ], [ 'version', r'(?<=\(v)\d+(?=\))' ] ] },
    { 'regexes' : [ r'[a-z]+(?= series)', r'(\d{4})' ] },
    { 'number' : [ 1, 1, 'import order' ] },
    { 'number' : [ 10, -2, 'n' ], 'all' : [ 'x' ] },
    { 'single' : { '0' : [ 'first only' ], '2' : [ 'third only', 'Shared' ] }, 'all' : [ 'shared' ] },
]


def record( session ):

    controller = session.controller
    gui = controller.gui

    def f():

        from hydrus.client import ClientConstants as CC
        from hydrus.client.gui.importing import ClientGUIImport

        names = { service.GetServiceKey() : service.GetName() for service in controller.services_manager.GetServices() }

        out = []

        for case in CASES:

            panel = ClientGUIImport.EditLocalImportFilenameTaggingPanel( gui, PATHS )

            tabs = [ panel._notebook.tabText( i ) for i in range( panel._notebook.count() ) ]

            page = next( p for p in panel._filename_tagging_option_pages if p._service_key == CC.DEFAULT_LOCAL_TAG_SERVICE_KEY )

            options = page._filename_tagging_panel

            simple = options._simple_panel
            advanced = options._advanced_panel

            if 'all' in case:

                simple.EnterTags( set( case[ 'all' ] ) )


            if 'filename' in case:

                simple._filename_namespace.SetChecked( True )
                simple._filename_namespace.SetValue( case[ 'filename' ] )


            for ( index, namespace ) in case.get( 'directories', {} ).items():

                control = simple._directory_namespace_controls[ int( index ) ]

                control.SetChecked( True )
                control.SetValue( namespace )


            if 'quick' in case:

                advanced._quick_namespaces_list.AddDatas( [ tuple( q ) for q in case[ 'quick' ] ] )


            for regex in case.get( 'regexes', [] ):

                advanced._regexes.Append( regex, regex )


            if 'number' in case:

                ( base, step, namespace ) = case[ 'number' ]

                advanced._num_base.setValue( base )
                advanced._num_step.setValue( step )
                advanced._num_namespace.setText( namespace )


            for ( index, tags ) in case.get( 'single', {} ).items():

                simple.SetSelectedPaths( [ PATHS[ int( index ) ] ] )
                simple.EnterTagsSingle( set( tags ) )


            page.RefreshFileList()

            rows = [ list( page._ConvertDataToDisplayTuple( ( i, path ) ) ) for ( i, path ) in enumerate( PATHS ) ]

            ( routers, paths_to_tags ) = panel.GetValue()

            value = { path : { names[ key ] : list( tags ) for ( key, tags ) in service_keys_to_tags.items() } for ( path, service_keys_to_tags ) in paths_to_tags.items() }

            out.append( { 'case' : case, 'tabs' : tabs, 'rows' : rows, 'value' : value } )


        return { 'paths' : PATHS, 'cases' : out }


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

        path = os.path.join( work, 'filename_tagging.json' )

        hydrus_driver.run_in_subprocess( os.path.abspath( __file__ ), '--child', path )

        with open( path ) as f:

            result = json.load( f )



    with open( OUT, 'w' ) as f:

        json.dump( result, f, indent = 1, ensure_ascii = False )
        f.write( '\n' )


    print( f'wrote {OUT}: {len( result[ "cases" ] )} cases' )


if __name__ == '__main__':

    main()

#!/usr/bin/env python3
"""Flow: the Client API driving a download, then searching and tagging.

On the `basic` fixture, with the fake booru (`flows.Site`) and its
downloader installed, and the Client API on (the oracle's full access key):

1. "add url": `POST /add_urls/add_url` with a post's URL, the page name
   "api downloads" and an additional tag for "my tags"; then
   `/manage_pages/get_pages` polled until that page is there, and the
   client left until its importer is done.
2. "search": `GET /get_files/search_files` for the tag, hashes returned,
   then `GET /get_files/file_metadata` for them (with notes).
3. "tag and note": `POST /add_tags/add_tags` (another tag on "my tags")
   and `POST /add_notes/set_notes` (a note "comment"), then the metadata
   again.

After each step: each call's status and answer (page keys, file ids and
times left out; of a file's metadata, what this flow changes: its URLs,
inbox, "my tags" and notes), the pages the GUI shows (names and kinds), the
"api downloads" page's file log, the files (`flows.files_state`) and the
site's requests.

Usage: QT_QPA_PLATFORM=offscreen ~/pyenv/bin/python oracle/record_flow_client_api.py
       (writes oracle/fixtures/flow_client_api.json)
"""

import json
import os
import sys
import time

HERE = os.path.dirname( os.path.abspath( __file__ ) )
sys.path.insert( 0, HERE )

import flows

OUT = os.path.join( HERE, 'fixtures', 'flow_client_api.json' )

SITE_PORT = 46017
API_PORT = 45917

POST = 5
PAGE = 'api downloads'

PHASES = [
    flows.booru_phase( { 'blue_eyes' : [ POST ] } ),
]

MY_TAGS = '6c6f63616c2074616773'


def pages_of( pages ):
    """`/manage_pages/get_pages`'s tree: names, types and what is selected,
    without the keys (made anew each run)."""

    out = { 'name' : pages[ 'name' ], 'page_type' : pages[ 'page_type' ], 'selected' : pages.get( 'selected' ) }

    if 'pages' in pages:

        out[ 'pages' ] = [ pages_of( p ) for p in pages[ 'pages' ] ]


    return out


def metadata_of( answer, normalise ):
    """Of `/get_files/file_metadata`'s answer, what this flow changes."""

    out = []

    for m in answer[ 'metadata' ]:

        mine = m[ 'tags' ].get( MY_TAGS, {} ).get( 'storage_tags', {} ).get( '0', [] )

        out.append( {
            'hash' : m[ 'hash' ],
            'is_inbox' : m[ 'is_inbox' ],
            'known_urls' : sorted( normalise( u ) for u in m[ 'known_urls' ] ),
            'my tags' : sorted( mine ),
            'notes' : m.get( 'notes' ),
        } )


    return out


def text( answer ):
    """An answer that is not JSON (an empty body), as text."""

    return answer.decode( 'utf-8' ) if isinstance( answer, bytes ) else answer


def record( session, site ):

    controller = session.controller
    gui = controller.gui
    api = session.api
    normalise = flows.normaliser( site )

    domain_manager = controller.CallBlockingToQt( gui, lambda: flows.install( controller, site ) )

    steps = []

    def gui_pages():

        def f():

            return [ p.GetName() for p in gui._notebook.GetMediaPages() ]


        return controller.CallBlockingToQt( gui, f )


    def api_page():

        def f():

            found = [ p for p in gui._notebook.GetMediaPages() if p.GetName() == PAGE ]

            return found[0] if len( found ) > 0 else None


        return controller.CallBlockingToQt( gui, f )


    def page_log():

        page = api_page()

        if page is None:

            return None


        urls_import = page.GetPageManager().GetVariable( 'urls_import' )

        return flows.file_log_state( urls_import.GetFileSeedCache(), normalise )


    def page_done():

        files = page_log()

        return files is not None and len( files ) > 0 and all( f[ 'status' ] != 0 for f in files )


    def step( name, calls ):

        files = page_log() or []

        steps.append( {
            'step' : name,
            'calls' : calls,
            'gui pages' : gui_pages(),
            'page' : files,
            'files' : flows.files_state( session, flows.hashes_in_log( files ), normalise ),
            'hits' : site.take_hits(),
        } )


    # 1. add url ---------------------------------------------------------

    calls = []

    answer = api.post( '/add_urls/add_url', {
        'url' : site.base + f'/post/{POST}',
        'destination_page_name' : PAGE,
        'service_keys_to_additional_tags' : { MY_TAGS : [ 'from api' ] },
    } )

    calls.append( { 'call' : 'add_url', 'answer' : { k : normalise( v ) if isinstance( v, str ) else v for ( k, v ) in answer.items() if k != 'version' and k != 'hydrus_version' } } )

    deadline = time.time() + 60

    while True:

        pages = api.get( '/manage_pages/get_pages' )[ 'pages' ]

        names = json.dumps( pages )

        if PAGE in names:

            break


        if time.time() > deadline:

            raise Exception( 'the page never came' )


        time.sleep( 0.2 )


    flows.wait_until_idle( controller, also = page_done )

    calls.append( { 'call' : 'get_pages', 'answer' : pages_of( api.get( '/manage_pages/get_pages' )[ 'pages' ] ) } )

    step( 'add url', calls )

    # 2. search ----------------------------------------------------------

    calls = []

    found = api.get( '/get_files/search_files', tags = json.dumps( [ 'from api' ] ), return_hashes = 'true', return_file_ids = 'false' )

    hashes = found[ 'hashes' ]

    calls.append( { 'call' : 'search_files', 'answer' : { 'hashes' : hashes } } )

    def metadata():

        return metadata_of( api.get( '/get_files/file_metadata', hashes = json.dumps( hashes ), include_notes = 'true' ), normalise )


    calls.append( { 'call' : 'file_metadata', 'answer' : metadata() } )

    step( 'search', calls )

    # 3. tag and note ----------------------------------------------------

    calls = []

    answer = api.post( '/add_tags/add_tags', { 'hashes' : hashes, 'service_keys_to_tags' : { MY_TAGS : [ 'api tag' ] } } )

    calls.append( { 'call' : 'add_tags', 'answer' : text( answer ) } )

    answer = api.post( '/add_notes/set_notes', { 'hash' : hashes[0], 'notes' : { 'comment' : 'from the api' } } )

    calls.append( { 'call' : 'set_notes', 'answer' : { k : v for ( k, v ) in answer.items() if k not in ( 'version', 'hydrus_version' ) } } )

    flows.wait_until_idle( controller )

    calls.append( { 'call' : 'file_metadata', 'answer' : metadata() } )

    step( 'tag and note', calls )

    return { 'domain_manager' : domain_manager, 'steps' : steps }


def child( out ):

    import hydrus_driver
    import record_api

    with flows.Site( SITE_PORT, PHASES ) as site:

        result = hydrus_driver.run_client( record_api.unpack_fixture( 'basic' ), lambda session: record( session, site ), port = API_PORT, network = True )


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

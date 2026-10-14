#!/usr/bin/env python3
"""Record what the reference's prefetch options do to a real download.

A fake booru is served over localhost and the real client's URL page imports
its posts. Posts 1-3 are imported first (default options). Then, one step per
scenario, a URL is added with the page's import options set to a given
prefetch panel result (hash/URL check type, the two "fetch metadata even if
known" flags, the neighbour-spam check) and the file's history staged in the
database (a URL already known, a second URL for the same file, the file
deleted). For each step it records what the site was asked for (the post page
and its file) and the file seed's final status and note: the reference's
`PredictPreImportStatus` at work in `FileSeed.WorkOnURL`.

Post 90's page names the same image as post 1's, with its sha256 as a parsed
hash, so a hash check can know the file before it is fetched.

Usage: QT_QPA_PLATFORM=offscreen python oracle/record_prefetch_outcomes.py
       (writes fixtures/prefetch_outcomes.json)
"""

import hashlib
import json
import os
import re
import shutil
import sys
import tempfile
import time

HERE = os.path.dirname( os.path.abspath( __file__ ) )

sys.path.insert( 0, HERE )

import flows
import hydrus_driver

OUT = os.path.join( HERE, 'fixtures', 'prefetch_outcomes.json' )

SITE_PORT = 45987
API_PORT = 45988

NOT_CHECK, CHECK, DISPOSITIVE = 0, 1, 2

# ( name, setup, url, options ): options are ( hash_check, url_check, neighbour_spam, fetch_hash, fetch_url )
# setup: None, 'extra_url' (post 1's file also known by /post/50), 'delete' (post 2's file deleted),
# 'preset_hash' (the file seed is queued with post 1's file's sha256 already known)
SCENARIOS = [
    ( 'defaults, a known post url', None, '/post/1', ( DISPOSITIVE, CHECK, True, False, False ) ),
    ( 'url check off, a known post url', None, '/post/1', ( DISPOSITIVE, NOT_CHECK, True, False, False ) ),
    ( 'url check on, fetch metadata anyway', None, '/post/1', ( DISPOSITIVE, CHECK, True, False, True ) ),
    ( 'url matches dispositive', None, '/post/1', ( CHECK, DISPOSITIVE, True, False, False ) ),
    ( 'a parsed hash known, hash check on', None, '/post/90', ( CHECK, NOT_CHECK, True, False, False ) ),
    ( 'a parsed hash known, hash check dispositive', None, '/post/90', ( DISPOSITIVE, NOT_CHECK, True, False, False ) ),
    ( 'a parsed hash known, hash check off', None, '/post/90', ( NOT_CHECK, NOT_CHECK, True, False, False ) ),
    ( 'a parsed md5 known, hash check on', None, '/post/91', ( CHECK, NOT_CHECK, True, False, False ) ),
    ( 'a parsed md5 known, hash check off', None, '/post/91', ( NOT_CHECK, NOT_CHECK, True, False, False ) ),
    ( 'the seed already has the hash, hash check on', 'preset_hash', '/post/90', ( CHECK, NOT_CHECK, True, False, False ) ),
    ( 'the seed already has the hash, hash check on, fetch metadata anyway', 'preset_hash', '/post/90', ( CHECK, NOT_CHECK, True, True, False ) ),
    ( 'the seed already has the hash, hash dispositive', 'preset_hash', '/post/90', ( DISPOSITIVE, NOT_CHECK, True, False, False ) ),
    ( 'the seed already has the hash, hash check off', 'preset_hash', '/post/90', ( NOT_CHECK, NOT_CHECK, True, False, False ) ),
    ( 'neighbour spam check on, a file with two post urls', 'extra_url', '/post/1', ( DISPOSITIVE, CHECK, True, False, False ) ),
    ( 'neighbour spam check off, a file with two post urls', 'extra_url', '/post/1', ( DISPOSITIVE, CHECK, False, False, False ) ),
    ( 'a deleted file, deleted files excluded', 'delete', '/post/2', ( DISPOSITIVE, CHECK, True, False, False ) ),
    ( 'a deleted file, deleted files not excluded', 'delete', '/post/2', ( DISPOSITIVE, CHECK, True, False, False ), False ),
]


def normalise_note( note ):
    """The note without when things happened (it says how long ago)."""

    note = re.sub( r'at \d{4}-\d\d-\d\d \d\d:\d\d:\d\d', 'at TIME', note )

    return re.sub( r'which was .*? before this check', 'which was AGO before this check', note )


def build_phase():

    pages = {}
    files = {}

    for pid in [ 1, 2, 3, 90, 91 ]:

        image = flows.IMAGES[ ( ( 1 if pid in ( 90, 91 ) else pid ) - 1 ) % len( flows.IMAGES ) ]

        with open( os.path.join( flows.MEDIA_DIR, image ), 'rb' ) as f:

            data = f.read()

        sha256 = hashlib.sha256( data ).hexdigest()
        md5 = hashlib.md5( data ).hexdigest()


        pages[ f'/post/{pid}' ] = (
            f'<html><head><title>post {pid}</title></head><body>'
            f'<ul id="tags"><li class="tag">post {pid}</li></ul>'
            f'<span class="hash">{sha256}</span>'
            f'<img id="image" src="/files/p{pid}.jpg"></body></html>'
        ) if pid == 90 else (
            f'<html><head><title>post {pid}</title></head><body>'
            f'<ul id="tags"><li class="tag">post {pid}</li></ul>'
            f'<span class="md5">{md5}</span>'
            f'<img id="image" src="/files/p{pid}.jpg"></body></html>'
        ) if pid == 91 else (
            f'<html><head><title>post {pid}</title></head><body>'
            f'<ul id="tags"><li class="tag">post {pid}</li></ul>'
            f'<img id="image" src="/files/p{pid}.jpg"></body></html>'
        )
        files[ f'/files/p{pid}.jpg' ] = image


    return { 'pages' : pages, 'files' : files }


def definitions( site ):

    from hydrus.core import HydrusConstants as HC
    from hydrus.client import ClientStrings as S
    from hydrus.client.networking import ClientNetworkingURLClass as U
    from hydrus.client.parsing import ClientParsing as P

    def fixed( text ):

        return S.StringMatch( match_type = S.STRING_MATCH_FIXED, match_value = text, example_string = text )


    def html( tag, attrs, content, attribute = None ):

        return P.ParseFormulaHTML(
            tag_rules = [ P.ParseRuleHTML( rule_type = P.HTML_RULE_TYPE_DESCENDING, tag_name = tag, tag_attributes = attrs ) ],
            content_to_fetch = content,
            attribute_to_fetch = attribute
        )


    url_class = U.URLClass(
        'local booru post',
        url_type = HC.URL_TYPE_POST,
        preferred_scheme = 'http',
        url_domain_mask = U.URLDomainMask( raw_domains = [ site.host ] ),
        path_components = [ ( fixed( 'post' ), None ), ( S.StringMatch(), None ) ],
        parameters = [],
        example_url = site.base + '/post/1'
    )

    parser = P.PageParser(
        'local booru post parser',
        content_parsers = [
            P.ContentParser( name = 'tags', content_type = HC.CONTENT_TYPE_MAPPINGS, formula = html( 'li', { 'class' : 'tag' }, P.HTML_CONTENT_STRING ), additional_info = None ),
            P.ContentParser( name = 'file', content_type = HC.CONTENT_TYPE_URLS, formula = html( 'img', { 'id' : 'image' }, P.HTML_CONTENT_ATTRIBUTE, 'src' ), additional_info = ( HC.URL_TYPE_DESIRED, 50 ) ),
            P.ContentParser( name = 'md5', content_type = HC.CONTENT_TYPE_HASH, formula = html( 'span', { 'class' : 'md5' }, P.HTML_CONTENT_STRING ), additional_info = ( 'md5', 'hex' ) ),
            P.ContentParser( name = 'hash', content_type = HC.CONTENT_TYPE_HASH, formula = html( 'span', { 'class' : 'hash' }, P.HTML_CONTENT_STRING ), additional_info = ( 'sha256', 'hex' ) ),
        ],
        example_urls = [ site.base + '/post/1' ]
    )

    return ( [ url_class ], [ parser ], [] )


def wait_until( what, test, timeout = 120 ):

    deadline = time.time() + timeout

    while time.time() < deadline:

        if test():

            return


        time.sleep( 0.3 )


    raise Exception( f'timed out waiting for {what}' )


def run( session, site ):

    from hydrus.core import HydrusConstants as HC
    from hydrus.client import ClientConstants as CC
    from hydrus.client.importing.options import ImportOptionsContainer, PrefetchImportOptions
    from hydrus.client.metadata import ClientContentUpdates

    controller = session.controller

    flows.install( controller, site, definitions )

    def media_pages():

        return controller.CallBlockingToQt( controller.gui, lambda: controller.gui._notebook.GetMediaPages() )


    seen = []

    def urls_imports():

        out = []

        for page in media_pages():

            manager = page.GetPageManager()

            if manager.HasVariable( 'urls_import' ):

                out.append( manager.GetVariable( 'urls_import' ) )


        return out


    def new_importer():

        for importer in urls_imports():

            if not any( importer is s for s in seen ):

                seen.append( importer )

                return importer


        return None


    def add( path, page ):

        # (a page of its own for each step: a URL already in a page's log is not queued again)
        session.api.post( '/add_urls/add_url', { 'url' : site.base + path, 'destination_page_name' : page } )


    current = []

    def idle( count ):

        if not current:

            importer = new_importer()

            if importer is None:

                return False


            current.append( importer )


        cache = current[ 0 ].GetFileSeedCache()

        return cache.GetFileSeedCount() >= count and cache.GetFileSeedCount( CC.STATUS_UNKNOWN ) == 0


    def urls_import():

        return current[ 0 ]


    def seed_record():

        seed = urls_import().GetFileSeedCache().GetFileSeeds()[ -1 ]

        sha256 = seed._hashes.get( 'sha256' )

        return { 'status' : seed.status, 'note' : normalise_note( seed.note ), 'sha256' : None if sha256 is None else sha256.hex() }


    def hash_of( path ):

        with open( os.path.join( flows.MEDIA_DIR, site.phases[ 0 ][ 'files' ][ path ] ), 'rb' ) as f:

            return hashlib.sha256( f.read() ).digest()



    # the first three posts, with the client's own defaults
    for pid in ( 1, 2, 3 ):

        current.clear()

        add( f'/post/{pid}', f'seed {pid}' )

        wait_until( f'post {pid}', lambda: idle( 1 ) )

        time.sleep( 0.5 )


    site.take_hits()

    recorded = []

    for scenario in SCENARIOS:

        ( name, setup, path, ( hash_check, url_check, neighbour, fetch_hash, fetch_url ) ) = scenario[ :4 ]

        exclude_deleted = scenario[ 4 ] if len( scenario ) > 4 else True

        if setup == 'extra_url':

            update = ClientContentUpdates.ContentUpdate( HC.CONTENT_TYPE_URLS, HC.CONTENT_UPDATE_ADD, ( [ site.base + '/post/50' ], [ hash_of( '/files/p1.jpg' ) ] ) )

            session.write( 'content_updates', ClientContentUpdates.ContentUpdatePackage.STATICCreateFromContentUpdates( CC.HYDRUS_LOCAL_FILE_STORAGE_SERVICE_KEY, [ update ] ) )

        elif setup == 'delete':

            update = ClientContentUpdates.ContentUpdate( HC.CONTENT_TYPE_FILES, HC.CONTENT_UPDATE_DELETE, { hash_of( '/files/p2.jpg' ) } )

            session.write( 'content_updates', ClientContentUpdates.ContentUpdatePackage.STATICCreateFromContentUpdates( CC.LOCAL_FILE_SERVICE_KEY, [ update ] ) )


        prefetch = PrefetchImportOptions.PrefetchImportOptions()
        prefetch.SetPreImportHashCheckType( hash_check )
        prefetch.SetPreImportURLCheckType( url_check )
        prefetch.SetPreImportURLCheckLooksForNeighbourSpam( neighbour )
        prefetch.SetShouldFetchMetadataEvenIfHashKnownAndFileAlreadyInDB( fetch_hash )
        prefetch.SetShouldFetchMetadataEvenIfURLKnownAndFileAlreadyInDB( fetch_url )

        container = ImportOptionsContainer.ImportOptionsContainer()
        container.SetImportOptions( prefetch )

        if not exclude_deleted:

            from hydrus.client.importing.options import FileFilteringImportOptions

            filtering = FileFilteringImportOptions.FileFilteringImportOptions()
            filtering.SetExcludesDeleted( False )
            container.SetImportOptions( filtering )


        # (the page is made empty and then given the options, before the URL)
        current.clear()

        page_name = f'step {len( recorded )}'

        controller.CallBlockingToQt( controller.gui, lambda: controller.gui._notebook.NewPageImportURLs( page_name = page_name ) )

        importer = new_importer()

        importer.SetImportOptionsContainer( container )

        current.append( importer )

        site.take_hits()

        if setup == 'preset_hash':

            from hydrus.client.importing import ClientImporting
            from hydrus.client.importing import ClientImportFileSeeds

            file_seed = ClientImportFileSeeds.FileSeed( ClientImportFileSeeds.FILE_SEED_TYPE_URL, site.base + path )
            file_seed.SetHash( hash_of( '/files/p1.jpg' ) )

            importer.GetFileSeedCache().AddFileSeeds( [ file_seed ] )

            ClientImporting.WakeRepeatingJob( importer._files_repeating_job )

        else:

            add( path, page_name )


        wait_until( name, lambda: idle( 1 ) )

        time.sleep( 0.5 )

        # (the file's history is put back for the next step)
        recorded.append( dict(
            name = name,
            setup = setup,
            path = path,
            options = dict( hash_check = hash_check, url_check = url_check, neighbour_spam = neighbour, fetch_hash = fetch_hash, fetch_url = fetch_url, exclude_deleted = exclude_deleted ),
            hits = site.take_hits(),
            seed = seed_record(),
        ) )

        if setup == 'extra_url':

            update = ClientContentUpdates.ContentUpdate( HC.CONTENT_TYPE_URLS, HC.CONTENT_UPDATE_DELETE, ( [ site.base + '/post/50' ], [ hash_of( '/files/p1.jpg' ) ] ) )

            session.write( 'content_updates', ClientContentUpdates.ContentUpdatePackage.STATICCreateFromContentUpdates( CC.HYDRUS_LOCAL_FILE_STORAGE_SERVICE_KEY, [ update ] ) )


    return recorded


def main():

    work = tempfile.mkdtemp( prefix = 'hydrus_prefetch_' )

    phases = [ build_phase() ]

    try:

        with flows.Site( SITE_PORT, phases ) as site:

            result = hydrus_driver.run_client( os.path.join( work, 'db' ), lambda session: run( session, site ), port = API_PORT, network = True )


    finally:

        shutil.rmtree( work, ignore_errors = True )


    output = { 'site_port' : SITE_PORT, 'pages' : sorted( phases[ 0 ][ 'pages' ] ), 'scenarios' : result }

    with open( OUT, 'w' ) as f:

        json.dump( output, f, indent = 1, sort_keys = True, ensure_ascii = False )
        f.write( '\n' )


    print( f'wrote {OUT}' )


if __name__ == '__main__':

    main()

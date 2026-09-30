#!/usr/bin/env python3
"""Record the reference's downloader against a local fake site.

A small booru (posts with tags, a file and a source link; tag searches of
three posts a page, newest first) and imageboard (a thread of file links)
are served over localhost in three phases:

0. seven "blue eyes" posts, four "red hair" posts, a thread with two files
1. two more "blue eyes" posts, and a third file in the thread (the same
   image as a post already downloaded)
2. the thread is gone (404)

URL classes, page parsers and a gallery URL generator for the site are
built with the reference's own classes and added to a fresh client, which
then, in order, waiting for each to finish:

* phase 0: gets URLs through the Client API's /add_urls/add_url into a URL
  page ("urls"): two posts (one with a filterable tag), a missing post, a
  direct file and a search page; then a thread URL (a watcher); then makes
  a subscription to "blue_eyes" with an initial file limit of four
* phase 1: checks the watcher and the subscription again
* phase 2: checks the watcher again

What each importer recorded (file seeds, gallery seeds, the watcher's and
the query's state) and what every downloaded file got (tags, URLs) is
written with the site and the definitions to oracle/fixtures/downloads.json,
for hydrus-download's end-to-end test to do the same and compare.

Usage: QT_QPA_PLATFORM=offscreen python oracle/record_downloads.py
"""

import http.server
import json
import os
import re
import shutil
import sys
import tempfile
import threading
import time

sys.path.insert( 0, os.path.dirname( __file__ ) )

import hydrus_driver

HERE = os.path.dirname( os.path.abspath( __file__ ) )
MEDIA_DIR = os.path.join( HERE, 'fixtures', 'media' )
OUT = os.path.join( HERE, 'fixtures', 'downloads.json' )

SITE_PORT = 45977
HOST = f'127.0.0.1:{SITE_PORT}'
BASE = f'http://{HOST}'
API_PORT = 45978

BLUE = [ 1, 2, 3, 4, 5, 6, 7 ]
BLUE_LATER = [ 8, 9 ]
RED = [ 20, 21, 22, 23 ]

IMAGES = [
    'jpeg_420.jpg', 'jpeg_422.jpg', 'jpeg_444_q95.jpg', 'jpeg_flat.jpg', 'jpeg_gray.jpg', 'jpeg_orient2.jpg',
    'jpeg_orient3.jpg', 'jpeg_orient4.jpg', 'jpeg_orient5.jpg', 'jpeg_orient6.jpg', 'jpeg_orient7.jpg',
    'jpeg_orient8.jpg', 'jpeg_mirror.jpg', 'jpeg_tiny.jpg', 'jpeg_wide.jpg', 'jpeg_tall.jpg', 'jpeg_icc_srgb.jpg',
]

# ---------------------------------------------------------------- the site

def post_tags( pid ):

    tags = [ f'post {pid}', 'creator:someone' ]

    if pid in BLUE or pid in BLUE_LATER:

        tags.append( 'blue eyes' )

    if pid in RED:

        tags.append( 'red hair' )


    return tags


def post_page( pid ):

    items = ''.join( f'<li class="tag">{t}</li>' for t in post_tags( pid ) )

    return (
        f'<html><head><title>post {pid}</title></head><body>'
        f'<ul id="tags">{items}</ul>'
        f'<img id="image" src="/files/p{pid}.jpg">'
        f'<a class="source" href="https://elsewhere.example/art/{pid}">source</a>'
        '</body></html>'
    )


def search_pages( tag, posts ):

    newest_first = list( reversed( posts ) )

    pages = {}

    for page in range( 1, max( 1, ( len( newest_first ) + 2 ) // 3 ) + 1 ):

        on_page = newest_first[ ( page - 1 ) * 3 : page * 3 ]

        html = '<html><body>' + ''.join( f'<a class="thumb" href="/post/{p}">post {p}</a>' for p in on_page )

        if len( newest_first ) > page * 3:

            html += f'<a class="next" href="/search/{tag}/{page + 1}">next</a>'


        html += '</body></html>'

        pages[ f'/search/{tag}/{page}' ] = html


    return pages


def thread_page( tid, files ):

    return (
        f'<html><head><title>thread {tid}: a thread about eyes</title></head><body>'
        + ''.join( f'<a class="file" href="/files/{f}.jpg">{f}</a>' for f in files )
        + '</body></html>'
    )


def build_phases():

    phases = []

    for phase in range( 3 ):

        blue = BLUE + ( BLUE_LATER if phase >= 1 else [] )

        pages = {}
        files = {}

        for pid in blue + RED:

            pages[ f'/post/{pid}' ] = post_page( pid )
            files[ f'/files/p{pid}.jpg' ] = IMAGES[ ( pid - 1 ) % len( IMAGES ) ] if pid < 20 else IMAGES[ 9 + pid - 20 ]


        pages.update( search_pages( 'blue_eyes', blue ) )
        pages.update( search_pages( 'red_hair', RED ) )

        thread_files = [ 't1', 't2' ] + ( [ 't3' ] if phase >= 1 else [] )

        if phase < 2:

            pages[ '/thread/5' ] = thread_page( 5, thread_files )


        files[ '/files/t1.jpg' ] = IMAGES[ 13 ]
        files[ '/files/t2.jpg' ] = IMAGES[ 14 ]
        # the same image as post 1's
        files[ '/files/t3.jpg' ] = IMAGES[ 0 ]
        # a file linked directly
        files[ '/files/direct.jpg' ] = IMAGES[ 16 ]

        phases.append( { 'pages' : pages, 'files' : files } )


    return phases


class Site( object ):

    def __init__( self, phases ):

        self.phases = phases
        self.phase = 0
        self.hits = []


    def serve( self ):

        site = self

        class Handler( http.server.BaseHTTPRequestHandler ):

            def do_GET( self ):

                path = self.path.split( '?' )[0]

                site.hits.append( ( site.phase, path ) )

                phase = site.phases[ site.phase ]

                if path in phase[ 'pages' ]:

                    body = phase[ 'pages' ][ path ].encode( 'utf-8' )
                    content_type = 'text/html; charset=utf-8'

                elif path in phase[ 'files' ]:

                    with open( os.path.join( MEDIA_DIR, phase[ 'files' ][ path ] ), 'rb' ) as f:

                        body = f.read()


                    content_type = 'image/jpeg'

                else:

                    self.send_response( 404 )
                    self.send_header( 'Content-Type', 'text/html; charset=utf-8' )
                    self.end_headers()
                    self.wfile.write( b'not found' )

                    return


                self.send_response( 200 )
                self.send_header( 'Content-Type', content_type )
                self.send_header( 'Content-Length', str( len( body ) ) )
                self.end_headers()
                self.wfile.write( body )


            def log_message( self, *args ):

                pass



        server = http.server.ThreadingHTTPServer( ( '127.0.0.1', SITE_PORT ), Handler )

        threading.Thread( target = server.serve_forever, daemon = True ).start()

        return server



# ---------------------------------------------------------------- definitions

def definitions():

    from hydrus.core import HydrusConstants as HC
    from hydrus.client import ClientStrings as S
    from hydrus.client.networking import ClientNetworkingGUG as G
    from hydrus.client.networking import ClientNetworkingURLClass as U
    from hydrus.client.parsing import ClientParsing as P

    def fixed( text ):

        return S.StringMatch( match_type = S.STRING_MATCH_FIXED, match_value = text, example_string = text )


    def url_class( name, url_type, path, example ):

        return U.URLClass(
            name,
            url_type = url_type,
            preferred_scheme = 'http',
            url_domain_mask = U.URLDomainMask( raw_domains = [ HOST ] ),
            path_components = [ ( fixed( path[ 0 ] ), None ) ] + [ ( S.StringMatch(), None ) for _ in path[ 1: ] ],
            parameters = [],
            example_url = BASE + example
        )


    def html( tag, attrs, content, attribute = None ):

        return P.ParseFormulaHTML(
            tag_rules = [ P.ParseRuleHTML( rule_type = P.HTML_RULE_TYPE_DESCENDING, tag_name = tag, tag_attributes = attrs ) ],
            content_to_fetch = content,
            attribute_to_fetch = attribute
        )


    def urls( name, tag, attrs, attribute, url_type ):

        return P.ContentParser( name = name, content_type = HC.CONTENT_TYPE_URLS, formula = html( tag, attrs, P.HTML_CONTENT_ATTRIBUTE, attribute ), additional_info = ( url_type, 50 ) )


    url_classes = [
        url_class( 'local booru post', HC.URL_TYPE_POST, [ 'post', 'id' ], '/post/1' ),
        url_class( 'local booru search', HC.URL_TYPE_GALLERY, [ 'search', 'tags', 'page' ], '/search/blue_eyes/1' ),
        url_class( 'local board thread', HC.URL_TYPE_WATCHABLE, [ 'thread', 'id' ], '/thread/5' ),
    ]

    parsers = [
        P.PageParser(
            'local booru post parser',
            content_parsers = [
                P.ContentParser( name = 'tags', content_type = HC.CONTENT_TYPE_MAPPINGS, formula = html( 'li', { 'class' : 'tag' }, P.HTML_CONTENT_STRING ), additional_info = None ),
                urls( 'file', 'img', { 'id' : 'image' }, 'src', HC.URL_TYPE_DESIRED ),
                urls( 'source', 'a', { 'class' : 'source' }, 'href', HC.URL_TYPE_SOURCE ),
            ],
            example_urls = [ BASE + '/post/1' ]
        ),
        P.PageParser(
            'local booru search parser',
            content_parsers = [
                urls( 'posts', 'a', { 'class' : 'thumb' }, 'href', HC.URL_TYPE_DESIRED ),
                urls( 'next page', 'a', { 'class' : 'next' }, 'href', HC.URL_TYPE_NEXT ),
            ],
            example_urls = [ BASE + '/search/blue_eyes/1' ]
        ),
        P.PageParser(
            'local board thread parser',
            content_parsers = [
                urls( 'files', 'a', { 'class' : 'file' }, 'href', HC.URL_TYPE_DESIRED ),
                P.ContentParser( name = 'title', content_type = HC.CONTENT_TYPE_TITLE, formula = html( 'title', {}, P.HTML_CONTENT_STRING ), additional_info = 0 ),
            ],
            example_urls = [ BASE + '/thread/5' ]
        ),
    ]

    gugs = [
        G.GalleryURLGenerator(
            'local booru tag search',
            url_template = BASE + '/search/%tags%/1',
            replacement_phrase = '%tags%',
            search_terms_separator = '+',
            initial_search_text = 'tag',
            example_search_text = 'blue_eyes'
        ),
    ]

    return ( url_classes, parsers, gugs )


# ---------------------------------------------------------------- recording

def normalise_note( note ):

    note = note.replace( BASE, 'BASE' )
    note = re.sub( r'\d{4}-\d{2}-\d{2} \d{2}:\d{2}:\d{2}', 'TIME', note )
    note = re.sub( r'which was .*? ago', 'which was X ago', note )

    return note


def file_seed_record( f ):

    hashes = { t : h.hex() for ( t, h ) in f._hashes.items() if h is not None }

    return {
        'data' : f.file_seed_data.replace( BASE, 'BASE' ),
        'status' : f.status,
        'note' : normalise_note( f.note ),
        'referral' : None if f._referral_url is None else f._referral_url.replace( BASE, 'BASE' ),
        'primary_urls' : sorted( u.replace( BASE, 'BASE' ) for u in f._primary_urls ),
        'source_urls' : sorted( u.replace( BASE, 'BASE' ) for u in f._source_urls ),
        'tags' : sorted( f._tags ),
        'external_filterable_tags' : sorted( f._external_filterable_tags ),
        'sha256' : hashes.get( 'sha256' ),
    }


def gallery_seed_record( g ):

    return {
        'url' : g.url.replace( BASE, 'BASE' ),
        'status' : g.status,
        'note' : normalise_note( g.note ),
    }


def log_record( file_seed_cache, gallery_seed_log ):

    return {
        'files' : [ file_seed_record( f ) for f in file_seed_cache.GetFileSeeds() ],
        'galleries' : [ gallery_seed_record( g ) for g in gallery_seed_log.GetGallerySeeds() ],
    }


def file_records( session, hashes ):

    from hydrus.client import ClientConstants as CC
    from hydrus.client.metadata import ClientTags

    records = {}

    for hash_hex in sorted( hashes ):

        media_result = session.read( 'media_result', bytes.fromhex( hash_hex ) )

        tags_manager = media_result.GetTagsManager()

        records[ hash_hex ] = {
            'downloader_tags' : sorted( tags_manager.GetCurrent( CC.DEFAULT_LOCAL_DOWNLOADER_TAG_SERVICE_KEY, ClientTags.TAG_DISPLAY_STORAGE ) ),
            'my_tags' : sorted( tags_manager.GetCurrent( CC.DEFAULT_LOCAL_TAG_SERVICE_KEY, ClientTags.TAG_DISPLAY_STORAGE ) ),
            'urls' : sorted( u.replace( BASE, 'BASE' ) for u in media_result.GetLocationsManager().GetURLs() ),
        }


    return records


# ---------------------------------------------------------------- the run

def wait_until( what, test, timeout = 300 ):

    deadline = time.time() + timeout

    while time.time() < deadline:

        if test():

            return


        time.sleep( 0.5 )


    raise Exception( f'timed out waiting for {what}' )


def run( session, site ):

    from hydrus.client import ClientConstants as CC
    from hydrus.core import HydrusSerialisable
    from hydrus.client.importing import ClientImportSubscriptionQuery
    from hydrus.client.importing import ClientImportSubscriptions

    controller = session.controller

    ( url_classes, parsers, gugs ) = definitions()

    domain_manager = controller.network_engine.domain_manager

    domain_manager.SetURLClasses( url_classes )
    domain_manager.SetParsers( parsers )
    domain_manager.SetGUGs( gugs )
    domain_manager.TryToLinkURLClassesAndParsers()

    # as a migration would find it
    serialised_domain_manager = list( domain_manager.GetSerialisableTuple() )

    def media_pages():

        return controller.CallBlockingToQt( controller.gui, lambda: controller.gui._notebook.GetMediaPages() )


    def page_variable( name ):

        for page in media_pages():

            manager = page.GetPageManager()

            if manager.HasVariable( name ):

                return manager.GetVariable( name )



        return None


    def urls_idle():

        urls_import = page_variable( 'urls_import' )

        if urls_import is None:

            return False


        ( file_seed_cache, gallery_seed_log ) = urls_import.GetFileSeedCache(), urls_import.GetGallerySeedLog()

        return file_seed_cache.GetFileSeedCount( CC.STATUS_UNKNOWN ) == 0 and not gallery_seed_log.WorkToDo() and file_seed_cache.GetFileSeedCount() > 0


    def watcher():

        multiple = page_variable( 'multiple_watcher_import' )

        if multiple is None or len( multiple.GetWatchers() ) == 0:

            return None


        return multiple.GetWatchers()[0]


    def watcher_idle( checks ):

        w = watcher()

        if w is None:

            return False


        galleries = w.GetGallerySeedLog().GetGallerySeeds()

        checked = len( [ g for g in galleries if g.status != CC.STATUS_UNKNOWN ] )

        return checked >= checks and w.GetFileSeedCache().GetFileSeedCount( CC.STATUS_UNKNOWN ) == 0


    # phase 0 -------------------------------------------------------------

    for ( url, extra ) in [
        ( '/post/1', {} ),
        ( '/post/6', { 'filterable_tags' : [ 'From Companion' ] } ),
        ( '/post/404', {} ),
        ( '/files/direct.jpg', {} ),
        ( '/search/red_hair/1', {} ),
    ]:

        session.api.post( '/add_urls/add_url', dict( url = BASE + url, destination_page_name = 'urls', **extra ) )

        # one at a time, so the order is fixed
        time.sleep( 0.5 )


    wait_until( 'the url page', urls_idle )

    session.api.post( '/add_urls/add_url', { 'url' : BASE + '/thread/5' } )

    wait_until( 'the watcher', lambda: watcher_idle( 1 ) )

    gug = gugs[0]

    subscription = ClientImportSubscriptions.Subscription( 'blue eyes sub', gug_key_and_name = ( gug.GetGUGKey(), gug.GetName() ) )

    header = ClientImportSubscriptionQuery.SubscriptionQueryHeader()
    header.SetQueryText( 'blue_eyes' )

    container = ClientImportSubscriptionQuery.SubscriptionQueryLogContainer( header.GetQueryLogContainerName() )

    subscription.SetQueryHeaders( [ header ] )
    subscription.SetFileLimits( 4, 100 )

    session.write( 'serialisable_atomic', overwrite_types_and_objs = ( [ HydrusSerialisable.SERIALISABLE_TYPE_SUBSCRIPTION ], [ subscription ] ), set_objs = [ container ] )

    controller.subscriptions_manager.SetSubscriptions( [ subscription ] )

    def subscription_idle( checks_after ):

        sub = controller.subscriptions_manager.GetSubscriptions()[0]

        h = sub.GetQueryHeaders()[0]

        running = len( controller.subscriptions_manager._names_to_running_subscription_info ) > 0

        return not running and h.GetLastCheckTime() > checks_after and not h.HasFileWorkToDo()


    wait_until( 'the subscription', lambda: subscription_idle( 0 ) )

    # phase 1 -------------------------------------------------------------

    site.phase = 1

    first_sub_check = controller.subscriptions_manager.GetSubscriptions()[0].GetQueryHeaders()[0].GetLastCheckTime()

    watcher().CheckNow()

    wait_until( 'the watcher again', lambda: watcher_idle( 2 ) )

    # (a check in the same second would not show as a new check)
    time.sleep( 1.1 )

    sub = controller.subscriptions_manager.GetSubscriptions()[0]
    sub.CheckNow()
    # (as the edit dialog does: the manager recalculates when it next works)
    controller.subscriptions_manager.SetSubscriptions( [ sub ] )

    wait_until( 'the subscription again', lambda: subscription_idle( first_sub_check ) )

    # phase 2 -------------------------------------------------------------

    site.phase = 2

    watcher().CheckNow()

    wait_until( 'the watcher to 404', lambda: watcher_idle( 3 ) and watcher().GetCheckingStatus() != 0 )

    # recording -----------------------------------------------------------

    urls_import = page_variable( 'urls_import' )
    w = watcher()
    sub = controller.subscriptions_manager.GetSubscriptions()[0]
    h = sub.GetQueryHeaders()[0]
    log = session.read( 'serialisable_named', HydrusSerialisable.SERIALISABLE_TYPE_SUBSCRIPTION_QUERY_LOG_CONTAINER, h.GetQueryLogContainerName() )

    recorded = {
        'url_page' : log_record( urls_import.GetFileSeedCache(), urls_import.GetGallerySeedLog() ),
        'watcher' : dict(
            log_record( w.GetFileSeedCache(), w.GetGallerySeedLog() ),
            subject = w.GetSubject(),
            checking_status = w.GetCheckingStatus(),
        ),
        'subscription' : dict(
            log_record( log.GetFileSeedCache(), log.GetGallerySeedLog() ),
            dead = h.IsDead(),
            paused = h.IsPaused(),
        ),
    }

    hashes = set()

    for importer in recorded.values():

        hashes.update( f[ 'sha256' ] for f in importer[ 'files' ] if f[ 'sha256' ] is not None )


    recorded[ 'files' ] = file_records( session, hashes )
    recorded[ 'hits' ] = [ [ phase, path ] for ( phase, path ) in site.hits ]

    return {
        'domain_manager' : serialised_domain_manager,
        'definitions' : {
            'url_classes' : [ list( u.GetSerialisableTuple() ) for u in url_classes ],
            'parsers' : [ list( p.GetSerialisableTuple() ) for p in parsers ],
            'gugs' : [ list( g.GetSerialisableTuple() ) for g in gugs ],
        },
        'recorded' : recorded,
    }


def main():

    phases = build_phases()

    site = Site( phases )
    server = site.serve()

    work = tempfile.mkdtemp( prefix = 'hydrus_downloads_' )

    try:

        result = hydrus_driver.run_client( os.path.join( work, 'db' ), lambda session: run( session, site ), port = API_PORT, network = True )

    finally:

        server.shutdown()
        shutil.rmtree( work, ignore_errors = True )


    output = {
        'site_port' : SITE_PORT,
        'phases' : phases,
        'domain_manager' : result[ 'domain_manager' ],
        'definitions' : result[ 'definitions' ],
        'recorded' : result[ 'recorded' ],
    }

    with open( OUT, 'w' ) as f:

        json.dump( output, f, indent = 1, sort_keys = True, ensure_ascii = False )
        f.write( '\n' )


    print( f'wrote {OUT}' )


if __name__ == '__main__':

    main()

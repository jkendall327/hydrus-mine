"""The harness for end-to-end user flows recorded in the reference.

A flow drives the running reference client through several steps a user
would take (through its real dialogs, with each modal answered by a script)
against a small fake site served over localhost, and records what the user
can see after each step. `docs/rust/FLOWS.md` says how to add one; the
hydrus-rs side replays the same steps in its headless GUI
(`crates/hydrus-gui/tests/gui/flows/`) and compares.

This module holds the parts every flow shares:

* `Site`: a booru served in phases (posts with tags and a file, tag
  searches three posts a page, newest first), with every request noted;
* `definitions`: URL classes, parsers and a gallery URL generator for it,
  built with the reference's own classes;
* `scripted_dialogs`: `DialogEdit.exec` replaced by a script that acts on
  each dialog as it opens (by its panel's class), then accepts or cancels;
* `wait_until_idle`: until the subscriptions manager has nothing running or
  due, the network engine has no jobs and the database no queued work, for
  a moment together;
* `popups_shown`: the popups the user sees at the end of a step;
* `state`: the normalised end state (subscriptions and their queries'
  logs, the files the flow touched with their tags, URLs and inbox state,
  popups and the site's requests), and `normalise_text`, which holds
  the comparison to what does not depend on when it ran.
"""

import http.server
import json
import os
import re
import threading
import time

HERE = os.path.dirname( os.path.abspath( __file__ ) )
MEDIA_DIR = os.path.join( HERE, 'fixtures', 'media' )

IMAGES = [
    'jpeg_420.jpg', 'jpeg_422.jpg', 'jpeg_444_q95.jpg', 'jpeg_flat.jpg', 'jpeg_gray.jpg', 'jpeg_orient2.jpg',
    'jpeg_orient3.jpg', 'jpeg_orient4.jpg', 'jpeg_orient5.jpg', 'jpeg_orient6.jpg', 'jpeg_orient7.jpg',
    'jpeg_orient8.jpg', 'jpeg_mirror.jpg', 'jpeg_tiny.jpg', 'jpeg_wide.jpg', 'jpeg_tall.jpg', 'jpeg_icc_srgb.jpg',
]

# ---------------------------------------------------------------- the site

def post_tags( pid, searches ):

    tags = [ f'post {pid}', 'creator:someone' ]

    for ( tag, posts ) in searches.items():

        if pid in posts:

            tags.append( tag.replace( '_', ' ' ) )



    return tags


def post_page( pid, searches ):

    items = ''.join( f'<li class="tag">{t}</li>' for t in post_tags( pid, searches ) )

    return (
        f'<html><head><title>post {pid}</title></head><body>'
        f'<ul id="tags">{items}</ul>'
        f'<img id="image" src="/files/p{pid}.jpg">'
        f'<a class="source" href="https://elsewhere.example/art/{pid}">source</a>'
        '</body></html>'
    )


def search_pages( tag, posts ):

    newest_first = sorted( posts, reverse = True )

    pages = {}

    for page in range( 1, max( 1, ( len( newest_first ) + 2 ) // 3 ) + 1 ):

        on_page = newest_first[ ( page - 1 ) * 3 : page * 3 ]

        html = '<html><body>' + ''.join( f'<a class="thumb" href="/post/{p}">post {p}</a>' for p in on_page )

        if len( newest_first ) > page * 3:

            html += f'<a class="next" href="/search/{tag}/{page + 1}">next</a>'


        html += '</body></html>'

        pages[ f'/search/{tag}/{page}' ] = html


    return pages


def booru_phase( searches ):
    """A phase of the booru: `searches` maps a search tag ("blue_eyes") to
    the post ids it finds. Each post's file is one of the media fixtures."""

    pages = {}
    files = {}

    posts = sorted( { p for ps in searches.values() for p in ps } )

    for pid in posts:

        pages[ f'/post/{pid}' ] = post_page( pid, searches )
        files[ f'/files/p{pid}.jpg' ] = IMAGES[ ( pid - 1 ) % len( IMAGES ) ]


    for ( tag, ps ) in searches.items():

        pages.update( search_pages( tag, ps ) )


    return { 'pages' : pages, 'files' : files }


class Site( object ):
    """The fake site, in phases; `phase` says which one is served."""

    def __init__( self, port, phases ):

        self.port = port
        self.host = f'127.0.0.1:{port}'
        self.base = f'http://{self.host}'
        self.phases = phases
        self.phase = 0
        self.hits = []
        self._server = None


    def __enter__( self ):

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



        self._server = http.server.ThreadingHTTPServer( ( '127.0.0.1', self.port ), Handler )

        threading.Thread( target = self._server.serve_forever, daemon = True ).start()

        return self


    def __exit__( self, *args ):

        self._server.shutdown()
        self._server.server_close()


    def take_hits( self ):
        """The paths requested since last asked, sorted (the reference works
        a query's pages and files side by side, so their order can vary)."""

        hits = sorted( path for ( phase, path ) in self.hits )

        self.hits.clear()

        return hits


    def describe( self ):

        return { 'port' : self.port, 'phases' : self.phases }



# ---------------------------------------------------------------- definitions

def definitions( site ):
    """URL classes, parsers and a GUG ("local booru tag search") for the
    booru, as a user would import them."""

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
            url_domain_mask = U.URLDomainMask( raw_domains = [ site.host ] ),
            path_components = [ ( fixed( path[ 0 ] ), None ) ] + [ ( S.StringMatch(), None ) for _ in path[ 1: ] ],
            parameters = [],
            example_url = site.base + example
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
    ]

    parsers = [
        P.PageParser(
            'local booru post parser',
            content_parsers = [
                P.ContentParser( name = 'tags', content_type = HC.CONTENT_TYPE_MAPPINGS, formula = html( 'li', { 'class' : 'tag' }, P.HTML_CONTENT_STRING ), additional_info = None ),
                urls( 'file', 'img', { 'id' : 'image' }, 'src', HC.URL_TYPE_DESIRED ),
                urls( 'source', 'a', { 'class' : 'source' }, 'href', HC.URL_TYPE_SOURCE ),
            ],
            example_urls = [ site.base + '/post/1' ]
        ),
        P.PageParser(
            'local booru search parser',
            content_parsers = [
                urls( 'posts', 'a', { 'class' : 'thumb' }, 'href', HC.URL_TYPE_DESIRED ),
                urls( 'next page', 'a', { 'class' : 'next' }, 'href', HC.URL_TYPE_NEXT ),
            ],
            example_urls = [ site.base + '/search/blue_eyes/1' ]
        ),
    ]

    gugs = [
        G.GalleryURLGenerator(
            'local booru tag search',
            url_template = site.base + '/search/%tags%/1',
            replacement_phrase = '%tags%',
            search_terms_separator = '+',
            initial_search_text = 'tag',
            example_search_text = 'blue_eyes'
        ),
    ]

    return ( url_classes, parsers, gugs )


def install( controller, site ):
    """Give the client the site's downloader and let it reach the network
    (the fixture was made with new traffic paused). Returns the domain
    manager as serialised, for hydrus-rs to install as a migration would."""

    ( url_classes, parsers, gugs ) = definitions( site )

    domain_manager = controller.network_engine.domain_manager

    domain_manager.SetURLClasses( url_classes )
    domain_manager.SetParsers( parsers )
    domain_manager.SetGUGs( gugs )
    domain_manager.SetGUGKeysToDisplay( [ g.GetGUGKey() for g in gugs ] )
    domain_manager.TryToLinkURLClassesAndParsers()

    # (as network > pause > "all new network traffic" unpauses it)
    if controller.network_engine._pause_all_new_network_traffic:

        controller.network_engine.PausePlayNewJobs()


    controller.new_options.SetBoolean( 'pause_subs_sync', False )
    # (how long it takes doesn't matter, only what it fetches)
    controller.new_options.SetInteger( 'gallery_page_wait_period_subscriptions', 0 )

    return list( domain_manager.GetSerialisableTuple() )


# ---------------------------------------------------------------- dialogs

class scripted_dialogs( object ):
    """Within, each `DialogEdit` that opens is handed (once its modal loop
    runs) to the next script for its panel's class name: `scripts` maps a
    panel class name to a list of functions `( dialog, panel ) -> bool`,
    used in turn; True accepts the dialog, False cancels it. What each
    script saw can be noted in `seen`, a list it appends to."""

    def __init__( self, scripts ):

        self.scripts = { name : list( fs ) for ( name, fs ) in scripts.items() }
        self.errors = []


    def __enter__( self ):

        from qtpy import QtCore as QC
        from hydrus.client.gui import ClientGUITopLevelWindowsPanels as Windows

        self._real_exec = Windows.DialogEdit.exec

        real_exec = self._real_exec
        scripts = self.scripts
        self_ = self

        def execute( dlg ):

            panel = dlg._panel
            name = type( panel ).__name__

            if name not in scripts or len( scripts[ name ] ) == 0:

                raise Exception( f'no script for a {name} dialog ("{dlg.windowTitle()}")' )


            script = scripts[ name ].pop( 0 )

            def act():

                try:

                    accepted = script( dlg, panel )

                except Exception as e:

                    # (cancelled, so the flow stops rather than hangs)
                    self_.errors.append( e )

                    accepted = False


                if accepted:

                    dlg.accept()

                else:

                    dlg.reject()



            QC.QTimer.singleShot( 0, act )

            return real_exec( dlg )


        Windows.DialogEdit.exec = execute

        return self


    def check( self ):
        """True, or the first error a script raised."""

        if len( self.errors ) > 0:

            raise self.errors[0]


        return True


    def __exit__( self, *args ):

        from hydrus.client.gui import ClientGUITopLevelWindowsPanels as Windows

        Windows.DialogEdit.exec = self._real_exec

        if len( self.errors ) > 0 and args[0] is None:

            raise self.errors[0]

        left = { name : len( fs ) for ( name, fs ) in self.scripts.items() if len( fs ) > 0 }

        if len( left ) > 0 and args[0] is None:

            raise Exception( f'dialogs scripted but never opened: {left}' )




# ---------------------------------------------------------------- waiting

def subscriptions_idle( controller ):
    """Nothing running, and nothing due (a subscription made or told to
    check now is due at once)."""

    from hydrus.core import HydrusTime

    manager = controller.subscriptions_manager

    if manager.SubscriptionsRunning():

        return False


    for subscription in manager.GetSubscriptions():

        if not subscription.IsExpectingToWorkInFuture():

            continue


        next_work_time = subscription.GetBestEarliestNextWorkTime()

        if next_work_time is not None and next_work_time < HydrusTime.GetNow() + 60:

            return False



    return True


def network_idle( controller ):

    engine = controller.network_engine

    with engine._lock:

        return len( engine._jobs_awaiting_validity ) + len( engine._jobs_awaiting_bandwidth ) + len( engine._jobs_awaiting_login ) + len( engine._jobs_awaiting_slot ) + len( engine._jobs_running ) == 0



def wait_until_idle( controller, also = None, settle = 1.5, timeout = 300 ):
    """Wait until the client's workers have nothing left to do: the
    subscriptions manager (`subscriptions_idle`), the network engine and the
    database's job queue, and `also()` if given, all at once for `settle`
    seconds running."""

    deadline = time.time() + timeout
    quiet_since = None

    while time.time() < deadline:

        quiet = subscriptions_idle( controller ) and network_idle( controller ) and controller.db.JobsQueueEmpty() and ( also is None or also() )

        if quiet:

            if quiet_since is None:

                quiet_since = time.time()

            elif time.time() - quiet_since >= settle:

                return


        else:

            quiet_since = None


        time.sleep( 0.1 )


    raise Exception( 'timed out waiting for the client to go idle' )


# ---------------------------------------------------------------- popups

def popups_shown( controller, normalise ):
    """The popups the popup message manager holds, not dismissed, oldest
    first: what the user sees (or would, scrolling its summary). A file
    popup merged into an earlier one of the same label never joins it."""

    from hydrus.core import HydrusNumbers

    queue = controller.job_status_popup_queue

    with queue._lock:

        job_statuses = [ j for j in queue._job_status_ordered_dict_queue.values() if not j.IsDismissed() ]


    out = []

    for job_status in job_statuses:

        files = job_status.GetFiles()

        out.append( {
            'title' : normalise( job_status.GetStatusTitle() or '' ),
            'text_1' : normalise( job_status.GetStatusText( 1 ) or '' ),
            'text_2' : normalise( job_status.GetStatusText( 2 ) or '' ),
            # (its button, as `PopupMessage.UpdateMessage` labels it)
            'files' : None if files is None else '{} - show {} files'.format( files[1], HydrusNumbers.ToHumanInt( len( files[0] ) ) ),
            'error' : job_status.HadError(),
        } )


    return out


# ---------------------------------------------------------------- the end state

# ("now" within three seconds, else "5 seconds ago": the same, a moment apart)
TIME_DELTA = re.compile( r'\b(\d+ (year|month|day|hour|minute|second)s?( \d+ (year|month|day|hour|minute|second)s?)*( ago)?|just now|now)\b' )
DATE_TIME = re.compile( r'\d{4}-\d{2}-\d{2} \d{2}:\d{2}:\d{2}' )


def normaliser( site ):
    """What makes two runs' texts comparable: the site's address is BASE,
    dates are TIME and times just past ("now", "5 minutes ago") are DELTA."""

    def normalise( text ):

        if text is None:

            return None


        text = text.replace( site.base, 'BASE' )
        text = DATE_TIME.sub( 'TIME', text )
        text = TIME_DELTA.sub( 'DELTA', text )

        return text


    return normalise


def file_seed_state( f, normalise ):

    hashes = { t : h.hex() for ( t, h ) in f._hashes.items() if h is not None }

    return {
        'data' : normalise( f.file_seed_data ),
        'status' : f.status,
        'note' : normalise( f.note ),
        'tags' : sorted( f._tags ),
        'sha256' : hashes.get( 'sha256' ),
    }


def gallery_seed_state( g, normalise ):

    return {
        'url' : normalise( g.url ),
        'status' : g.status,
        'note' : normalise( g.note ),
    }


def subscriptions_state( session, normalise ):
    """Each subscription as saved: its downloader, paused, and each query
    with its file and search logs."""

    from hydrus.core import HydrusSerialisable

    out = []

    for subscription in sorted( session.controller.subscriptions_manager.GetSubscriptions(), key = lambda s: s.GetName() ):

        queries = []

        for header in subscription.GetQueryHeaders():

            log = session.read( 'serialisable_named', HydrusSerialisable.SERIALISABLE_TYPE_SUBSCRIPTION_QUERY_LOG_CONTAINER, header.GetQueryLogContainerName() )

            queries.append( {
                'text' : header.GetQueryText(),
                'paused' : header.IsPaused(),
                'dead' : header.IsDead(),
                'checked' : header.GetLastCheckTime() > 0,
                'check_now' : header.IsCheckingNow(),
                'files' : [ file_seed_state( f, normalise ) for f in log.GetFileSeedCache().GetFileSeeds() ],
                'searches' : [ gallery_seed_state( g, normalise ) for g in log.GetGallerySeedLog().GetGallerySeeds() ],
            } )


        out.append( {
            'name' : subscription.GetName(),
            'downloader' : subscription.GetGUGKeyAndName()[1],
            'paused' : subscription.IsPaused(),
            'queries' : queries,
        } )


    return out


def files_state( session, hashes, normalise ):
    """What each file shows: its tags (storage, by service name), URLs,
    whether it is in the inbox, and where it is."""

    from hydrus.core import HydrusConstants as HC
    from hydrus.client.metadata import ClientTags

    services = session.controller.services_manager

    out = {}

    for hash_hex in sorted( hashes ):

        media_result = session.read( 'media_result', bytes.fromhex( hash_hex ) )

        tags_manager = media_result.GetTagsManager()
        locations = media_result.GetLocationsManager()

        tags = {}

        for service in services.GetServices( ( HC.LOCAL_TAG, ) ):

            current = sorted( tags_manager.GetCurrent( service.GetServiceKey(), ClientTags.TAG_DISPLAY_STORAGE ) )

            if len( current ) > 0:

                tags[ service.GetName() ] = current



        out[ hash_hex ] = {
            'tags' : tags,
            'urls' : sorted( normalise( u ) for u in locations.GetURLs() ),
            'inbox' : locations.inbox,
            'in' : sorted( services.GetName( k ) for k in locations.GetCurrent() if services.GetService( k ).GetServiceType() in ( HC.LOCAL_FILE_DOMAIN, HC.LOCAL_FILE_TRASH_DOMAIN ) ),
        }


    return out


def hashes_in( subscriptions ):

    return { f[ 'sha256' ] for s in subscriptions for q in s[ 'queries' ] for f in q[ 'files' ] if f[ 'sha256' ] is not None }


def write_fixture( path, data ):

    with open( path, 'w' ) as f:

        json.dump( data, f, indent = 1, sort_keys = True, ensure_ascii = False )
        f.write( '\n' )


    print( f'wrote {path}' )

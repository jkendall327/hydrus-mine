#!/usr/bin/env python3
"""Record the reference's searches by URL class.

`system:has url with class X` names one of the client's URL classes. The
`basic` fixture has none, so this boots the reference on a copy of it, gives
it URL classes chosen to test the matching (a domain with and without its
subdomains, `www` variants, a domain regex, query parameters in another
order, a gallery class sharing a post class's domain, a name that only
matches when case-folded), gives some files URLs for them (as given, not
normalised), and records:

- each search's response through the Client API (the file hashes, or the
  error), for class names in several spellings and alongside other
  predicates;
- for each search predicate the reference could parse, which files it
  matches when testing files in memory (`Predicate.TestMediaResult`).

The domain manager is recorded too, as a migration would find it.

Usage: QT_QPA_PLATFORM=offscreen python oracle/record_url_class_search.py
"""

import json
import os
import shutil
import sys
import tarfile
import tempfile

HERE = os.path.dirname( os.path.abspath( __file__ ) )

sys.path.insert( 0, HERE )

import hydrus_driver

OUT = os.path.join( HERE, 'fixtures', 'url_class_search.json' )

PORT = 45991

# files of the basic fixture (by position in its manifest) and the URLs given
# to them
URLS = {
    0 : [ 'http://booru.example/post/1' ],
    1 : [ 'https://www.booru.example/post/2', 'https://booru.example/search/blue_sky/1' ],
    2 : [ 'https://sub.booru.example/post/3' ],
    3 : [ 'https://booru.example/search/tag/2' ],
    4 : [ 'https://cdn.img.example/files/x.png', 'https://other.example/post/1' ],
    5 : [ 'https://gal.example/index.php?id=5&page=post&s=view', 'https://gal.example/index.php?page=post&s=list' ],
    6 : [ 'https://cdn12.regex.example/p/9', 'https://regex.example/p/9' ],
    7 : [ 'https://strasse.example/thread/4' ],
    8 : [ 'https://img.example/abc.jpg', 'https://www2.booru.example/post/77' ],
    9 : [ 'https://gal.example/index.php?page=post&s=view&id=abc' ],
}

CLASS_NAMES = [ 'Booru Post', 'booru search', 'Image Host File', 'gallery php post', 'regex domain post', 'Straße Thread' ]


def url_classes():

    from hydrus.core import HydrusConstants as HC
    from hydrus.client.networking import ClientNetworkingURLClass as U
    from hydrus.client import ClientStrings as S

    def fixed( text ):

        return S.StringMatch( match_type = S.STRING_MATCH_FIXED, match_value = text, example_string = text )


    def numeric():

        return S.StringMatch( match_type = S.STRING_MATCH_FLEXIBLE, match_value = S.FLEXIBLE_MATCH_NUMERIC, example_string = '123' )


    def anything():

        return S.StringMatch()


    def param( name, match ):

        return U.URLClassParameterFixedName( name = name, value_string_match = match )


    return [
        U.URLClass(
            'Booru Post',
            url_type = HC.URL_TYPE_POST,
            url_domain_mask = U.URLDomainMask( raw_domains = [ 'booru.example' ] ),
            path_components = [ ( fixed( 'post' ), None ), ( numeric(), None ) ],
            parameters = [],
            example_url = 'https://booru.example/post/1'
        ),
        U.URLClass(
            'booru search',
            url_type = HC.URL_TYPE_GALLERY,
            url_domain_mask = U.URLDomainMask( raw_domains = [ 'booru.example' ] ),
            path_components = [ ( fixed( 'search' ), None ), ( anything(), None ), ( numeric(), None ) ],
            parameters = [],
            example_url = 'https://booru.example/search/tag/1'
        ),
        U.URLClass(
            'Image Host File',
            url_type = HC.URL_TYPE_FILE,
            url_domain_mask = U.URLDomainMask( raw_domains = [ 'img.example' ], match_subdomains = True, keep_matched_subdomains = True ),
            path_components = [ ( anything(), None ) ],
            parameters = [],
            example_url = 'https://img.example/abc.jpg'
        ),
        U.URLClass(
            'gallery php post',
            url_type = HC.URL_TYPE_POST,
            url_domain_mask = U.URLDomainMask( raw_domains = [ 'gal.example' ] ),
            path_components = [ ( fixed( 'index.php' ), None ) ],
            parameters = [ param( 'page', fixed( 'post' ) ), param( 's', fixed( 'view' ) ), param( 'id', numeric() ) ],
            example_url = 'https://gal.example/index.php?id=1&page=post&s=view'
        ),
        U.URLClass(
            'regex domain post',
            url_type = HC.URL_TYPE_POST,
            url_domain_mask = U.URLDomainMask( domain_regexes = [ r'cdn\d+\.regex\.example' ] ),
            path_components = [ ( fixed( 'p' ), None ), ( numeric(), None ) ],
            parameters = [],
            example_url = 'https://cdn1.regex.example/p/1'
        ),
        U.URLClass(
            'Straße Thread',
            url_type = HC.URL_TYPE_WATCHABLE,
            url_domain_mask = U.URLDomainMask( raw_domains = [ 'strasse.example' ] ),
            path_components = [ ( fixed( 'thread' ), None ), ( numeric(), None ) ],
            parameters = [],
            example_url = 'https://strasse.example/thread/1'
        ),
    ]


def searches():

    out = []

    for name in CLASS_NAMES:

        lower = name.lower()

        out.append( [ f'system:has url with class {lower}' ] )
        out.append( [ f'system:does not have url with class {lower}' ] )


    out += [
        # other spellings of the predicate and the name
        [ 'system:has url with url class booru post' ],
        [ 'system:has a url with class booru post' ],
        [ 'system:has an url with class booru post' ],
        [ "system:doesn't have url with class booru post" ],
        [ 'system:does not have a url with url class booru search' ],
        [ 'system:has url with class Booru Post' ],
        [ 'system:has url with class BOORU POST' ],
        [ 'system:has url with class strasse thread' ],
        [ 'system:has url with class nothing like this' ],
        [ 'system:does not have url with class nothing like this' ],
        # alongside other predicates
        [ 'system:has url with class booru post', 'system:has url with class booru search' ],
        [ 'system:has url with class booru post', 'system:does not have url with class booru search' ],
        [ 'system:inbox', 'system:has url with class booru post' ],
        [ 'system:filetype is image', 'system:has url with class image host file' ],
        [ 'system:has url with domain booru.example', 'system:does not have url with class booru post' ],
    ]

    return out


def record( session, hashes ):

    from hydrus.client.search import ClientSearchParseSystemPredicates

    controller = session.controller

    domain_manager = controller.network_engine.domain_manager

    domain_manager.SetURLClasses( url_classes() )

    for ( i, urls ) in URLS.items():

        session.api.post( '/add_urls/associate_url', { 'hash' : hashes[ i ], 'urls_to_add' : urls, 'normalise_urls' : False } )


    results = []

    for tags in searches():

        ( status, _, body ) = session.api.request( 'GET', '/get_files/search_files', query = { 'tags' : json.dumps( tags ), 'return_hashes' : 'true' } )

        body = json.loads( body )

        if status == 200:

            results.append( { 'tags' : tags, 'status' : status, 'hashes' : sorted( body[ 'hashes' ] ) } )

        else:

            results.append( { 'tags' : tags, 'status' : status, 'exception_type' : body[ 'exception_type' ], 'error' : body[ 'error' ] } )



    media_results = session.read( 'media_results', [ bytes.fromhex( h ) for h in hashes ] )

    in_memory = []

    for tags in searches():

        if len( tags ) != 1:

            continue


        try:

            [ predicate ] = ClientSearchParseSystemPredicates.ParseSystemPredicateStringsToPredicates( tags )

        except Exception as e:

            in_memory.append( { 'text' : tags[ 0 ], 'error' : str( e ) } )

            continue


        matching = sorted( mr.GetHash().hex() for mr in media_results if predicate.TestMediaResult( mr ) )

        in_memory.append( { 'text' : tags[ 0 ], 'matching' : matching } )


    # which classes match each URL, and which the client classifies it as
    url_matches = {}

    for urls in URLS.values():

        for url in urls:

            try:

                classified = domain_manager.GetURLClass( url )

            except Exception as e:

                classified = None


            url_matches[ url ] = {
                'matches' : [ c.GetName() for c in domain_manager.GetURLClasses() if c.Matches( url ) ],
                'classified_as' : None if classified is None else classified.GetName(),
            }



    return {
        'url_matches' : url_matches,
        'domain_manager' : json.loads( json.dumps( domain_manager.GetSerialisableTuple() ) ),
        'urls' : { hashes[ i ] : urls for ( i, urls ) in URLS.items() },
        'searches' : results,
        'in_memory' : in_memory,
    }


def main():

    manifest = json.load( open( os.path.join( HERE, 'fixtures', 'legacy_db', 'basic.manifest.json' ) ) )

    hashes = [ f[ 'hash' ] for f in manifest[ 'files' ] ]

    work = tempfile.mkdtemp( prefix = 'hydrus_url_class_search_' )

    try:

        db_dir = os.path.join( work, 'db' )

        with tarfile.open( os.path.join( HERE, 'fixtures', 'legacy_db', 'basic.tar.gz' ) ) as tar:

            tar.extractall( db_dir, filter = 'data' )


        result = hydrus_driver.run_client( db_dir, lambda s: record( s, hashes ), port = PORT )

    finally:

        shutil.rmtree( work, ignore_errors = True )


    with open( OUT, 'w' ) as f:

        json.dump( result, f, indent = 1, sort_keys = True, ensure_ascii = False )
        f.write( '\n' )


    print( f'wrote {OUT}: {len( result[ "searches" ] )} searches' )


if __name__ == '__main__':

    main()

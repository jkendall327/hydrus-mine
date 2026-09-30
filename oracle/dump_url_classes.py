#!/usr/bin/env python3
"""Record the reference's URL classes making referral URLs and next pages.

Random gallery URL classes (page number in a path component or a query
parameter, assorted deltas, parameter ordering and single-value parameter
settings) built with the reference's classes make the next gallery page of
random URLs; random referral settings (each mode, converters that work or
fail) make the referral URL for random URLs with and without a given one.

Usage: python oracle/dump_url_classes.py > oracle/fixtures/url_classes.json
"""

import json
import os
import random
import sys

sys.path.insert( 0, os.path.dirname( __file__ ) )

import dump_formulas # the fake controller and its client options

from hydrus.core import HydrusConstants as HC
from hydrus.core import HydrusExceptions
from hydrus.client import ClientGlobals as CG
from hydrus.client import ClientStrings as S
from hydrus.client.networking import ClientNetworkingURLClass as U

rng = random.Random( 50 )


def fixed( text ):

    return S.StringMatch( match_type = S.STRING_MATCH_FIXED, match_value = text, example_string = text )


def numeric():

    return S.StringMatch( match_type = S.STRING_MATCH_FLEXIBLE, match_value = S.FLEXIBLE_MATCH_NUMERIC, example_string = '0' )


def parameter( name, match, default = None ):

    p = U.URLClassParameterFixedName( name = name, value_string_match = match )

    if default is not None:

        p._default_value = default


    return p


def converter():

    return rng.choice( [
        S.StringConverter( conversions = [], example_string = 'x' ),
        S.StringConverter( conversions = [ ( S.STRING_CONVERSION_REGEX_SUB, ( r'/post/view/(\d+)', r'/gallery/\1' ) ) ], example_string = 'x' ),
        S.StringConverter( conversions = [ ( S.STRING_CONVERSION_APPEND_TEXT, '#ref' ) ], example_string = 'x' ),
        # fails: not hex
        S.StringConverter( conversions = [ ( S.STRING_CONVERSION_DECODE, S.ENCODING_TYPE_HEX_UTF8 ) ], example_string = 'x' ),
    ] )


def path_gallery():

    index = rng.choice( [ 1, 2, -1, 5 ] )

    uc = U.URLClass(
        'path gallery',
        url_type = HC.URL_TYPE_GALLERY,
        url_domain_mask = U.URLDomainMask( raw_domains = [ 'example.com' ] ),
        path_components = [ ( fixed( 'tags' ), None ), ( S.StringMatch(), None ), ( numeric(), rng.choice( [ None, '1' ] ) ) ],
        parameters = [],
        gallery_index_type = U.GALLERY_INDEX_TYPE_PATH_COMPONENT,
        gallery_index_identifier = index,
        gallery_index_delta = rng.choice( [ 1, -1, 20 ] ),
        example_url = 'https://example.com/tags/blue/1'
    )

    urls = [
        'https://example.com/tags/blue/' + rng.choice( [ '1', '0', '-3', '+5', 'x', '1_000', '07' ] ),
        'https://example.com/tags/blue',
        'https://example.com/tags/a%20b/3?extra=1&z=2',
        'https://example.com/tags/blue/2/more',
        'http://www.example.com/tags/blue/4#frag',
    ]

    return ( uc, urls )


def param_gallery():

    uc = U.URLClass(
        'param gallery',
        url_type = rng.choice( [ HC.URL_TYPE_GALLERY, HC.URL_TYPE_GALLERY, HC.URL_TYPE_POST ] ),
        url_domain_mask = U.URLDomainMask( raw_domains = [ 'example.com' ] ),
        path_components = [ ( fixed( 'index.php' ), None ) ],
        parameters = [ parameter( 'page', fixed( 'post' ) ), parameter( 's', fixed( 'list' ) ), parameter( 'tags', S.StringMatch() ), parameter( 'pid', numeric(), default = rng.choice( [ None, '0' ] ) ) ],
        has_single_value_parameters = rng.random() < 0.5,
        gallery_index_type = U.GALLERY_INDEX_TYPE_PARAMETER,
        gallery_index_identifier = rng.choice( [ 'pid', 'pid', 'missing' ] ),
        gallery_index_delta = rng.choice( [ 42, 20, -1 ] ),
        example_url = 'https://example.com/index.php?page=post&s=list&tags=blue&pid=0'
    )

    uc._alphabetise_get_parameters = rng.random() < 0.5
    uc._keep_extra_parameters_for_server = rng.random() < 0.5

    urls = [
        'https://example.com/index.php?page=post&s=list&tags=blue&pid=' + rng.choice( [ '0', '42', 'x', ' 7', '-1' ] ),
        'https://example.com/index.php?tags=blue&s=list&page=post&pid=84&extra=1',
        'https://example.com/index.php?page=post&s=list&tags=blue',
        'https://example.com/index.php?page=post&single&s=list&tags=blue&pid=0',
    ]

    return ( uc, urls )


def main():

    cases = []

    for _ in range( 150 ):

        ( uc, urls ) = path_gallery() if rng.random() < 0.5 else param_gallery()

        uc._send_referral_url = rng.choice( U.SEND_REFERRAL_URL_TYPES )
        uc._referral_url_converter = converter()

        collapse = rng.random() < 0.2

        CG.client_controller.new_options.SetBoolean( 'remove_leading_url_double_slashes', collapse )

        results = []

        for url in urls:

            r = { 'url': url }

            try:

                r[ 'next_page' ] = uc.GetNextGalleryPage( url )

            except HydrusExceptions.URLClassException as e:

                r[ 'next_page_error' ] = str( e )

            except NotImplementedError as e:

                r[ 'next_page_error' ] = str( e )


            for given in ( None, 'https://example.com/where/it/was/found' ):

                try:

                    r[ 'referral_given' if given else 'referral' ] = uc.GetReferralURL( url, given )

                except HydrusExceptions.URLClassException:

                    # the reference only asks a class about URLs it matched
                    pass



            results.append( r )


        cases.append( { 'url_class': uc.GetSerialisableTuple(), 'collapse_leading_slashes': collapse, 'can_generate_next_gallery_page': uc.CanGenerateNextGalleryPage(), 'results': results } )


    json.dump( { 'cases': cases }, sys.stdout, ensure_ascii = False )
    sys.stdout.write( '\n' )


if __name__ == '__main__':

    main()

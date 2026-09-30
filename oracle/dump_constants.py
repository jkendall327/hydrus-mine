#!/usr/bin/env python3
"""Dump Hydrus (Python, v688) constant tables to JSON.

The Rust crates keep hand-maintained tables for mimes, service types, etc.
Their unit tests compare against the JSON this script emits, so any drift from
the reference implementation is caught mechanically.

Usage: python oracle/dump_constants.py > oracle/fixtures/constants.json
"""
import json
import os
import sys

sys.path.insert( 0, os.path.join( os.path.dirname( __file__ ), '..' ) )

from hydrus.core import HydrusConstants as HC
from hydrus.client import ClientConstants as CC

def int_consts( prefix_filter ):
    return { name: getattr( HC, name ) for name in dir( HC ) if prefix_filter( name ) and isinstance( getattr( HC, name ), int ) and not isinstance( getattr( HC, name ), bool ) }


mime_ints = sorted( set( HC.mime_string_lookup.keys() ) )

mimes = []
for m in mime_ints:
    mimes.append( {
        'id': m,
        'string': HC.mime_string_lookup.get( m ),
        'mimetype': HC.mime_mimetype_string_lookup.get( m ),
        'ext': HC.mime_ext_lookup.get( m ),
        'general': HC.mimes_to_general_mimetypes.get( m ),
        'searchable': m in HC.SEARCHABLE_MIMES,
        'allowed': m in HC.ALLOWED_MIMES,
        'has_thumbnail': m in HC.MIMES_WITH_THUMBNAILS,
        'can_have_pixel_hash': m in HC.FILES_THAT_CAN_HAVE_PIXEL_HASH,
        'has_perceptual_hash': m in HC.FILES_THAT_HAVE_PERCEPTUAL_HASH,
        'can_have_exif': m in HC.FILES_THAT_CAN_HAVE_EXIF,
        'can_have_icc_profile': m in HC.FILES_THAT_CAN_HAVE_ICC_PROFILE,
        'can_check_transparency': m in HC.MIMES_THAT_WE_CAN_CHECK_FOR_TRANSPARENCY,
    } )

service_types = []
for st in sorted( HC.service_string_lookup.keys() ):
    service_types.append( {
        'id': st,
        'string': HC.service_string_lookup[ st ],
        'short': HC.service_string_lookup_short.get( st ),
    } )

out = {
    'software_version': HC.SOFTWARE_VERSION,
    'client_api_version': HC.CLIENT_API_VERSION,
    'network_version': HC.NETWORK_VERSION,
    'mimes': mimes,
    'mime_string_enum_lookup': { k: ( sorted( v ) if isinstance( v, ( list, set, tuple ) ) else v ) for ( k, v ) in HC.mime_enum_lookup.items() },
    'general_mimetypes_to_mime_groups': { str( k ): sorted( v ) for ( k, v ) in HC.general_mimetypes_to_mime_groups.items() },
    'service_types': service_types,
    'content_types': { str( k ): v for ( k, v ) in HC.content_type_string_lookup.items() },
    'content_statuses': { str( k ): v for ( k, v ) in HC.content_status_string_lookup.items() },
    'content_update_actions': { str( k ): v for ( k, v ) in HC.content_update_string_lookup.items() },
    'duplicate_types': { str( k ): v for ( k, v ) in HC.duplicate_type_string_lookup.items() },
    'timestamp_types': { str( k ): v for ( k, v ) in HC.timestamp_type_str_lookup.items() },
    'url_types': { str( k ): v for ( k, v ) in HC.url_type_string_lookup.items() },
    'int_constants': int_consts( lambda n: n.isupper() and not n.startswith( '_' ) ),
    'client_int_constants': { name: getattr( CC, name ) for name in dir( CC ) if name.isupper() and not name.startswith( '_' ) and isinstance( getattr( CC, name ), int ) and not isinstance( getattr( CC, name ), bool ) },
}

json.dump( out, sys.stdout, indent = 1, sort_keys = True )
print()

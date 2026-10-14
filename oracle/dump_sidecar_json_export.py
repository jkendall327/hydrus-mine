#!/usr/bin/env python3
"""Record the reference's JSON sidecar exporter writing nested object names.

`SingleFileMetadataExporterJSON.Export` on a file in a private folder, for
several lists of object names (the duplicate, padded, blank and
newline-carrying ones the JSON exporter's queue editor accepts) and several
states of the sidecar already there (none, an object with other content, the
same names already holding rows, a JSON list, a name holding a number, text
that is no JSON): the sidecar's text afterwards, or the error.

Usage: python oracle/dump_sidecar_json_export.py > oracle/fixtures/sidecar_json_export.json
"""

import json
import os
import sys
import tempfile
import types

sys.path.insert( 0, os.path.join( os.path.dirname( __file__ ), '..' ) )

from hydrus.client import ClientGlobals as CG
from hydrus.client import ClientStrings
from hydrus.client import ClientOptions
from hydrus.client.caches import ClientCaches
from hydrus.client.metadata import ClientMetadataMigrationExporters as E

CG.client_controller = types.SimpleNamespace( parsing_cache = ClientCaches.ParsingCache(), new_options = ClientOptions.ClientOptions() )

NAME_LISTS = [
    [ 'files' ],
    [ 'files', 'files' ],
    [ 'meta', 'tags' ],
    [ ' tags ', '   ' ],
    [ 'line\nbreak' ],
    [ 'files', ' renamed ', 'files', 'line\nbreak' ],
    [ 'x', 'y', 'x', 'y', 'x' ],
]

EXISTING = [
    None,
    '{"keep":{"unknown":true},"files":{"other":"preserved"}}',
    '{"files":{"files":{"line\\nbreak":["old"]}},"files ":1}',
    '{"meta":{"tags":["old"],"other":[1,2]},"top":null}',
    '[1,2]',
    '{"files":5}',
    'not json at all',
    '',
]

ROWS = [ 'https://sidecar-keys.example/a', 'https://sidecar-keys.example/b?x=1', 'https://sidecar-keys.example/c' ]


def main():

    cases = []

    for names in NAME_LISTS:

        for existing in EXISTING:

            with tempfile.TemporaryDirectory() as d:

                file_path = os.path.join( d, 'image.jpg' )

                with open( file_path, 'wb' ) as f:

                    f.write( b'x' )

                exporter = E.SingleFileMetadataExporterJSON( nested_object_names = list( names ) )

                sidecar = exporter.GetExportPath( file_path )

                if existing is not None:

                    with open( sidecar, 'w', encoding = 'utf-8', newline = '' ) as f:

                        f.write( existing )

                case = { 'names' : names, 'existing' : existing, 'rows' : ROWS, 'sidecar' : os.path.basename( sidecar ) }

                try:

                    exporter.Export( file_path, ROWS )

                    with open( sidecar, 'rb' ) as f:

                        case[ 'result' ] = f.read().decode( 'utf-8' )

                except Exception as e:

                    case[ 'error' ] = str( e ).replace( d, '<dir>' )

                    if os.path.exists( sidecar ):

                        with open( sidecar, 'rb' ) as f:

                            case[ 'left' ] = f.read().decode( 'utf-8' )

                cases.append( case )

    json.dump( { 'cases' : cases }, sys.stdout, ensure_ascii = False, indent = 1 )
    sys.stdout.write( '\n' )


if __name__ == '__main__':

    main()

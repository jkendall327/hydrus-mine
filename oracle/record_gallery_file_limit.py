#!/usr/bin/env python3
"""Record what the reference's "By default, stop searching once this many files
are found" option (`HC.options[ 'gallery_file_limit' ]`) becomes in the things
that read it: a new gallery downloader page (`MultipleGalleryImport`), a new
gallery import (`GalleryImport`) and a new subscription's initial file limit
(`min( 100, limit )`, or 100 with no limit).

Usage: QT_QPA_PLATFORM=offscreen ~/pyenv/bin/python oracle/record_gallery_file_limit.py
       (writes oracle/fixtures/gallery_file_limit.json)
"""
import json
import os
import sys
import tempfile

HERE = os.path.dirname( os.path.abspath( __file__ ) )
sys.path.insert( 0, HERE )

OUT = os.path.join( HERE, 'fixtures', 'gallery_file_limit.json' )

LIMITS = [ 2000, 345, 100, 99, 101, 50, 1, 1000000, None ]


def record( session ):

    c = session.controller

    def work():

        from hydrus.core import HydrusConstants as HC
        from hydrus.client.importing import ClientImportGallery, ClientImportSubscriptions

        old = HC.options[ 'gallery_file_limit' ]

        out = { 'default' : old, 'cases' : [] }

        try:

            for limit in LIMITS:

                HC.options[ 'gallery_file_limit' ] = limit

                out[ 'cases' ].append( {
                    'limit' : limit,
                    'gallery page' : ClientImportGallery.MultipleGalleryImport()._file_limit,
                    'gallery import' : ClientImportGallery.GalleryImport()._file_limit,
                    'new subscription initial' : ClientImportSubscriptions.Subscription( 'new subscription' )._initial_file_limit,
                    'new subscription periodic' : ClientImportSubscriptions.Subscription( 'new subscription' )._periodic_file_limit,
                } )


        finally:

            HC.options[ 'gallery_file_limit' ] = old


        return out

    return c.CallBlockingToQt( c.gui, work )


def main():

    import hydrus_driver
    import record_api

    if len( sys.argv ) > 1 and sys.argv[1] == '--child':

        output = sys.argv[2]

        result = hydrus_driver.run_client( record_api.unpack_fixture( 'basic' ), record )

        with open( output, 'w' ) as f:

            json.dump( result, f, ensure_ascii = False, indent = 1 )


        return

    with tempfile.TemporaryDirectory() as work:

        output = os.path.join( work, 'result.json' )

        hydrus_driver.run_in_subprocess( os.path.abspath( __file__ ), '--child', output )

        result = json.load( open( output ) )

        with open( OUT, 'w' ) as f:

            json.dump( result, f, ensure_ascii = False, indent = 1 )
            f.write( '\n' )


        print( 'wrote', OUT )


if __name__ == '__main__':

    main()

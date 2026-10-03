#!/usr/bin/env python3
"""Record the reference's default simple downloader formulae, and what each
finds on some pages.

A simple downloader page parses each page it is given with a formula
(`SimpleDownloaderParsingFormula`): the formula runs on the page's text
with the page's URL as its parsing context, newlines collapsed, and each
result is joined to the page's URL (`urljoin`) as a file URL to download
(`SimpleDownloaderImport._WorkOnGallery`). A new client has the formulae
in `static/default/simple_downloader_formulae`.

This writes the default formulae as the reference serialises them to
`crates/hydrus-legacy/src/objects/simple_downloader_formulae_defaults.json`
(hydrus-rs's defaults for a new store), and to the fixture, with the file
URLs each finds on a few pages (images linked and not, relative and
absolute links, nested tags, none).

Usage: python oracle/dump_simple_downloader_formulae.py > oracle/fixtures/simple_downloader_formulae.json
"""

import json
import os
import sys
import types
import urllib.parse

ROOT = os.path.join( os.path.dirname( __file__ ), '..' )

sys.path.insert( 0, ROOT )

os.chdir( ROOT )

from hydrus.client import ClientDefaults
from hydrus.client import ClientGlobals as CG
from hydrus.client import ClientOptions
from hydrus.client.caches import ClientCaches

import tempfile

from hydrus.core import HydrusGlobals as HG

CG.client_controller = types.SimpleNamespace( parsing_cache = ClientCaches.ParsingCache(), new_options = ClientOptions.ClientOptions() )

# (reading the defaults' PNGs goes through a temp file)
HG.controller = types.SimpleNamespace( GetHydrusTempDir = lambda: tempfile.mkdtemp(), GetDBDir = lambda: tempfile.mkdtemp() )

PAGES = [
    [ 'https://example.com/gallery/page.html', '<html><body><a href="/full/1.jpg"><img src="/thumbs/1.jpg"></a><a href="full/2.png"><img src="t2.png"></a><img src="https://cdn.example.com/solo.gif"><p>text</p></body></html>' ],
    [ 'https://example.com/a/b/', '<div><a href="../c/x.webm"><span><img src="y.jpg"></span></a><img src="//other.net/z.jpg"></div>' ],
    [ 'https://example.com/', '<html><body><p>nothing here</p><a href="link.html">a link</a></body></html>' ],
    [ 'https://example.com/q?x=1', '<a href="?page=2"><img src="i.png"></a><a href="#frag"><img src="j.png"></a>' ],
]


def main():

    formulae = ClientDefaults.GetDefaultSimpleDownloaderFormulae()

    formulae.sort( key = lambda f: f.GetName() )

    tuples = [ f.GetSerialisableTuple() for f in formulae ]

    with open( os.path.join( ROOT, 'crates', 'hydrus-legacy', 'src', 'objects', 'simple_downloader_formulae_defaults.json' ), 'w' ) as f:

        json.dump( tuples, f )
        f.write( '\n' )


    pages = []

    for ( url, text ) in PAGES:

        found = {}

        for formula in formulae:

            parsed = formula.GetFormula().Parse( { 'url' : url }, text, True )

            found[ formula.GetName() ] = [ urllib.parse.urljoin( url, p ) for p in parsed ]


        pages.append( { 'url' : url, 'text' : text, 'found' : found } )


    json.dump( { 'formulae' : tuples, 'pages' : pages }, sys.stdout, indent = 1, ensure_ascii = False )
    sys.stdout.write( '\n' )


if __name__ == '__main__':

    main()

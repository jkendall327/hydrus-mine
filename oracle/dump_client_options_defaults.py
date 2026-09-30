#!/usr/bin/env python3
"""Save a new reference client's options object, for hydrus-rs to report
as its options where none were migrated (and to fill in options an older
install never saved).

Writes crates/hydrus-legacy/src/objects/client_options_defaults.json: the
serialised `ClientOptions` of a client booted on an empty database.

Usage: QT_QPA_PLATFORM=offscreen python oracle/dump_client_options_defaults.py
"""

import json
import os
import shutil
import sys
import tempfile

HERE = os.path.dirname( os.path.abspath( __file__ ) )

sys.path.insert( 0, HERE )

import hydrus_driver

OUT = os.path.join( HERE, '..', 'crates', 'hydrus-legacy', 'src', 'objects', 'client_options_defaults.json' )


def main():

    work = tempfile.mkdtemp( prefix = 'hydrus_options_' )

    try:

        tuple_ = hydrus_driver.run_client( os.path.join( work, 'db' ), lambda s: json.loads( json.dumps( s.controller.new_options.GetSerialisableTuple() ) ) )

    finally:

        shutil.rmtree( work, ignore_errors = True )


    with open( OUT, 'w' ) as f:

        json.dump( tuple_, f, sort_keys = False )
        f.write( '\n' )


    print( f'wrote {OUT}: {os.path.getsize( OUT )} bytes' )


if __name__ == '__main__':

    main()

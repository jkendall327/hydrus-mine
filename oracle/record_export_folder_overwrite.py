#!/usr/bin/env python3
"""Record what an export folder's sidecar overwrite options do.

The recorded "regular" export folder (oracle/fixtures/legacy_db/
export_folder.tar.gz: files tagged `export me`, their tags written to `.txt`
sidecars and their URLs to `.json` ones) is run into a fresh folder, its
files named by hash. Stale `.txt` sidecars are put where the export will
write them, or none, and the folder runs now under each overwrite option: none, "overwrite sidecars on next run" (spent by the run) and
"always overwrite sidecars". Recorded per run: the sidecar's text afterwards
and the options as stored afterwards. Each case runs the folder twice, the
stale sidecar put back between the runs.

Usage: QT_QPA_PLATFORM=offscreen python oracle/record_export_folder_overwrite.py
       (writes fixtures/export_folder_overwrite.json)
"""

import json
import os
import shutil
import sys
import tempfile

HERE = os.path.dirname( os.path.abspath( __file__ ) )

sys.path.insert( 0, HERE )

import hydrus_driver

PORT = 45988

IMAGES = os.path.join( HERE, 'fixtures', 'auto_resolution' )

STALE = 'stale tag\n'

TAGS = [ 'export me', 'blue eyes', 'series:metroid' ]

# (name, stale sidecar placed first, overwrite on next run, always overwrite)
CASES = [
    ( 'nothing set, a stale sidecar', True, False, False ),
    ( 'nothing set, no sidecar yet', False, False, False ),
    ( 'next run, a stale sidecar', True, True, False ),
    ( 'always, a stale sidecar', True, False, True ),
]


def record( session ):
    
    from hydrus.core import HydrusSerialisable
    
    api = session.api
    
    hashes = sorted( api.get( '/get_files/search_files', tags = json.dumps( [ 'export me' ] ), return_hashes = 'true' )[ 'hashes' ] )
    
    exts = { m[ 'hash' ] : m[ 'ext' ] for m in api.get( '/get_files/file_metadata', hashes = json.dumps( hashes ) )[ 'metadata' ] }
    
    names = [ h + exts[ h ] for h in hashes ]
    
    work = tempfile.mkdtemp( prefix = 'hydrus_overwrite_' )
    
    out = { 'names' : names, 'stale' : STALE, 'cases' : [] }
    
    def sidecars( folder_path ):
        
        found = {}
        
        for name in sorted( os.listdir( folder_path ) ):
            
            if name.endswith( '.txt' ) or name.endswith( '.json' ):
                
                with open( os.path.join( folder_path, name ) ) as f:
                    
                    found[ name ] = f.read()
                    
                
            
        
        return found
        
    
    for ( name, stale, next_run, always ) in CASES:
        
        folder_path = os.path.join( work, name )
        
        os.makedirs( folder_path )
        
        # the recorded "regular" export folder (a tags .txt sidecar and a URLs
        # .json one), here with the files named by their hashes
        folder = session.read( 'serialisable_named', HydrusSerialisable.SERIALISABLE_TYPE_EXPORT_FOLDER, 'regular' )
        
        folder._path = folder_path
        folder._phrase = '{hash}'
        folder._always_overwrite_sidecars = always
        
        runs = []
        
        for run in range( 2 ):
            
            for file_name in names:
                
                path = os.path.join( folder_path, file_name + '.txt' )
                
                if stale:
                    
                    with open( path, 'w' ) as f:
                        
                        f.write( STALE )
                        
                    
                elif os.path.exists( path ):
                    
                    os.remove( path )
                    
                
            
            folder._run_now = True
            
            if run == 0:
                
                folder._overwrite_sidecars_on_next_run = next_run
                
            
            folder.DoWork()
            
            runs.append( { 'sidecars' : sidecars( folder_path ), 'overwrite_next_after' : bool( folder._overwrite_sidecars_on_next_run ), 'always_after' : bool( folder._always_overwrite_sidecars ) } )
            
        
        out[ 'cases' ].append( { 'name' : name, 'stale' : stale, 'next_run' : next_run, 'always' : always, 'runs' : runs } )
        
    
    shutil.rmtree( work, ignore_errors = True )
    
    return out
    

def main():
    
    if len( sys.argv ) > 1 and sys.argv[ 1 ] == '--child':
        
        import record_api
        
        output_path = sys.argv[ 2 ]
        
        result = hydrus_driver.run_client( record_api.unpack_fixture( 'export_folder' ), record, port = PORT )
        
        with open( output_path, 'w' ) as f:
            
            json.dump( result, f, ensure_ascii = False )
            
        
        return
        
    
    with tempfile.TemporaryDirectory() as tmp:
        
        path = os.path.join( tmp, 'result.json' )
        
        hydrus_driver.run_in_subprocess( os.path.abspath( __file__ ), '--child', path )
        
        with open( path ) as f:
            
            result = json.load( f )
            
        
    
    with open( os.path.join( HERE, 'fixtures', 'export_folder_overwrite.json' ), 'w' ) as f:
        
        json.dump( result, f, indent = 1, ensure_ascii = False )
        f.write( '\n' )
        
    
    print( 'wrote export_folder_overwrite.json' )
    

if __name__ == '__main__':
    
    main()

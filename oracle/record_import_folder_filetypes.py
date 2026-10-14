#!/usr/bin/env python3
"""Record which files an import folder imports under different allowed filetypes.

A folder holds a png, a jpeg, a gif, a bmp, a text file and a webp-less mix;
for each set of allowed filetypes (the file filtering import options of the
folder), a fresh import folder checks it now and works through its files.
Recorded: every file seed's path (relative to the folder), status and note.

Usage: QT_QPA_PLATFORM=offscreen python oracle/record_import_folder_filetypes.py
       (writes fixtures/import_folder_filetypes.json)
"""

import json
import os
import shutil
import sys
import tempfile

HERE = os.path.dirname( os.path.abspath( __file__ ) )

sys.path.insert( 0, HERE )

import hydrus_driver

PORT = 45987

IMAGES = os.path.join( HERE, 'fixtures', 'auto_resolution' )

OLD_MTIME = 1_600_000_000

# each case gets files the client has not seen (it keeps what it imports)
FILES = {
    'a.png' : [ 'p03_a.png', 'p04_a.png', 'p05_a.png', 'p06_a.png' ],
    'b.jpg' : [ 'p01_e.jpg', 'p02_e.jpg', 'p03_e.jpg', 'p04_e.jpg' ],
    'c.gif' : [ 'p00_h.gif', 'p03_h.gif', 'p04_h.gif', 'p05_h.gif' ],
    'd.bmp' : [ 'p00_c.bmp', 'p02_c.bmp', 'p04_c.bmp', 'p05_c.bmp' ],
}

# (name, allowed filetypes as the reference's constants)
CASES = [
    ( 'everything', None ),
    ( 'jpeg and gif', [ 'IMAGE_JPEG', 'IMAGE_GIF' ] ),
    ( 'only png', [ 'IMAGE_PNG' ] ),
    ( 'nothing but bmp', [ 'IMAGE_BMP' ] ),
]


def record( session ):
    
    from hydrus.core import HydrusConstants as HC
    from hydrus.core import HydrusTime
    from hydrus.client.importing import ClientImportLocal
    from hydrus.client.importing.options import FileFilteringImportOptions
    from hydrus.client.importing.options import ImportOptionsContainer
    
    work = tempfile.mkdtemp( prefix = 'hydrus_filetypes_' )
    
    out = { 'files' : sorted( FILES ), 'cases' : [] }
    
    for ( n, ( name, filetypes ) ) in enumerate( CASES ):
        
        folder_path = os.path.join( work, str( n ) )
        
        os.makedirs( folder_path )
        
        for ( target, sources ) in FILES.items():
            
            source = sources[ n ]
            
            path = os.path.join( folder_path, target )
            
            shutil.copy( os.path.join( IMAGES, source ), path )
            
            os.utime( path, ( OLD_MTIME, OLD_MTIME ) )
            
        
        container = ImportOptionsContainer.ImportOptionsContainer()
        
        if filetypes is not None:
            
            options = FileFilteringImportOptions.FileFilteringImportOptions()
            
            options.SetAllowedSpecificFiletypes( [ getattr( HC, t ) for t in filetypes ] )
            
            container.SetImportOptions( options )
            
        
        folder = ClientImportLocal.ImportFolder( 'types', path = folder_path, import_options_container = container, period = 3600, check_regularly = False )
        
        folder._check_now = True
        
        folder.DoWork()
        
        seeds = [ [ os.path.relpath( f.file_seed_data, folder_path ), f.status, f.note ] for f in folder.GetFileSeedCache().GetFileSeeds() ]
        
        out[ 'cases' ].append( { 'name' : name, 'allowed' : filetypes, 'seeds' : sorted( seeds ) } )
        
        last = ( folder, folder_path )
        
    
    # the last folder (only bmp allowed) is edited to allow everything again:
    # the ignored files are forgotten and so looked at again
    ( folder, folder_path ) = last
    
    values = list( folder.ToTuple() )
    
    values[ 2 ] = ImportOptionsContainer.ImportOptionsContainer()
    values[ 9 ] = True
    
    folder.SetTuple( *values )
    
    out[ 'after_edit' ] = { 'seeds_forgotten' : len( folder.GetFileSeedCache() ) }
    
    folder.DoWork()
    
    out[ 'after_edit' ][ 'seeds' ] = sorted( [ [ os.path.relpath( f.file_seed_data, folder_path ), f.status, f.note ] for f in folder.GetFileSeedCache().GetFileSeeds() ] )
    
    # the skip period: files modified more recently than it are left for a
    # later check
    import time
    
    folder_path = os.path.join( work, 'skip' )
    
    os.makedirs( folder_path )
    
    ages = { 'new.png' : 3600, 'old.jpg' : 200000 }
    
    for ( target, source ) in [ ( 'new.png', 'p06_b_bloated.png' ), ( 'old.jpg', 'p05_e.jpg' ) ]:
        
        path = os.path.join( folder_path, target )
        
        shutil.copy( os.path.join( IMAGES, source ), path )
        
        at = time.time() - ages[ target ]
        
        os.utime( path, ( at, at ) )
        
    
    folder = ClientImportLocal.ImportFolder( 'skip', path = folder_path, period = 3600, check_regularly = False )
    
    out[ 'skip' ] = { 'ages' : ages, 'checks' : [] }
    
    for period in [ 86400, 1 ]:
        
        folder.SetLastModifiedTimeSkipPeriod( period )
        
        folder._check_now = True
        
        folder.DoWork()
        
        out[ 'skip' ][ 'checks' ].append( { 'period' : period, 'seeds' : sorted( [ [ os.path.relpath( f.file_seed_data, folder_path ), f.status ] for f in folder.GetFileSeedCache().GetFileSeeds() ] ) } )
        
    
    shutil.rmtree( work, ignore_errors = True )
    
    return out
    

def main():
    
    if len( sys.argv ) > 1 and sys.argv[ 1 ] == '--child':
        
        import record_api
        
        output_path = sys.argv[ 2 ]
        
        result = hydrus_driver.run_client( record_api.unpack_fixture( 'basic' ), record, port = PORT )
        
        with open( output_path, 'w' ) as f:
            
            json.dump( result, f, ensure_ascii = False )
            
        
        return
        
    
    with tempfile.TemporaryDirectory() as tmp:
        
        path = os.path.join( tmp, 'result.json' )
        
        hydrus_driver.run_in_subprocess( os.path.abspath( __file__ ), '--child', path )
        
        with open( path ) as f:
            
            result = json.load( f )
            
        
    
    with open( os.path.join( HERE, 'fixtures', 'import_folder_filetypes.json' ), 'w' ) as f:
        
        json.dump( result, f, indent = 1, ensure_ascii = False )
        f.write( '\n' )
        
    
    print( 'wrote import_folder_filetypes.json' )
    

if __name__ == '__main__':
    
    main()

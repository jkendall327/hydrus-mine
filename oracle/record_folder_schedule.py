#!/usr/bin/env python3
"""Record when the reference's import and export folders work, and what they show.

On the `basic` fixture plus one file, an export folder (a regular export of
the files tagged `export me`) and an import folder (one old image waiting)
are made afresh for each case, with the schedule options set and
`last_checked` put at a given age, then `DoWork` is called once. Recorded
per case: whether it worked (files written / the folder checked), the
popups it sent (title, whether they carried files), and the folder's stored
state afterwards (`run_now`/`check_now`, `paused`, whether it still runs
regularly).

Usage: QT_QPA_PLATFORM=offscreen python oracle/record_folder_schedule.py
       (writes fixtures/folder_schedule.json)
"""

import json
import os
import shutil
import sys
import tempfile
import time

HERE = os.path.dirname( os.path.abspath( __file__ ) )

sys.path.insert( 0, HERE )

import hydrus_driver

PORT = 45986

IMAGES = os.path.join( HERE, 'fixtures', 'auto_resolution' )

OLD_MTIME = 1_600_000_000

PERIOD = 3600

# (name, options, age of last_checked in seconds)
EXPORT_CASES = [
    ( 'regular, not yet due', { 'run_regularly' : True }, PERIOD // 2 ),
    ( 'regular, due', { 'run_regularly' : True }, PERIOD + 1 ),
    ( 'regular, due, popup', { 'run_regularly' : True, 'show_working_popup' : True }, PERIOD + 1 ),
    ( 'not regular, long ago', { 'run_regularly' : False }, 10 * PERIOD ),
    ( 'not regular, run now', { 'run_regularly' : False, 'run_now' : True }, 10 * PERIOD ),
    ( 'regular, run now, no popup asked for', { 'run_regularly' : True, 'run_now' : True, 'show_working_popup' : False }, 10 ),
]

IMPORT_CASES = [
    ( 'regular, not yet due', { 'check_regularly' : True }, PERIOD // 2 ),
    ( 'regular, due', { 'check_regularly' : True, 'show_working_popup' : False, 'publish_files_to_popup_button' : False }, PERIOD + 1 ),
    ( 'regular, due, popup, files published', { 'check_regularly' : True, 'show_working_popup' : True, 'publish_files_to_popup_button' : True }, PERIOD + 1 ),
    ( 'regular, due, popup, files not published', { 'check_regularly' : True, 'show_working_popup' : True, 'publish_files_to_popup_button' : False }, PERIOD + 1 ),
    ( 'not regular, long ago', { 'check_regularly' : False }, 10 * PERIOD ),
    ( 'not regular, check now, no popup asked for', { 'check_regularly' : False, 'check_now' : True, 'show_working_popup' : False, 'publish_files_to_popup_button' : False }, 10 * PERIOD ),
    ( 'paused, check now', { 'check_regularly' : True, 'paused' : True, 'check_now' : True }, 10 * PERIOD ),
    ( 'paused, due', { 'check_regularly' : True, 'paused' : True }, 10 * PERIOD ),
]


def popup_summary( popups ):
    
    return [ { 'title' : p[ 'title' ], 'has_files' : p[ 'files' ] is not None } for p in popups ]
    

def listing( folder ):
    
    out = []
    
    for ( root, dirs, files ) in os.walk( folder ):
        
        for name in files:
            
            out.append( os.path.relpath( os.path.join( root, name ), folder ) )
            
        
    
    return sorted( out )
    

def record( session ):
    
    from hydrus.core import HydrusConstants as HC
    from hydrus.core import HydrusTime
    from hydrus.client import ClientConstants as CC
    from hydrus.client import ClientLocation
    from hydrus.client.exporting import ClientExportingFiles
    from hydrus.client.importing import ClientImportLocal
    from hydrus.client.search import ClientSearchFileSearchContext
    from hydrus.client.search import ClientSearchPredicate
    from hydrus.client.search import ClientSearchTagContext
    
    my_tags = CC.DEFAULT_LOCAL_TAG_SERVICE_KEY
    
    h = session.api.post( '/add_files/add_file', { 'path' : os.path.join( IMAGES, 'p00_a.png' ) } )[ 'hash' ]
    
    session.api.post( '/add_tags/add_tags', { 'hash' : h, 'service_keys_to_tags' : { my_tags.hex() : [ 'export me' ] } } )
    
    HC.options[ 'delete_to_recycle_bin' ] = False
    
    time.sleep( 2 )
    
    work = tempfile.mkdtemp( prefix = 'hydrus_schedule_' )
    
    def search():
        
        location_context = ClientLocation.LocationContext.STATICCreateSimple( CC.LOCAL_FILE_SERVICE_KEY )
        tag_context = ClientSearchTagContext.TagContext( service_key = CC.COMBINED_TAG_SERVICE_KEY )
        predicates = [ ClientSearchPredicate.Predicate( ClientSearchPredicate.PREDICATE_TYPE_TAG, 'export me' ) ]
        
        return ClientSearchFileSearchContext.FileSearchContext( location_context = location_context, tag_context = tag_context, predicates = predicates )
        
    
    out = { 'period' : PERIOD, 'export' : [], 'import' : [] }
    
    for ( name, options, age ) in EXPORT_CASES:
        
        folder_path = os.path.join( work, 'export ' + name )
        
        os.makedirs( folder_path )
        
        folder = ClientExportingFiles.ExportFolder( 'schedule', path = folder_path, export_type = HC.EXPORT_FOLDER_TYPE_REGULAR, file_search_context = search(), period = PERIOD, phrase = '{hash}', run_regularly = options.get( 'run_regularly', True ), show_working_popup = options.get( 'show_working_popup', False ), run_now = options.get( 'run_now', False ) )
        
        folder._last_checked = HydrusTime.GetNow() - age
        
        before = folder._last_checked
        
        popups = hydrus_driver.popups_sent( folder.DoWork )
        
        out[ 'export' ].append( { 'name' : name, 'options' : options, 'age' : age, 'worked' : len( listing( folder_path ) ) > 0, 'popups' : popup_summary( popups ), 'run_now_after' : bool( folder._run_now ), 'run_regularly_after' : bool( folder._run_regularly ), 'last_checked_moved' : folder._last_checked != before } )
        
    
    # (a file the client has not seen, for each case)
    fresh = [ 'p01_e.jpg', 'p02_h.gif', 'p03_a.png', 'p04_a.png', 'p00_c.bmp', 'p00_f_exif.jpg', 'p00_g_lossy.jpg', 'p00_b_bloated.png' ]
    
    for ( name, options, age ) in IMPORT_CASES:
        
        folder_path = os.path.join( work, 'import ' + name )
        
        os.makedirs( folder_path )
        
        target = os.path.join( folder_path, 'a.png' )
        
        shutil.copy( os.path.join( IMAGES, fresh.pop( 0 ) ), target )
        
        os.utime( target, ( OLD_MTIME, OLD_MTIME ) )
        
        folder = ClientImportLocal.ImportFolder( 'schedule', path = folder_path, period = PERIOD, check_regularly = options.get( 'check_regularly', True ), show_working_popup = options.get( 'show_working_popup', True ), publish_files_to_popup_button = options.get( 'publish_files_to_popup_button', True ) )
        
        folder._paused = options.get( 'paused', False )
        folder._check_now = options.get( 'check_now', False )
        folder._last_checked = HydrusTime.GetNow() - age
        
        before = folder._last_checked
        
        popups = hydrus_driver.popups_sent( folder.DoWork )
        
        out[ 'import' ].append( { 'name' : name, 'options' : options, 'age' : age, 'worked' : len( folder.GetFileSeedCache() ) > 0, 'statuses' : [ [ f.status, f.note ] for f in folder.GetFileSeedCache().GetFileSeeds() ], 'popups' : popup_summary( popups ), 'paused_after' : bool( folder._paused ), 'check_now_after' : bool( folder._check_now ), 'last_checked_moved' : folder._last_checked != before } )
        
    
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
            
        
    
    with open( os.path.join( HERE, 'fixtures', 'folder_schedule.json' ), 'w' ) as f:
        
        json.dump( result, f, indent = 1, ensure_ascii = False )
        f.write( '\n' )
        
    
    print( 'wrote folder_schedule.json' )
    

if __name__ == '__main__':
    
    main()

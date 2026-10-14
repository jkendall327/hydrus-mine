#!/usr/bin/env python3
"""Record the file log's selected-row actions on the reference's real panel.

On the `basic` fixture, a file log (`EditFileSeedCachePanel`) holds a
previously deleted file (really deleted in the client: in the trash, with a
deletion record), a failed import, and others. With rows selected:

- `_SetSelected( STATUS_UNKNOWN )` ("try again") and `_SetSelected(
  STATUS_SKIPPED )` ("skip"): the questions asked, the log afterwards, and
  the file's state in the client (trashed, deleted record) after each answer;
- `_OpenSelectedFileSeedData` ("open URLs" / "open files' locations"): the
  question for more than ten, the URLs given to the browser and the paths
  given to `ClientPaths.OpenFileLocations`.

Usage: QT_QPA_PLATFORM=offscreen python oracle/record_file_log_selected.py
       (writes fixtures/file_log_selected.json)
"""

import json
import os
import sys
import tempfile
import time

HERE = os.path.dirname( os.path.abspath( __file__ ) )

sys.path.insert( 0, HERE )

PORT = 45984


def record( session ):
    
    controller = session.controller
    api = session.api
    
    from qtpy import QtWidgets as QW
    
    from hydrus.client import ClientConstants as CC
    from hydrus.client import ClientPaths
    from hydrus.client.gui import ClientGUIDialogsQuick as Q
    from hydrus.client.gui.executables import ClientGUIExecutableActions
    from hydrus.client.gui.importing import ClientGUIFileSeedCache as W
    from hydrus.client.importing import ClientImportFileSeeds as S
    
    time.sleep( 3 )
    
    hashes = sorted( api.get( '/get_files/search_files', tags = json.dumps( [ 'system:everything' ] ), return_hashes = 'true' )[ 'hashes' ] )
    
    victim = hashes[ 0 ]
    
    def file_state():
        
        m = api.get( '/get_files/file_metadata', hashes = json.dumps( [ victim ] ) )[ 'metadata' ][ 0 ]
        
        return { 'trashed' : bool( m[ 'is_trashed' ] ), 'deleted_record' : len( m[ 'file_services' ][ 'deleted' ] ) > 0, 'current_services' : len( m[ 'file_services' ][ 'current' ] ) }
        
    
    def delete_it():
        
        api.post( '/add_files/delete_files', { 'hash' : victim } )
        
        time.sleep( 1 )
        
    
    asked = []
    answer = { 'yes' : False }
    
    def yesno( parent, message, **kwargs ):
        
        asked.append( message )
        
        return QW.QDialog.DialogCode.Accepted if answer[ 'yes' ] else QW.QDialog.DialogCode.Rejected
        
    
    Q.GetYesNo = yesno
    
    urls = []
    locations = []
    
    ClientGUIExecutableActions.OpenExternallyURLDefault = lambda win, url: urls.append( url )
    ClientPaths.OpenFileLocations = lambda paths: locations.append( list( paths ) )
    
    plan = [
        ( 'https://example.com/post/1', CC.STATUS_DELETED, 'Previously deleted', True ),
        ( 'https://example.com/post/2', CC.STATUS_ERROR, 'Error: 500', False ),
        ( 'https://example.com/post/3', CC.STATUS_VETOED, 'veto: blacklisted!', False ),
        ( 'https://example.com/post/4', CC.STATUS_UNKNOWN, '', False ),
    ]
    
    def run_on_panel( cache, selected_indices, call ):
        
        holder = {}
        
        def work():
            
            panel = W.EditFileSeedCachePanel( controller.gui, cache )
            
            seeds = cache.GetFileSeeds()
            
            panel._list_ctrl.SelectDatas( [ seeds[ i ] for i in selected_indices ], deselect_others = True )
            
            call( panel )
            
            QW.QApplication.processEvents()
            
            holder[ 'panel' ] = panel
            
        
        controller.CallBlockingToQt( controller.gui, work )
        
        # (the undelete is written on a thread, then the statuses set)
        time.sleep( 2.5 )
        
        controller.CallBlockingToQt( controller.gui, lambda: holder[ 'panel' ].deleteLater() )
        
    
    def build():
        
        seeds = []
        
        for ( url, status, note, has_hash ) in plan:
            
            s = S.FileSeed( S.FILE_SEED_TYPE_URL, url )
            
            if status != CC.STATUS_UNKNOWN:
                
                s.SetStatus( status, note = note )
                
            
            if has_hash:
                
                s.SetHash( bytes.fromhex( victim ) )
                
            
            seeds.append( s )
            
        
        cache = S.FileSeedCache()
        
        cache.AddFileSeeds( seeds )
        
        return cache
        
    
    def state( cache ):
        
        return [ [ s.file_seed_data, s.status, s.note ] for s in cache.GetFileSeeds() ]
        
    
    out = { 'start' : state( build() ), 'try_again' : [], 'open' : [] }
    
    # try again: with the deleted file selected the question is asked; yes
    # erases its record (and takes it out of the trash), no does not; the
    # statuses are reset either way
    for ( name, selected, yes ) in [ ( 'deleted and failed, no', [ 0, 1 ], False ), ( 'deleted and failed, yes', [ 0, 1 ], True ), ( 'failed and ignored', [ 1, 2 ], False ) ]:
        
        delete_it()
        
        del asked[ : ]
        
        answer[ 'yes' ] = yes
        
        cache = build()
        
        before = file_state()
        
        run_on_panel( cache, selected, lambda panel: panel._SetSelected( CC.STATUS_UNKNOWN ) )
        
        out[ 'try_again' ].append( { 'name' : name, 'selected' : selected, 'answer' : yes, 'asked' : list( asked ), 'after' : state( cache ), 'file_before' : before, 'file_after' : file_state() } )
        
    
    # skip never asks
    delete_it()
    
    del asked[ : ]
    
    cache = build()
    
    run_on_panel( cache, [ 0, 1, 3 ], lambda panel: panel._SetSelected( CC.STATUS_SKIPPED ) )
    
    out[ 'skip' ] = { 'selected' : [ 0, 1, 3 ], 'asked' : list( asked ), 'after' : state( cache ), 'file_after' : file_state() }
    
    # open
    def open_case( name, seeds_data, seed_type, yes ):
        
        cache = S.FileSeedCache()
        
        cache.AddFileSeeds( [ S.FileSeed( seed_type, data ) for data in seeds_data ] )
        
        del asked[ : ]
        del urls[ : ]
        del locations[ : ]
        
        answer[ 'yes' ] = yes
        
        run_on_panel( cache, list( range( len( seeds_data ) ) ), lambda panel: panel._OpenSelectedFileSeedData() )
        
        out[ 'open' ].append( { 'name' : name, 'sources' : seeds_data, 'answer' : yes, 'asked' : list( asked ), 'urls' : list( urls ), 'locations' : list( locations ) } )
        
    
    few = [ f'https://example.com/post/{i}' for i in range( 3 ) ]
    many = [ f'https://example.com/post/{i}' for i in range( 12 ) ]
    paths = [ '/tmp/hydrus_file_log/a.png', '/tmp/hydrus_file_log/sub/b.jpg' ]
    
    open_case( 'few urls', few, S.FILE_SEED_TYPE_URL, False )
    open_case( 'many urls, no', many, S.FILE_SEED_TYPE_URL, False )
    open_case( 'many urls, yes', many, S.FILE_SEED_TYPE_URL, True )
    open_case( 'paths', paths, S.FILE_SEED_TYPE_HDD, False )
    
    return out
    

def child( out ):
    
    import hydrus_driver
    import record_api
    
    db_dir = record_api.unpack_fixture( 'basic' )
    
    result = hydrus_driver.run_client( db_dir, record, port = PORT )
    
    with open( out, 'w' ) as f:
        
        json.dump( result, f, ensure_ascii = False )
        
    

def main():
    
    if len( sys.argv ) > 1 and sys.argv[ 1 ] == '--child':
        
        child( sys.argv[ 2 ] )
        
        return
        
    
    import hydrus_driver
    
    with tempfile.TemporaryDirectory() as work:
        
        out = os.path.join( work, 'result.json' )
        
        hydrus_driver.run_in_subprocess( os.path.abspath( __file__ ), '--child', out )
        
        with open( out ) as f:
            
            result = json.load( f )
            
        
    
    with open( os.path.join( HERE, 'fixtures', 'file_log_selected.json' ), 'w' ) as f:
        
        json.dump( result, f, indent = 1, ensure_ascii = False )
        f.write( '\n' )
        
    
    print( 'wrote file_log_selected.json' )
    

if __name__ == '__main__':
    
    main()

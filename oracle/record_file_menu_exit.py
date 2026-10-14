#!/usr/bin/env python3
"""Record the reference's File menu entries that open folders and leave the client.

On the `basic` fixture, each entry is triggered from the real menu bar:

- "open > installation directory" and "open > database directory": the
  directory handed to `HydrusPaths.LaunchDirectory`, as a relation to the
  client's database directory and install directory;
- "restart", "exit/force maintenance" and "exit" (`ClientGUI.TryToExit`):
  every question asked (its text, title and auto-answer), and whether the
  client then went on to exit (`Controller.Exit`) with `HG.restart` and
  `HG.do_idle_shutdown_work` set, under different options and answers.

Nothing really exits or launches: the launcher, the yes/no dialogs and
`Controller.Exit` are replaced by recorders.

Usage: QT_QPA_PLATFORM=offscreen python oracle/record_file_menu_exit.py
       (writes fixtures/file_menu_exit.json)
"""

import json
import os
import sys
import tempfile
import time

HERE = os.path.dirname( os.path.abspath( __file__ ) )

sys.path.insert( 0, HERE )

OUT = os.path.join( HERE, 'fixtures', 'file_menu_exit.json' )


def find( menu, path ):
    
    for action in menu.actions():
        
        text = action.text().replace( '&', '' )
        
        if text == path[ 0 ]:
            
            if len( path ) == 1:
                
                return action
                
            
            return find( action.menu(), path[ 1: ] )
            
        
    
    raise Exception( 'no menu entry ' + repr( path ) )
    

def record( session ):
    
    controller = session.controller
    gui = controller.gui
    
    time.sleep( 5 )
    
    from qtpy import QtWidgets as QW
    
    from hydrus.core import HydrusConstants as HC
    from hydrus.core import HydrusGlobals as HG
    from hydrus.core import HydrusPaths
    from hydrus.client import ClientConstants as CC
    from hydrus.client.gui import ClientGUIDialogsQuick
    
    launched = []
    asked = []
    exits = []
    answer = { 'yes' : False }
    
    HydrusPaths.LaunchDirectory = lambda path: launched.append( path )
    
    def fake_yes_no( parent, text, title = 'Are you sure?', yes_label = 'yes', no_label = 'no', auto_yes_time = None, auto_no_time = None, check_for_cancelled = False, **kwargs ):
        
        asked.append( { 'text' : text, 'title' : title, 'yes_label' : yes_label, 'no_label' : no_label, 'auto_yes_time' : auto_yes_time, 'auto_no_time' : auto_no_time } )
        
        result = QW.QDialog.DialogCode.Accepted if answer[ 'yes' ] else QW.QDialog.DialogCode.Rejected
        
        return ( result, False ) if check_for_cancelled else result
        
    
    ClientGUIDialogsQuick.GetYesNo = fake_yes_no
    
    def fake_yes_no_no( win, message, title = 'Are you sure?', yes_label = 'yes', no_tuples = None, auto_yes_time = None, disable_yes_initially = False ):
        
        asked.append( { 'text' : message, 'title' : title, 'yes_label' : yes_label, 'no_labels' : [ label for ( label, data ) in no_tuples ], 'auto_yes_time' : auto_yes_time, 'auto_no_time' : None, 'disable_yes_initially' : disable_yes_initially, 'kind' : 'yes_no_no' } )
        
        if answer[ 'yes' ]:
            
            return ( QW.QDialog.DialogCode.Accepted, None )
            
        
        return ( QW.QDialog.DialogCode.Rejected, 'no' )
        
    
    ClientGUIDialogsQuick.GetYesNoNo = fake_yes_no_no
    
    # the free space the exit looks at (None: the real one, which is plenty)
    free_space = { 'wanted' : None }
    
    real_space = controller.db.GetSafeTransactionDiskSpaceAndCurrentFreeSpace
    
    def fake_space():
        
        if free_space[ 'wanted' ] is None:
            
            return real_space()
            
        
        return free_space[ 'wanted' ]
        
    
    controller.db.GetSafeTransactionDiskSpaceAndCurrentFreeSpace = fake_space
    
    real_exit = controller.Exit
    
    controller.Exit = lambda: exits.append( { 'restart' : bool( HG.restart ), 'idle_work' : bool( HG.do_idle_shutdown_work ) } )
    
    def trigger( path ):
        
        def f():
            
            find( gui._menubar, path ).trigger()
            
        
        controller.CallBlockingToQt( gui, f )
        
        # (the exit is queued after the question)
        time.sleep( 1.0 )
        
    
    result = {}
    
    # the folders
    trigger( [ 'file', 'open', 'database directory' ] )
    trigger( [ 'file', 'open', 'installation directory' ] )
    
    result[ 'launched' ] = [ { 'is_db_dir' : path == controller.GetDBDir(), 'is_base_dir' : path == HC.BASE_DIR } for path in launched ]
    
    # leaving
    result[ 'exits' ] = []
    
    def scenario( name, entry, confirm, yes, idle_shutdown, work_due, space = None, pages = None ):
        
        del asked[ : ]
        del exits[ : ]
        
        HG.restart = False
        HG.do_idle_shutdown_work = False
        
        HC.options[ 'confirm_client_exit' ] = confirm
        HC.options[ 'idle_shutdown' ] = idle_shutdown
        
        answer[ 'yes' ] = yes
        
        if work_due:
            
            controller.new_options.SetInteger( 'shutdown_work_period', 1 )
            
            time.sleep( 2 )
            
        else:
            
            controller.new_options.SetInteger( 'shutdown_work_period', 86400 * 30 )
            
            controller.Write( 'register_shutdown_work' )
            
            time.sleep( 1 )
            
        
        free_space[ 'wanted' ] = space
        
        notebook = gui._notebook
        
        if pages is not None:
            
            def setup():
                
                from hydrus.client.importing import ClientImportFileSeeds
                
                while notebook.count() > 0:
                    
                    notebook.widget( 0 ).deleteLater()
                    notebook.removeTab( 0 )
                    
                
                location_context = controller.new_options.GetDefaultLocalLocationContext()
                
                notebook.NewPageQuery( location_context, page_name = 'a' )
                
                if 'urls' in pages:
                    
                    urls_page = notebook.NewPageImportURLs()
                    
                    urls_import = urls_page.GetSidebar()._urls_import
                    
                    # (the network is paused, so nothing is fetched, and the
                    # importer still counts as working)
                    if 'paused' in pages:
                        
                        urls_import.PausePlay()
                        
                    
                    urls_import.PendURLs( [ f'https://site.example/post/{n}' for n in range( 1, 5 ) ] )
                    
                
                if 'simple' in pages:
                    
                    simple_page = notebook.NewPageImportSimpleDownloader()
                    
                    simple_import = simple_page.GetSidebar()._simple_downloader_import
                    
                    simple_import._file_seed_cache.AddFileSeeds( [ ClientImportFileSeeds.FileSeed( ClientImportFileSeeds.FILE_SEED_TYPE_URL, f'https://site.example/file/{n}.jpg' ) for n in range( 1, 3 ) ] )
                    
                
                notebook.setCurrentIndex( 0 )
                
            
            controller.CallBlockingToQt( gui, setup )
            
            time.sleep( 1 )
            
        
        trigger( [ 'file', entry ] )
        
        result[ 'exits' ].append( { 'name' : name, 'entry' : entry, 'confirm' : confirm, 'answer' : yes, 'idle_shutdown' : idle_shutdown, 'work_due' : work_due, 'space' : space, 'pages' : pages, 'asked' : list( asked ), 'exited' : list( exits ) } )
        
    
    NEVER = CC.IDLE_NOT_ON_SHUTDOWN
    ALWAYS = CC.IDLE_ON_SHUTDOWN
    
    scenario( 'exit declined', 'exit', True, False, NEVER, False )
    scenario( 'exit accepted', 'exit', True, True, NEVER, False )
    scenario( 'restart declined', 'restart', True, False, NEVER, False )
    scenario( 'restart accepted', 'restart', True, True, NEVER, False )
    scenario( 'exit unconfirmed', 'exit', False, False, NEVER, False )
    scenario( 'restart unconfirmed', 'restart', False, False, NEVER, False )
    scenario( 'force unconfirmed', 'exit/force maintenance', False, False, NEVER, False )
    scenario( 'force confirmed', 'exit/force maintenance', True, True, NEVER, False )
    scenario( 'exit with work due', 'exit', False, False, ALWAYS, True )
    scenario( 'exit with work not due', 'exit', False, False, ALWAYS, False )
    
    # little free space (wanted, free), then pages that hold importers
    LOW = ( 5 * 1048576, 1048576 )
    
    scenario( 'low space backed out', 'exit', False, False, NEVER, False, space = LOW )
    scenario( 'low space tried', 'exit', False, True, NEVER, False, space = LOW )
    scenario( 'low space then confirmed', 'exit', True, True, NEVER, False, space = LOW )
    scenario( 'low space restart backed out', 'restart', True, False, NEVER, False, space = LOW )
    scenario( 'low space enough', 'exit', False, False, NEVER, False, space = ( 5 * 1048576, 5 * 1048576 ) )
    scenario( 'urls declined', 'exit', False, False, NEVER, False, pages = [ 'urls' ] )
    scenario( 'urls accepted', 'exit', False, True, NEVER, False, pages = [ 'urls' ] )
    scenario( 'urls restart accepted', 'restart', True, True, NEVER, False, pages = [ 'urls' ] )
    scenario( 'urls and simple accepted', 'exit', True, True, NEVER, False, pages = [ 'urls', 'simple' ] )
    scenario( 'paused urls', 'exit', False, True, NEVER, False, pages = [ 'urls', 'paused' ] )
    scenario( 'simple declined', 'exit/force maintenance', False, False, NEVER, False, pages = [ 'simple' ] )
    scenario( 'low space and urls', 'exit', True, True, NEVER, False, space = LOW, pages = [ 'urls' ] )
    
    controller.Exit = real_exit
    
    return result
    

def child( out ):
    
    import hydrus_driver
    import record_api
    
    db_dir = record_api.unpack_fixture( 'basic' )
    
    result = hydrus_driver.run_client( db_dir, record )
    
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
            
        
    
    with open( OUT, 'w' ) as f:
        
        json.dump( result, f, indent = 1, ensure_ascii = False )
        f.write( '\n' )
        
    
    print( 'wrote file_menu_exit.json' )
    

if __name__ == '__main__':
    
    main()

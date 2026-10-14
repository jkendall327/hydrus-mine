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
    
    def scenario( name, entry, confirm, yes, idle_shutdown, work_due ):
        
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
            
        
        trigger( [ 'file', entry ] )
        
        result[ 'exits' ].append( { 'name' : name, 'entry' : entry, 'confirm' : confirm, 'answer' : yes, 'idle_shutdown' : idle_shutdown, 'work_due' : work_due, 'asked' : list( asked ), 'exited' : list( exits ) } )
        
    
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

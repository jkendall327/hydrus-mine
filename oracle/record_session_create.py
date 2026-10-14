#!/usr/bin/env python3
"""Record the reference's "enter new network context" editor (network >
data > review session cookies > add).

On the `repositories` fixture, the real `EditNetworkContextPanel` is made as
the sessions review makes it (`limited_types=( web domain, hydrus service )`,
no "act as default" box), starting on a web domain "example.com". Recorded:
the type choices and the note under each, what is enabled for each, the
hydrus service choices, and the network context `GetValue` gives (and the
session the session manager then establishes for it, or its error) for a
range of typed domains and for each service.

Usage: QT_QPA_PLATFORM=offscreen python oracle/record_session_create.py
       (writes fixtures/session_create.json)
"""

import json
import os
import sys
import tempfile

HERE = os.path.dirname( os.path.abspath( __file__ ) )

sys.path.insert( 0, HERE )

import hydrus_driver

DOMAINS = [ 'example.com', 'Example.COM', '  spaced.example  ', 'not a domain!!', '', 'sub.domain.example:8080', 'http://example.com/path' ]


def record( session ):
    
    controller = session.controller
    
    def qt():
        
        from hydrus.client import ClientConstants as CC
        from hydrus.client.gui.networking import ClientGUINetwork as G
        from hydrus.client.networking import ClientNetworkingContexts as C
        
        manager = controller.network_engine.session_manager
        
        start = C.NetworkContext( CC.NETWORK_CONTEXT_DOMAIN, 'example.com' )
        
        panel = G.EditNetworkContextPanel( controller.gui, start, limited_types = ( CC.NETWORK_CONTEXT_DOMAIN, CC.NETWORK_CONTEXT_HYDRUS ), allow_default = False )
        
        out = {
            'types' : [ panel._context_type.itemText( i ) for i in range( panel._context_type.count() ) ],
            'initial' : { 'type' : panel._context_type.GetValue(), 'domain_text' : panel._context_data_text.text(), 'note' : panel._context_type_info.text() },
            'services' : [ panel._context_data_services.itemText( i ) for i in range( panel._context_data_services.count() ) ],
            'default_box_hidden' : not panel._context_data_none.isVisibleTo( panel ),
            'per_type' : [],
            'domains' : [],
            'services_values' : [],
        }
        
        for i in range( panel._context_type.count() ):
            
            panel._context_type.setCurrentIndex( i )
            
            out[ 'per_type' ].append( { 'note' : panel._context_type_info.text(), 'domain_enabled' : panel._context_data_text.isEnabled(), 'services_enabled' : panel._context_data_services.isEnabled() } )
            
        
        panel._context_type.SetValue( CC.NETWORK_CONTEXT_DOMAIN )
        
        for text in DOMAINS:
            
            panel._context_data_text.setText( text )
            
            value = panel.GetValue()
            
            entry = { 'typed' : text, 'context' : [ value.context_type, value.context_data ] }
            
            try:
                
                manager.GetSession( value )
                
                entry[ 'session' ] = True
                
            except Exception as e:
                
                entry[ 'session' ] = False
                entry[ 'error' ] = str( e )
                
            
            entry[ 'contexts_now' ] = sorted( [ n.ToString() for n in manager.GetNetworkContexts() ] )
            
            out[ 'domains' ].append( entry )
            
        
        panel._context_type.SetValue( CC.NETWORK_CONTEXT_HYDRUS )
        
        for i in range( panel._context_data_services.count() ):
            
            panel._context_data_services.setCurrentIndex( i )
            
            value = panel.GetValue()
            
            out[ 'services_values' ].append( { 'name' : panel._context_data_services.itemText( i ), 'context' : [ value.context_type, value.context_data.hex() ] } )
            
        
        panel.deleteLater()
        
        return out
        
    
    return controller.CallBlockingToQt( controller.gui, qt )
    

def main():
    
    if len( sys.argv ) > 1 and sys.argv[ 1 ] == '--child':
        
        import record_api
        
        output_path = sys.argv[ 2 ]
        
        result = hydrus_driver.run_client( record_api.unpack_fixture( 'repositories' ), record )
        
        with open( output_path, 'w' ) as f:
            
            json.dump( result, f, ensure_ascii = False )
            
        
        return
        
    
    with tempfile.TemporaryDirectory() as tmp:
        
        path = os.path.join( tmp, 'result.json' )
        
        hydrus_driver.run_in_subprocess( os.path.abspath( __file__ ), '--child', path )
        
        with open( path ) as f:
            
            result = json.load( f )
            
        
    
    with open( os.path.join( HERE, 'fixtures', 'session_create.json' ), 'w' ) as f:
        
        json.dump( result, f, indent = 1, ensure_ascii = False )
        f.write( '\n' )
        
    
    print( 'wrote session_create.json' )
    

if __name__ == '__main__':
    
    main()

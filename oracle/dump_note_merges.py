#!/usr/bin/env python3
"""Record the reference implementation's note merging ("merge cleverly" in
/add_notes/set_notes, and note import options) on random small cases.

Usage: python oracle/dump_note_merges.py > oracle/fixtures/note_merges.json
"""
import json
import os
import random
import sys

sys.path.insert( 0, os.path.join( os.path.dirname( __file__ ), '..' ) )

from hydrus.client.importing.options import NoteImportOptions

rng = random.Random( 688 )

NAMES = [ 'a', 'ab', 'a (1)', 'b', 'comment', 'comment (1)', 'é' ]
TEXTS = [ '', 'x', 'xy', 'xyz', 'hello', 'hello again', 'again', 'yx', 'é', 'héllo' ]

cases = []

for _ in range( 1500 ):
    
    existing = { rng.choice( NAMES ) : rng.choice( TEXTS[ 1: ] ) for _ in range( rng.randint( 0, 3 ) ) }
    incoming = [ ( rng.choice( NAMES ), rng.choice( TEXTS ) ) for _ in range( rng.randint( 1, 3 ) ) ]
    extend = rng.random() < 0.5
    conflict = rng.choice( list( NoteImportOptions.note_import_conflict_str_lookup.keys() ) )
    
    options = NoteImportOptions.NoteImportOptions()
    options.SetExtendExistingNoteIfPossible( extend )
    options.SetConflictResolution( conflict )
    
    result = options.GetUpdateeNamesToNotes( existing, incoming )
    
    cases.append( {
        'existing': existing,
        'incoming': incoming,
        'extend_existing_note_if_possible': extend,
        'conflict_resolution': conflict,
        'result': result,
    } )
    

# name whitelists and renames, from a separate stream so the cases above
# stay as they were
rng = random.Random( 6880 )

for _ in range( 500 ):

    existing = { rng.choice( NAMES ) : rng.choice( TEXTS[ 1: ] ) for _ in range( rng.randint( 0, 3 ) ) }
    incoming = [ ( rng.choice( NAMES ), rng.choice( TEXTS ) ) for _ in range( rng.randint( 1, 3 ) ) ]
    extend = rng.random() < 0.5
    conflict = rng.choice( list( NoteImportOptions.note_import_conflict_str_lookup.keys() ) )
    whitelist = sorted( set( rng.sample( NAMES, rng.randint( 0, 3 ) ) ) ) if rng.random() < 0.5 else []
    overrides = { rng.choice( NAMES ) : rng.choice( NAMES ) for _ in range( rng.randint( 0, 2 ) ) }
    all_override = rng.choice( NAMES ) if rng.random() < 0.25 else None

    options = NoteImportOptions.NoteImportOptions()
    options.SetExtendExistingNoteIfPossible( extend )
    options.SetConflictResolution( conflict )
    options.SetNameWhitelist( whitelist )
    options.SetNamesToNameOverrides( overrides )
    options.SetAllNameOverride( all_override )

    result = options.GetUpdateeNamesToNotes( existing, incoming )

    cases.append( {
        'existing': existing,
        'incoming': incoming,
        'extend_existing_note_if_possible': extend,
        'conflict_resolution': conflict,
        'name_whitelist': whitelist,
        'names_to_name_overrides': overrides,
        'all_name_override': all_override,
        'result': result,
    } )


json.dump( { 'conflict_resolutions': { str( k ) : v for ( k, v ) in NoteImportOptions.note_import_conflict_str_lookup.items() }, 'cases': cases }, sys.stdout, indent = 1, ensure_ascii = False, sort_keys = True )
sys.stdout.write( '\n' )

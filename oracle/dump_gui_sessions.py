#!/usr/bin/env python3
"""Record how the reference reads GUI sessions and the pages in them.

Random downloader importers (URL pages, gallery searches and their pages,
watchers and their pages), page managers of every page type hydrus-rs cares
about, and session trees, built with the reference's classes. Each case
keeps:

* `stored`: the serialised tuple as a database holds it
* `facts`: the loaded object's fields, read with the reference's own
  attributes

Sessions are kept as the database holds them: the container (a tree of
notebooks and pages, each page by the hash of its data) and each page's data
under its hash, so a decoder can put them together as the reference does.

Usage: python oracle/dump_gui_sessions.py > oracle/fixtures/gui_sessions.json
"""

import json
import os
import sys

sys.path.insert( 0, os.path.dirname( __file__ ) )

import dump_subscriptions as S # the fake controller, seeds and their facts

from hydrus.core import HydrusSerialisable
from hydrus.client import ClientConstants as CC
from hydrus.client import ClientLocation
from hydrus.client.importing import ClientImportGallery
from hydrus.client.importing import ClientImporting
from hydrus.client.importing import ClientImportSimpleURLs
from hydrus.client.importing import ClientImportWatchers
from hydrus.client.importing.options import ImportOptionsContainer
from hydrus.client.importing.options import NoteImportOptions
from hydrus.client.media import ClientMediaCollect
from hydrus.client.media import ClientMediaSort
from hydrus.client.search import ClientSearchFileSearchContext
from hydrus.client.search import ClientSearchPredicate
from hydrus.client.search import ClientSearchTagContext
from hydrus.client.gui.pages import ClientGUIPageManager
from hydrus.client.gui.pages import ClientGUIPagesCore
from hydrus.client.gui.pages import ClientGUISession

from hydrus.client import ClientGlobals as CG

# importers subscribe to the controller's notifications
CG.client_controller.sub = lambda *args, **kwargs: None

rng = S.rng


def tuple_of( obj ):

    return json.loads( json.dumps( obj.GetSerialisableTuple() ) )


def read_back( stored ):

    return HydrusSerialisable.CreateFromSerialisableTuple( tuple( stored ) )


def import_options_container():

    container = ImportOptionsContainer.ImportOptionsContainer()

    if rng.random() < 0.5:

        note_options = NoteImportOptions.NoteImportOptions()
        note_options.SetGetNotes( rng.random() < 0.5 )
        note_options.SetNameWhitelist( S.some( [ 'comment', 'artist note' ] ) )

        container.SetImportOptions( note_options )


    return container


def fill( file_seed_cache, gallery_seed_log, i ):

    file_seed_cache.AddFileSeeds( [ S.file_seed( i * 10 + k, in_cache = True ) for k in range( rng.randint( 0, 5 ) ) ] )
    gallery_seed_log.AddGallerySeeds( [ S.gallery_seed( i * 10 + k ) for k in range( rng.randint( 0, 3 ) ) ] )


def seeds_facts( file_seed_cache, gallery_seed_log ):

    return {
        'file_seeds' : [ S.file_seed_facts( f ) for f in file_seed_cache.GetFileSeeds() ],
        'gallery_seeds' : [ S.gallery_seed_facts( g ) for g in gallery_seed_log.GetGallerySeeds() ],
    }


# URL pages

def urls_import( i ):

    u = ClientImportSimpleURLs.URLsImport( import_options_container = import_options_container() )

    fill( u._file_seed_cache, u._gallery_seed_log, i )
    u._paused = rng.random() < 0.3

    return u


def urls_import_facts( u ):

    return dict(
        seeds_facts( u._file_seed_cache, u._gallery_seed_log ),
        paused = u._paused,
        import_options = tuple_of( u._import_options_container ),
    )


# gallery searches and gallery pages

def gallery_import( i ):

    g = ClientImportGallery.GalleryImport(
        query = rng.choice( [ 'blue_eyes', 'samus_aran solo', 'キャラクター' ] ) + str( i ),
        source_name = rng.choice( [ 'safebooru tag search', 'gelbooru tag search' ] ),
        start_file_queue_paused = rng.random() < 0.3,
        start_gallery_queue_paused = rng.random() < 0.3
    )

    fill( g._file_seed_cache, g._gallery_seed_log, i )
    g._creation_time = S.when()
    g._current_page_index = rng.randint( 0, 5 )
    g._num_urls_found = rng.randint( 0, 500 )
    g._num_new_urls_found = rng.randint( 0, g._num_urls_found )
    g._file_limit = rng.choice( [ None, 100, 1000 ] )
    g._no_work_until = rng.choice( [ 0, S.when() ] )
    g._no_work_until_reason = rng.choice( [ '', 'network error: 503' ] )
    g._import_options_container = import_options_container()

    return g


def gallery_import_facts( g ):

    return dict(
        seeds_facts( g._file_seed_cache, g._gallery_seed_log ),
        key = g._gallery_import_key.hex(),
        created = g._creation_time,
        query = g._query,
        source_name = g._source_name,
        current_page_index = g._current_page_index,
        num_urls_found = g._num_urls_found,
        num_new_urls_found = g._num_new_urls_found,
        file_limit = g._file_limit,
        gallery_paused = g._gallery_paused,
        files_paused = g._files_paused,
        no_work_until = g._no_work_until,
        no_work_until_reason = g._no_work_until_reason,
        import_options = tuple_of( g._import_options_container ),
    )


def multiple_gallery_import( i ):

    m = ClientImportGallery.MultipleGalleryImport( gug_key_and_name = ( bytes( rng.randrange( 256 ) for _ in range( 32 ) ), rng.choice( [ 'safebooru tag search', 'gelbooru tag search' ] ) ) )

    for k in range( rng.randint( 0, 3 ) ):

        m._gallery_imports.append( gallery_import( i * 10 + k ) )


    if len( m._gallery_imports ) > 0 and rng.random() < 0.5:

        m._highlighted_gallery_import_key = m._gallery_imports[0]._gallery_import_key


    m._file_limit = rng.choice( [ None, 100, 1000 ] )
    m._start_file_queues_paused = rng.random() < 0.3
    m._start_gallery_queues_paused = rng.random() < 0.3
    m._merge_simultaneous_pends_to_one_importer = rng.random() < 0.3
    m._do_not_allow_new_dupes = rng.random() < 0.3
    m._import_options_container = import_options_container()

    return m


def multiple_gallery_import_facts( m ):

    ( gug_key, gug_name ) = m._gug_key_and_name

    return {
        'gug_key' : gug_key.hex(),
        'gug_name' : gug_name,
        'highlighted' : None if m._highlighted_gallery_import_key is None else m._highlighted_gallery_import_key.hex(),
        'file_limit' : m._file_limit,
        'start_file_queues_paused' : m._start_file_queues_paused,
        'start_gallery_queues_paused' : m._start_gallery_queues_paused,
        'do_not_allow_new_dupes' : m._do_not_allow_new_dupes,
        'merge_simultaneous_pends_to_one_importer' : m._merge_simultaneous_pends_to_one_importer,
        'import_options' : tuple_of( m._import_options_container ),
        'gallery_imports' : [ gallery_import_facts( g ) for g in m._gallery_imports ],
    }


# watchers and watcher pages

def watcher_import( i ):

    w = ClientImportWatchers.WatcherImport()

    w._url = rng.choice( [ f'https://boards.4chan.org/a/thread/{i}', f'https://8chan.moe/v/res/{i}.html', '' ] )
    fill( w._file_seed_cache, w._gallery_seed_log, i )
    w._external_filterable_tags = set( S.some( S.TAGS ) )
    w._external_additional_service_keys_to_tags = S.service_keys_to_tags()
    w._checker_options = S.checker_options()
    w._import_options_container = import_options_container()
    w._last_check_time = rng.choice( [ 0, S.when() ] )
    w._files_paused = rng.random() < 0.3
    w._checking_paused = rng.random() < 0.3
    w._checking_status = rng.choice( [ ClientImporting.CHECKER_STATUS_OK, ClientImporting.CHECKER_STATUS_DEAD, ClientImporting.CHECKER_STATUS_404 ] )
    w._subject = rng.choice( [ 'unknown subject', 'a thread about cats', 'スレッド' ] )
    w._no_work_until = rng.choice( [ 0, S.when() ] )
    w._no_work_until_reason = rng.choice( [ '', 'network error: 503' ] )
    w._creation_time = S.when()

    return w


def watcher_import_facts( w ):

    return dict(
        seeds_facts( w._file_seed_cache, w._gallery_seed_log ),
        url = w._url,
        filterable = sorted( w._external_filterable_tags ),
        additional = S.sorted_tags( w._external_additional_service_keys_to_tags ),
        checker_options = tuple_of( w._checker_options ),
        import_options = tuple_of( w._import_options_container ),
        last_check_time = w._last_check_time,
        files_paused = w._files_paused,
        checking_paused = w._checking_paused,
        checking_status = w._checking_status,
        subject = w._subject,
        no_work_until = w._no_work_until,
        no_work_until_reason = w._no_work_until_reason,
        created = w._creation_time,
    )


def multiple_watcher_import( i ):

    m = ClientImportWatchers.MultipleWatcherImport( import_options_container = import_options_container() )

    for k in range( rng.randint( 0, 3 ) ):

        m._watchers.append( watcher_import( i * 10 + k ) )


    if len( m._watchers ) > 0 and rng.random() < 0.5:

        m._highlighted_watcher_url = m._watchers[0]._url


    m._checker_options = S.checker_options()

    return m


def multiple_watcher_import_facts( m ):

    return {
        'highlighted' : m._highlighted_watcher_url,
        'checker_options' : tuple_of( m._checker_options ),
        'import_options' : tuple_of( m._import_options_container ),
        'watchers' : [ watcher_import_facts( w ) for w in m._watchers ],
    }


# pages

def base_page( name, page_type ):

    page = ClientGUIPageManager.PageManager( name )
    page.SetType( page_type )
    page.SetVariable( 'media_sort', ClientMediaSort.MediaSort( ( 'system', rng.choice( [ CC.SORT_FILES_BY_IMPORT_TIME, CC.SORT_FILES_BY_FILESIZE ] ) ), rng.choice( [ CC.SORT_ASC, CC.SORT_DESC ] ) ) )
    page.SetVariable( 'media_collect', ClientMediaCollect.MediaCollect() )

    return page


def page_manager( i ):

    kind = rng.choice( [ 'query', 'urls', 'gallery', 'watcher' ] )

    if kind == 'query':

        page = base_page( rng.choice( [ 'files', 'my search', 'ファイル' ] ), ClientGUIPagesCore.PAGE_TYPE_QUERY )

        location_context = ClientLocation.LocationContext.STATICCreateSimple( CC.COMBINED_LOCAL_FILE_DOMAINS_SERVICE_KEY )
        tag_context = ClientSearchTagContext.TagContext()
        predicates = [ ClientSearchPredicate.Predicate( ClientSearchPredicate.PREDICATE_TYPE_TAG, tag ) for tag in S.some( S.TAGS ) ]

        page.SetVariable( 'file_search_context', ClientSearchFileSearchContext.FileSearchContext( location_context = location_context, tag_context = tag_context, predicates = predicates ) )
        page.SetVariable( 'synchronised', rng.random() < 0.7 )
        page.SetVariable( 'system_hash_locked', False )
        page.SetVariable( 'system_hash_locked_syncs_new', True )
        page.SetVariable( 'system_hash_locked_syncs_removes', True )

    elif kind == 'urls':

        page = base_page( rng.choice( [ 'url import', 'HC-bookmarks' ] ), ClientGUIPagesCore.PAGE_TYPE_IMPORT_URLS )
        page.SetVariable( 'urls_import', urls_import( i ) )

    elif kind == 'gallery':

        page = base_page( rng.choice( [ 'gallery', 'safebooru' ] ), ClientGUIPagesCore.PAGE_TYPE_IMPORT_MULTIPLE_GALLERY )
        page.SetVariable( 'multiple_gallery_import', multiple_gallery_import( i ) )

    else:

        page = base_page( rng.choice( [ 'watcher', 'threads' ] ), ClientGUIPagesCore.PAGE_TYPE_IMPORT_MULTIPLE_WATCHER )
        page.SetVariable( 'multiple_watcher_import', multiple_watcher_import( i ) )


    return page


def duplicates_page( i ):
    """A duplicates page (made after the others, so their random draws are
    unchanged), with a potential-duplicates search, sort and group mode."""

    from hydrus.client.duplicates import ClientPotentialDuplicatesSearchContext

    page = base_page( rng.choice( [ 'duplicates', 'dupes' ] ), ClientGUIPagesCore.PAGE_TYPE_DUPLICATE_FILTER )

    location_context = ClientLocation.LocationContext.STATICCreateSimple( rng.choice( [ CC.COMBINED_LOCAL_FILE_DOMAINS_SERVICE_KEY, CC.LOCAL_FILE_SERVICE_KEY ] ) )

    predicates = [ ClientSearchPredicate.Predicate( ClientSearchPredicate.PREDICATE_TYPE_TAG, tag ) for tag in S.some( S.TAGS ) ]

    if len( predicates ) == 0:

        predicates = [ ClientSearchPredicate.Predicate( ClientSearchPredicate.PREDICATE_TYPE_SYSTEM_EVERYTHING ) ]


    search = ClientPotentialDuplicatesSearchContext.PotentialDuplicatesSearchContext( location_context = location_context, initial_predicates = predicates )

    search.SetDupeSearchType( rng.choice( [ 0, 1, 2 ] ) )
    search.SetPixelDupesPreference( rng.choice( [ 0, 1, 2 ] ) )
    search.SetMaxHammingDistance( rng.choice( [ 0, 4, 8 ] ) )

    page.SetVariable( 'synchronised', rng.random() < 0.7 )
    page.SetVariable( 'potential_duplicates_search_context', search )
    page.SetVariable( 'duplicate_pair_sort_type', rng.choice( [ 0, 1, 2, 3 ] ) )
    page.SetVariable( 'duplicate_pair_sort_asc', rng.random() < 0.5 )
    page.SetVariable( 'filter_group_mode', rng.random() < 0.5 )

    return page


def page_manager_facts( page ):

    facts = { 'name' : page.GetPageName(), 'type' : page.GetType() }

    describe = {
        'urls_import' : urls_import_facts,
        'multiple_gallery_import' : multiple_gallery_import_facts,
        'multiple_watcher_import' : multiple_watcher_import_facts,
    }

    variables = {}

    for ( name, value ) in page._variables.items():

        if name in describe:

            variables[ name ] = describe[ name ]( value )

        elif isinstance( value, HydrusSerialisable.SerialisableBase ):

            variables[ name ] = tuple_of( value )

        else:

            variables[ name ] = value



    facts[ 'variables' ] = variables

    return facts


# sessions

def session( i ):

    hashes_to_page_data = {}

    def page_container( depth ):

        if depth < 2 and rng.random() < 0.3:

            return ClientGUISession.GUISessionContainerPageNotebook( rng.choice( [ 'pages', 'downloaders' ] ), [ page_container( depth + 1 ) for _ in range( rng.randint( 0, 3 ) ) ] )


        page = page_manager( i * 100 + len( hashes_to_page_data ) )
        hashes = [ bytes( rng.randrange( 256 ) for _ in range( 32 ) ) for _ in range( rng.randint( 0, 3 ) ) ]

        page_data = ClientGUISession.GUISessionPageData( page, hashes )
        page_data_hash = page_data.GetSerialisedHash()

        hashes_to_page_data[ page_data_hash ] = page_data

        return ClientGUISession.GUISessionContainerPageSingle( page.GetPageName(), page_data_hash )


    top = ClientGUISession.GUISessionContainerPageNotebook( 'top', [ page_container( 1 ) for _ in range( rng.randint( 1, 4 ) ) ] )

    container = ClientGUISession.GUISessionContainer( rng.choice( [ CC.LAST_SESSION_SESSION_NAME, 'exit session', 'my saved session' ] ), top_notebook_container = top )

    def tree_facts( c ):

        if isinstance( c, ClientGUISession.GUISessionContainerPageNotebook ):

            return { 'name' : c.GetName(), 'pages' : [ tree_facts( p ) for p in c.GetPageContainers() ] }


        return { 'name' : c.GetName(), 'page_data_hash' : c.GetPageDataHash().hex() }


    return {
        'container' : tuple_of( container ),
        'page_data' : { h.hex() : tuple_of( d ) for ( h, d ) in hashes_to_page_data.items() },
        'facts' : {
            'name' : container.GetName(),
            'tree' : tree_facts( container.GetTopNotebook() ),
            'pages' : { h.hex() : { 'page' : page_manager_facts( d.GetPageManager() ), 'hashes' : [ x.hex() for x in d.GetHashes() ] } for ( h, d ) in hashes_to_page_data.items() },
        },
    }


def cases( make, facts, count ):

    out = []

    for i in range( count ):

        stored = tuple_of( make( i ) )
        loaded = read_back( stored )

        out.append( { 'stored' : stored, 'facts' : facts( loaded ) } )


    return out


def main():

    fixture = {
        'urls_imports' : cases( urls_import, urls_import_facts, 8 ),
        'gallery_imports' : cases( gallery_import, gallery_import_facts, 8 ),
        'multiple_gallery_imports' : cases( multiple_gallery_import, multiple_gallery_import_facts, 5 ),
        'watcher_imports' : cases( watcher_import, watcher_import_facts, 8 ),
        'multiple_watcher_imports' : cases( multiple_watcher_import, multiple_watcher_import_facts, 5 ),
        'page_managers' : cases( page_manager, page_manager_facts, 12 ),
        'sessions' : [ session( i ) for i in range( 4 ) ],
    }

    fixture[ 'duplicates_pages' ] = cases( duplicates_page, page_manager_facts, 6 )

    json.dump( fixture, sys.stdout, indent = 1, sort_keys = True, ensure_ascii = False )
    sys.stdout.write( '\n' )


if __name__ == '__main__':

    main()

#!/usr/bin/env python3
"""Record real SQLite HTA/HTPA interoperability, Qt inspectors and count gates.

--rust-executable is the bounded standalone helper including the current native
archive.rs (cached third-party SQLite only). Python creates actual archives,
Rust reads/writes them in three-entry batches, and Python reads native output.
The live client's actual migration source converts all four hash kinds, applies
file/tag scopes, and filters sibling/parent pairs against current/pending counts.
"""
import argparse
import json
import os
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path
HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
from hydrus_driver import run_client
import record_api


def record(session, executable):
    from hydrus.core import HydrusConstants as HC, HydrusTags, HydrusTagArchive as A
    from hydrus.client import ClientMigration as M, ClientConstants as CC, ClientLocation
    from hydrus.client.metadata import ClientContentUpdates as U
    controller = session.controller
    local = controller.services_manager.GetServices((HC.LOCAL_TAG,))[0].GetServiceKey()
    repo = controller.services_manager.GetServices((HC.TAG_REPOSITORY,))[0].GetServiceKey()
    location = ClientLocation.LocationContext.STATICCreateSimple(CC.COMBINED_FILE_SERVICE_KEY)
    source = M.MigrationSourceTagServiceMappings(controller, local, location, 'sha256', None, HydrusTags.TagFilter(), (HC.CONTENT_STATUS_CURRENT,))
    source.Prepare()
    rows = []
    while source.StillWorkToDo(): rows.extend(source.GetSomeData())
    source.CleanUp()
    known = sorted(h for h, tags in rows)[:2]
    result = {'archives': [], 'conversion': [], 'counts': [], 'qt': []}
    for kind in ('sha256', 'md5', 'sha1', 'sha512'):
        path = HERE / 'fixtures' / ('tag_archive_' + kind + '.db')
        path.unlink(missing_ok=True)
        archive = A.HydrusTagArchive(str(path))
        archive.SetHashType(A.hash_str_to_type_lookup[kind])
        input_rows = []
        for i, sha in enumerate(known):
            converted = sha if kind == 'sha256' else controller.Read('file_hashes', (sha,), 'sha256', kind).get(sha)
            assert converted is not None, kind
            tags = ['archive:known-' + str(i), 'creator:猫', 'plain']
            archive.AddMappings(converted, tags)
            archive.AddMapping(converted, tags[0])
            input_rows.append({'hash': converted.hex(), 'tags': sorted(tags)})
        unknown = bytes([240]) * {'sha256': 32, 'md5': 16, 'sha1': 20, 'sha512': 64}[kind]
        archive.AddMapping(unknown, 'archive:unknown')
        input_rows.append({'hash': unknown.hex(), 'tags': ['archive:unknown']})
        archive.Close()
        with tempfile.TemporaryDirectory() as tmp:
            native = Path(tmp) / 'native.db'
            subprocess.run([executable, 'mappings', str(path), str(native)], check=True, timeout=15)
            loaded = A.HydrusTagArchive(str(native))
            actual = sorted([{'hash':h.hex(), 'tags':sorted(tags)} for h,tags in loaded.IterateMappings()], key=lambda v:v['hash'])
            assert actual == sorted(input_rows,key=lambda v:v['hash'])
            assert loaded.GetHashType() == A.hash_str_to_type_lookup[kind]
            namespaces = sorted(loaded.GetNamespaces())
            loaded.Close()
            result['archives'].append({'kind':kind, 'file':path.name, 'mappings':actual,'namespaces':namespaces,'native_loaded_by_python':True})
        for desired in ('sha256', 'md5', 'sha1', 'sha512'):
            for selected in (False, True):
                tag_filter = HydrusTags.TagFilter()
                tag_filter.SetRule('creator:', HC.FILTER_BLACKLIST)
                source = M.MigrationSourceHTA(controller, str(path), location, desired, set(known[:1]) if selected else None, tag_filter)
                source.Prepare()
                actual = []
                while source.StillWorkToDo(): actual.extend(source.GetSomeData())
                source.CleanUp()
                result['conversion'].append({'kind':kind,'desired':desired,'selected':selected,'hashes':[h.hex() for h in known[:1]] if selected else [],'mappings':sorted([{'hash':h.hex(),'tags':sorted(tags)} for h,tags in actual],key=lambda v:v['hash'])})
    domain=controller.services_manager.GetServices((HC.LOCAL_FILE_DOMAIN,))[0].GetServiceKey()
    controller.WriteSynchronous('content_updates',U.ContentUpdatePackage.STATICCreateFromContentUpdates(domain,[U.ContentUpdate(HC.CONTENT_TYPE_FILES,HC.CONTENT_UPDATE_DELETE,{known[1]},reason='archive domain oracle')]))
    media=controller.Read('media_results',known)
    result['domain_membership']=[{'hash':m.GetHash().hex(),'current':domain in m.GetLocationsManager().GetCurrent(),'deleted':domain in m.GetLocationsManager().GetDeleted()} for m in media]
    assert any(row['current'] for row in result['domain_membership'])
    assert any(row['deleted'] for row in result['domain_membership'])
    for kind in ('sha256','md5','sha1','sha512'):
        for status in ('current','deleted'):
            scoped=ClientLocation.LocationContext(current_service_keys=(domain,) if status=='current' else (),deleted_service_keys=(domain,) if status=='deleted' else ())
            tag_filter=HydrusTags.TagFilter();tag_filter.SetRule('creator:',HC.FILTER_BLACKLIST)
            source=M.MigrationSourceHTA(controller,str(HERE/'fixtures'/('tag_archive_'+kind+'.db')),scoped,'sha256',None,tag_filter)
            source.Prepare();actual=[]
            while source.StillWorkToDo():actual.extend(source.GetSomeData())
            source.CleanUp()
            result['conversion'].append({'kind':kind,'desired':'sha256','selected':False,'domain':status,'hashes':[],'mappings':sorted([{'hash':h.hex(),'tags':sorted(tags)} for h,tags in actual],key=lambda v:v['hash'])})
    for content, label, pair_type in [(HC.CONTENT_TYPE_TAG_SIBLINGS,'siblings',A.TAG_PAIR_TYPE_SIBLINGS),(HC.CONTENT_TYPE_TAG_PARENTS,'parents',A.TAG_PAIR_TYPE_PARENTS)]:
        path=HERE/'fixtures'/('tag_archive_'+label+'.db')
        path.unlink(missing_ok=True)
        archive=A.HydrusTagPairArchive(str(path))
        archive.SetPairType(pair_type)
        pairs=[('archive:left','archive:right'),('archive:right','archive:ideal'),('archive:empty','archive:none'),('archive:pending','archive:none')]
        archive.AddPairs(pairs + pairs[:1])
        archive.Close()
        with tempfile.TemporaryDirectory() as tmp:
            native=Path(tmp)/'native.db'
            subprocess.run([executable,label,str(path),str(native)],check=True,timeout=15)
            loaded=A.HydrusTagPairArchive(str(native))
            actual=sorted([list(p) for p in loaded.IteratePairs()])
            assert actual==sorted([list(p) for p in pairs])
            assert loaded.GetPairType()==pair_type
            loaded.Close()
            result['archives'].append({'kind':label,'file':path.name,'pairs':actual,'native_loaded_by_python':True})
    # Count inputs deliberately separate right from terminal ideal and real
    # storage mappings from implied display counts, with a pending-only tag.
    updates=[U.ContentUpdate(HC.CONTENT_TYPE_MAPPINGS,HC.CONTENT_UPDATE_ADD,('archive:left',{known[0]})),U.ContentUpdate(HC.CONTENT_TYPE_MAPPINGS,HC.CONTENT_UPDATE_ADD,('archive:ideal',{known[0]})),U.ContentUpdate(HC.CONTENT_TYPE_TAG_SIBLINGS,HC.CONTENT_UPDATE_ADD,('archive:right','archive:ideal'))]
    controller.WriteSynchronous('content_updates',U.ContentUpdatePackage.STATICCreateFromContentUpdates(local,updates))
    controller.WriteSynchronous('content_updates',U.ContentUpdatePackage.STATICCreateFromContentUpdates(repo,[U.ContentUpdate(HC.CONTENT_TYPE_MAPPINGS,HC.CONTENT_UPDATE_PEND,('archive:pending',{known[0]}))]))
    for content,label in [(HC.CONTENT_TYPE_TAG_SIBLINGS,'siblings'),(HC.CONTENT_TYPE_TAG_PARENTS,'parents')]:
        for service_name,key in [('local',local),('repository',repo)]:
            for left,right,either in [(True,False,False),(False,True,False),(True,True,False),(False,False,True)]:
                source=M.MigrationSourceHTPA(controller,str(HERE/'fixtures'/('tag_archive_'+label+'.db')),content,HydrusTags.TagFilter(),HydrusTags.TagFilter(),left,right,either,key)
                source.Prepare()
                actual=[]
                while source.StillWorkToDo(): actual.extend(source.GetSomeData())
                source.CleanUp()
                result['counts'].append({'content':label,'service':service_name,'left':left,'right':right,'either':either,'pairs':sorted([list(p) for p in actual])})
    # Bound the actual source pull to the native test's single-row batch,
    # retaining the reference GetSomeData/filter-count implementation.
    from hydrus.core import HydrusLists
    pull=HydrusLists.PullNFromIterator
    HydrusLists.PullNFromIterator=lambda iterator,n:pull(iterator,1)
    source=M.MigrationSourceHTPA(controller,str(HERE/'fixtures/tag_archive_parents.db'),HC.CONTENT_TYPE_TAG_PARENTS,HydrusTags.TagFilter(),HydrusTags.TagFilter(),True,False,False,local)
    source.Prepare();dynamic=[]
    try:
        dynamic.extend(source.GetSomeData())
        controller.WriteSynchronous('content_updates',U.ContentUpdatePackage.STATICCreateFromContentUpdates(local,[U.ContentUpdate(HC.CONTENT_TYPE_MAPPINGS,HC.CONTENT_UPDATE_ADD,('archive:right',{known[0]}))]))
        while source.StillWorkToDo():dynamic.extend(source.GetSomeData())
    finally:
        source.CleanUp();HydrusLists.PullNFromIterator=pull
    result['dynamic_counts']={'batch_size':1,'added_after_first_batch':'archive:right','left':True,'right':False,'either':False,'pairs':sorted([list(p) for p in dynamic])}
    def qt():
        from qtpy import QtWidgets as QW
        from hydrus.client.gui.metadata.ClientGUIMigrateTags import MigrateTagsPanel
        from hydrus.client.gui import ClientGUIDialogsFiles as F, ClientGUIDialogsMessage as D, ClientGUIDialogsQuick as Q
        chosen=[]
        messages=[]
        class Dialog:
            def __init__(self,*args,**kwargs):
                self.path,self.accept=chosen.pop(0)
                result.setdefault('picker_requests',[]).append({'message':kwargs.get('message'),'accept_mode':kwargs.get('acceptMode',QW.QFileDialog.AcceptMode.AcceptOpen).name,'accepted':self.accept})
            def __enter__(self): return self
            def __exit__(self,*args): pass
            def exec(self): return QW.QDialog.DialogCode.Accepted if self.accept else QW.QDialog.DialogCode.Rejected
            def GetPath(self): return self.path
        old_dialog,old_warning,old_yes=F.FileDialog,D.ShowWarning,Q.GetYesNo
        questions=[]
        def question(parent,message,**kwargs):
            questions.append(message)
            return QW.QDialog.DialogCode.Rejected
        Q.GetYesNo=question
        F.FileDialog=Dialog
        D.ShowWarning=lambda parent,message:messages.append(message)
        panel=MigrateTagsPanel(controller.gui,local)
        try:
            for content,label,source_key in [(HC.CONTENT_TYPE_MAPPINGS,'sha256',panel.HTA_SERVICE_KEY),(HC.CONTENT_TYPE_MAPPINGS,'md5',panel.HTA_SERVICE_KEY),(HC.CONTENT_TYPE_TAG_SIBLINGS,'siblings',panel.HTPA_SERVICE_KEY),(HC.CONTENT_TYPE_TAG_PARENTS,'parents',panel.HTPA_SERVICE_KEY)]:
                panel._migration_content_type.SetValue(content)
                panel._UpdateMigrationControlsNewType()
                panel._migration_source.SetValue(source_key)
                panel._UpdateMigrationControlsNewSource()
                panel._migration_destination.SetValue(source_key)
                panel._UpdateMigrationControlsNewDestination()
                path=str(HERE/'fixtures'/('tag_archive_'+label+'.db'))
                chosen.append((path,True));panel._SetSourceArchivePath()
                chosen.append((path,True));panel._SetDestinationArchivePath()
                panel._migration_source_location_context_button.SetValue(location)
                panel._MigrationGo()
                before=panel._migration_source_archive_path_button.text()
                chosen.append(('',False));panel._SetSourceArchivePath()
                result['qt'].append({'kind':label,'source':before,'source_hash':panel._migration_source_hash_type_st.text(),'destination_hash':panel._migration_destination_hash_type_choice.GetValue(),'destination_hash_enabled':panel._migration_destination_hash_type_choice.isEnabled(),'statuses':[panel._migration_source_content_status_filter.itemText(i) for i in range(panel._migration_source_content_status_filter.count())],'actions':[panel._migration_action.itemText(i) for i in range(panel._migration_action.count())],'cancel_preserved':panel._migration_source_archive_path_button.text()==before,'confirmation':questions[-1]})
            chosen.append((str(HERE/'fixtures/tag_archive_siblings.db'),True));panel._SetSourceArchivePath()
            assert messages[-1]=='This Hydrus Tag Pair Archive is not a tag parents archive!'
            result['qt_warnings']=messages
        finally:
            panel.deleteLater();F.FileDialog=old_dialog;D.ShowWarning=old_warning;Q.GetYesNo=old_yes
    controller.CallBlockingToQt(controller.gui,qt)
    result['known_sha256']=[h.hex() for h in known]
    (HERE/'fixtures/tag_archives.json').write_text(json.dumps(result,indent=2)+'\n')


if __name__=='__main__':
    parser=argparse.ArgumentParser()
    parser.add_argument('--rust-executable',required=True)
    args=parser.parse_args()
    db=record_api.unpack_fixture('repositories')
    try:run_client(db,lambda session:record(session,args.rust_executable))
    finally:shutil.rmtree(db)

#!/usr/bin/env python3
"""Global missing-archive scan questions and real writes on a synthetic copied DB.

Only the freshly unpacked fixture is seeded before controller startup. All calls
then use actual Qt Database repair dispatch, actual reads/content writes and live
media timestamp consumers. Async scheduling and question choices are scripted;
no reference methods or conversion rules are replaced.
"""
import json
import os
import sqlite3
import sys
import tempfile
HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0,HERE)
OUT = os.path.join(HERE,'fixtures','archive_time_repair.json')
MAGIC = 1644991200000

def seed(path):
    manifest = json.load(open(os.path.join(HERE,'fixtures','legacy_db','basic.manifest.json')))
    hashes = [r['hash'] for r in manifest['files'][:10]]
    c = sqlite3.connect(os.path.join(path,'client.db'))
    c.execute('ATTACH DATABASE ? AS master', (os.path.join(path,'client.master.db'),))
    ids = [c.execute('SELECT hash_id FROM master.hashes WHERE hash=?',(bytes.fromhex(h),)).fetchone()[0] for h in hashes]
    services = dict(c.execute('SELECT service_type,service_id FROM services WHERE service_type IN(15,21,14)'))
    storage,media,trash = services[15],services[21],services[14]
    # Existing non-corpus records are given valid times to isolate the global scan.
    c.execute('INSERT OR REPLACE INTO archive_timestamps SELECT hash_id,? FROM master.hashes',(MAGIC+777,))
    imports=[MAGIC-1000,MAGIC+1000,MAGIC-2000,MAGIC+2000,MAGIC-3000,MAGIC-4000,MAGIC,None,MAGIC-1000,MAGIC-5000]
    deletes={2:MAGIC-1500,3:MAGIC+4000,8:MAGIC-2000}
    local = [r[0] for r in c.execute('SELECT service_id FROM services WHERE service_type=2')]
    for index,hid in enumerate(ids):
        for sid in [storage,media,trash,*local]:
            c.execute(f'DELETE FROM current_files_{sid} WHERE hash_id=?',(hid,))
            c.execute(f'DELETE FROM deleted_files_{sid} WHERE hash_id=?',(hid,))
        c.execute('DELETE FROM file_inbox WHERE hash_id=?',(hid,))
        c.execute('DELETE FROM archive_timestamps WHERE hash_id=?',(hid,))
        if index in deletes:
            for sid in [storage,media,local[0]]:
                c.execute(f'INSERT INTO deleted_files_{sid} VALUES(?,?,?)',(hid,deletes[index],imports[index]))
        else:
            for sid in [storage,trash if index==9 else media, *([] if index==9 else [local[0]])]:
                c.execute(f'INSERT INTO current_files_{sid} VALUES(?,?)',(hid,imports[index]))
        if index==4:c.execute('INSERT INTO file_inbox VALUES(?)',(hid,))
        if index==5:c.execute('INSERT INTO archive_timestamps VALUES(?,?)',(hid,MAGIC+123))
    c.commit();c.close()
    return dict(hashes=hashes,imports=imports,deletes={str(k):v for k,v in deletes.items()},inbox=[4],already_archived={'5':MAGIC+123},trash=[9],magic=MAGIC)

def record(session, path, both=False):
    from qtpy import QtWidgets as QW
    from hydrus.client.gui import ClientGUIDialogsQuick as Q
    from hydrus.core import HydrusExceptions
    c=session.controller
    manifest=json.load(open(os.path.join(HERE,'fixtures','legacy_db','basic.manifest.json')))
    hashes=[bytes.fromhex(r['hash'])for r in manifest['files'][:10]]
    def work():
        old_thread,old_after,old_yes,old_choices,old_read=c.CallToThread,c.CallAfterQtSafe,Q.GetYesNo,Q.GetYesYesNo,c.Read
        c.CallToThread=lambda fn,*a,**kw:fn(*a,**kw)
        c.CallAfterQtSafe=lambda widget,fn,*a:fn(*a)
        events=[]
        def state():
            c.db.ForceACommit()
            old_read('missing_archive_timestamps_legacy_count') # queue barrier after actual commit job
            conn=sqlite3.connect('file:'+os.path.join(path,'client.db')+'?mode=ro',uri=True)
            conn.execute('ATTACH DATABASE ? AS master',(os.path.join(path,'client.master.db'),))
            values=[conn.execute('SELECT archived_timestamp_ms FROM archive_timestamps JOIN master.hashes USING(hash_id) WHERE hash=?',(h,)).fetchone() for h in hashes]
            conn.close()
            return [row[0] if row else None for row in values]
        try:
            cases=[(True,['legacy','import'])] if both else [(True,'cancel-scan'),(False,None),(True,None),(True,['legacy']),(True,['import'])]
            for start,choice in cases:
                asked=[]
                def yes(parent,message,**kw):
                    asked.append(dict(message=message,**kw));return QW.QDialog.DialogCode.Accepted if start else QW.QDialog.DialogCode.Rejected
                def choose(parent,message,**kw):
                    asked.append(dict(message=message,**kw))
                    if choice is None:raise HydrusExceptions.CancelledException()
                    return choice
                Q.GetYesNo,Q.GetYesYesNo=yes,choose
                def read(action,*args,**kwargs):
                    result=old_read(action,*args,**kwargs)
                    if choice=='cancel-scan' and args and action=='missing_archive_timestamps_legacy_count':args[0].Cancel()
                    return result
                c.Read=read
                before=state()
                c.gui._FixMissingArchiveTimes()
                events.append(dict(start=start,choice=choice,asked=asked,before=before,after=state(),counts=[c.Read('missing_archive_timestamps_legacy_count'),c.Read('missing_archive_timestamps_import_count')]))
            return events
        finally:c.CallToThread,c.CallAfterQtSafe,Q.GetYesNo,Q.GetYesYesNo,c.Read=old_thread,old_after,old_yes,old_choices,old_read
    return c.CallBlockingToQt(c.gui,work)

def main():
    import hydrus_driver
    if len(sys.argv)>1 and sys.argv[1]=='--child':
        mode,output,path=sys.argv[2:5]
        if mode in ('record','both'):result=hydrus_driver.run_client(path,lambda session:record(session,path,mode=='both'))
        else:
            def reopened(session):
                manifest=json.load(open(os.path.join(HERE,'fixtures','legacy_db','basic.manifest.json')))
                hashes=[bytes.fromhex(r['hash'])for r in manifest['files'][:10]]
                return [m.GetTimesManager().GetArchivedTimestampMS()for m in session.controller.Read('media_results',hashes)]
            result=hydrus_driver.run_client(path,reopened)
        with open(output,'w')as f:json.dump(result,f)
        return
    import record_api
    with tempfile.TemporaryDirectory()as d:
        path=record_api.unpack_fixture('basic');corpus=seed(path)
        output=os.path.join(d,'result.json')
        hydrus_driver.run_in_subprocess(os.path.abspath(__file__),'--child','record',output,path)
        with open(output)as f:events=json.load(f)
        hydrus_driver.run_in_subprocess(os.path.abspath(__file__),'--child','reopen',output,path)
        with open(output)as f:reopened=json.load(f)
        seed(path)
        hydrus_driver.run_in_subprocess(os.path.abspath(__file__),'--child','both',output,path)
        with open(output)as f:both_events=json.load(f)
        result=dict(corpus=corpus,events=events,both_events=both_events,reopened_archived=reopened)
    with open(OUT,'w')as f:json.dump(result,f,indent=2);f.write('\n')
    print('wrote',OUT)
if __name__=='__main__':main()

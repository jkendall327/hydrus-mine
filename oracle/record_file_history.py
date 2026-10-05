#!/usr/bin/env python3
"""Actual global file-history chart, query, controls and cancel publication.

Only a freshly copied fixture is seeded before boot. Real Qt ReviewFileHistory
and its original work/publish callbacks are driven with a deferred scheduler;
no chart, search, timestamp or axis calculation is substituted.
"""
import json
import os
import sqlite3
import sys
import tempfile
HERE=os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0,HERE)
OUT=os.path.join(HERE,'fixtures','file_history.json')
T=1704067200000
DAY=86400000

def seed(path):
    manifest=json.load(open(os.path.join(HERE,'fixtures','legacy_db','basic.manifest.json')))
    hashes=[r['hash']for r in manifest['files'][:6]]
    c=sqlite3.connect(os.path.join(path,'client.db'))
    c.execute('ATTACH DATABASE ? AS master',(os.path.join(path,'client.master.db'),))
    services=dict(c.execute('SELECT service_type,service_id FROM services WHERE service_type IN(15,21,14,2)'))
    storage,media,trash,local=[services[t]for t in [15,21,14,2]]
    for sid in [storage,media,trash,local]:
        c.execute(f'DELETE FROM current_files_{sid}')
        c.execute(f'DELETE FROM deleted_files_{sid}')
    c.execute('DELETE FROM file_inbox');c.execute('DELETE FROM archive_timestamps')
    rows=[]
    for i,h in enumerate(hashes):
        hid=c.execute('SELECT hash_id FROM master.hashes WHERE hash=?',(bytes.fromhex(h),)).fetchone()[0]
        imported=T+[0,1,0,1,4,5][i]*DAY
        deleted=T+(3 if i==2 else 4)*DAY if i in [2,3]else None
        archived=T+[0,2,2,0,5,0][i]*DAY if i in [1,2,4]else None
        if i==2:
            c.execute(f'INSERT INTO deleted_files_{storage} VALUES(?,?,?)',(hid,deleted,imported))
        else:
            c.execute(f'INSERT INTO current_files_{storage} VALUES(?,?)',(hid,imported))
        if deleted:
            for sid in [media,local]:c.execute(f'INSERT INTO deleted_files_{sid} VALUES(?,?,?)',(hid,deleted,imported))
            if i==3:c.execute(f'INSERT INTO current_files_{trash} VALUES(?,?)',(hid,imported))
        else:
            for sid in [media,local]:c.execute(f'INSERT INTO current_files_{sid} VALUES(?,?)',(hid,imported))
        if archived:c.execute('INSERT INTO archive_timestamps VALUES(?,?)',(hid,archived))
        inbox=i in [0,3,5]
        if inbox:c.execute('INSERT INTO file_inbox VALUES(?)',(hid,))
        rows.append(dict(hash=h,imported=imported,deleted=deleted,archived=archived,trash=i==3,inbox=inbox))
    c.commit();c.close()
    return rows

def record(session):
    from qtpy import QtCore as QC, QtWidgets as QW
    from hydrus.client.gui import ClientGUIAsync
    from hydrus.client.gui.panels import ClientGUIScrolledPanelsReview as R
    from hydrus.client.gui import ClientGUITopLevelWindowsPanels as F
    from hydrus.client.search import ClientSearchPredicate as P
    from hydrus.client import ClientConstants as CC
    from hydrus.client.metadata import ClientContentUpdates as U
    from hydrus.core import HydrusConstants as HC
    c=session.controller
    manifest=json.load(open(os.path.join(HERE,'fixtures','legacy_db','basic.manifest.json')))
    hashes=[bytes.fromhex(r['hash'])for r in manifest['files'][:6]]
    c.WriteSynchronous('content_updates',U.ContentUpdatePackage.STATICCreateFromContentUpdate(CC.DEFAULT_LOCAL_TAG_SERVICE_KEY,U.ContentUpdate(HC.CONTENT_TYPE_MAPPINGS,HC.CONTENT_UPDATE_ADD,('parity:history-selected',[hashes[i]for i in [0,2,4]]))))
    session.sync_tag_display()
    def work():
        jobs=[]
        class DeferredJob:
            def __init__(self,owner,callback,publish):self.callback,self.publish=callback,publish
            def start(self):jobs.append(self)
        old_job=ClientGUIAsync.AsyncQtJob
        ClientGUIAsync.AsyncQtJob=DeferredJob
        frame=None
        try:
            frame=F.FrameThatTakesScrollablePanel(c.gui,'file history',frame_key='file_history_chart')
            panel=R.ReviewFileHistory(frame)
            frame.SetPanel(panel)
            def flush():
                while jobs:
                    job=jobs.pop(0);job.publish(job.callback())
            def state(case):
                chart=panel._file_history_chart
                return dict(case=case,history={k:dict(length=len(v),first=v[0] if v else None,last=v[-1] if v else None,min_count=min((p[1]for p in v),default=0),max_count=max((p[1]for p in v),default=0))for k,v in chart._file_history.items()},visible=[chart._show_current,chart._show_inbox,chart._show_archive,chart._show_deleted],x=[chart._x_datetime_axis.min().toSecsSinceEpoch(),chart._x_datetime_axis.max().toSecsSinceEpoch()],y=[chart.GetMinY(),chart.GetMaxY()],dates=[panel._start_date.date().toString('yyyy-MM-dd'),panel._end_date.date().toString('yyyy-MM-dd')],custom=[panel._user_set_custom_x,panel._user_set_custom_y],status=panel._status_st.text(),status_visible=panel._status_st.isVisible(),chart_visible=panel._file_history_chart_panel.isVisible(),cancel=panel._cancel_button.isEnabled(),refresh=panel._refresh_button.isEnabled())
            events=[state('loading')]
            flush();events.append(state('initial'))
            context=panel._tag_autocomplete.GetFileSearchContext()
            compact=c.Read('file_history',8,file_search_context=context)
            frame.resize(1300,850);frame.grab().save(OUT.replace('.json','.png'))
            panel._show_current.click();events.append(state('hide_all'))
            panel._min_y.setValue(2);panel._max_y.setValue(12)
            panel._start_date.setDate(QC.QDate(2024,1,2));panel._end_date.setDate(QC.QDate(2024,1,5))
            events.append(state('custom_ranges'))
            panel._show_inbox.click();events.append(state('hide_inbox_keeps_custom_y'))
            panel._RefreshSearch();flush();events.append(state('refresh_keeps_custom_ranges'))
            panel._auto_x_range.click();panel._auto_y_range.click();events.append(state('refit'))
            panel._RefreshSearch();panel._CancelCurrentSearch();flush();events.append(state('cancelled'))
            context=context.Duplicate();context.SetPredicates([P.Predicate(P.PREDICATE_TYPE_TAG,'parity:history-selected')])
            panel._tag_autocomplete.SetFileSearchContext(context);panel._RefreshSearch();flush();events.append(state('filtered'))
            filtered=c.Read('file_history',8,file_search_context=context)
            selected_current=c.Read('file_query_ids',file_search_context=context,apply_implicit_limit=False)
            deleted_context=context.Duplicate();deleted_context.SetLocationContext(context.GetLocationContext().GetDeletedInverse())
            selected_deleted=c.Read('file_query_ids',file_search_context=deleted_context,apply_implicit_limit=False)
            selected_hashes=lambda ids:sorted(h.hex()for h in c.Read('hash_ids_to_hashes',ids).values())
            old_limit=c.new_options.GetNoneableInteger('forced_search_limit')
            try:
                c.new_options.SetNoneableInteger('forced_search_limit',1)
                limited=c.Read('file_query_ids',file_search_context=context)
                uncapped=c.Read('file_history',8,file_search_context=context)
                implicit_limit_case=dict(limit=1,ordinary_count=len(limited),history=uncapped)
            finally:c.new_options.SetNoneableInteger('forced_search_limit',old_limit)
            widgets=[dict(type=w.metaObject().className(),text=w.text())for w in panel.findChildren(QW.QAbstractButton)]
            return dict(events=events,compact=compact,filtered_compact=filtered,buttons=widgets,context=context.GetSerialisableTuple(),filter_tag='parity:history-selected',requested_filter_hash_indices=[0,2,4],selected_current=selected_hashes(selected_current),selected_deleted=selected_hashes(selected_deleted),num_steps=7680,implicit_limit_case=implicit_limit_case)
        finally:
            ClientGUIAsync.AsyncQtJob=old_job
            if frame is not None:frame.close()
    return c.CallBlockingToQt(c.gui,work)

def main():
    import hydrus_driver
    if len(sys.argv)>1 and sys.argv[1]=='--child':
        import record_api
        output=sys.argv[2];path=record_api.unpack_fixture('basic');corpus=seed(path)
        result=hydrus_driver.run_client(path,record);result['corpus']=corpus
        with open(output,'w')as f:json.dump(result,f)
        return
    with tempfile.TemporaryDirectory()as d:
        output=os.path.join(d,'result.json');hydrus_driver.run_in_subprocess(os.path.abspath(__file__),'--child',output)
        with open(output)as f:result=json.load(f)
    with open(OUT,'w')as f:json.dump(result,f,indent=2);f.write('\n')
    print('wrote',OUT)
if __name__=='__main__':main()

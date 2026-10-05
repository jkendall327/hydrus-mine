#!/usr/bin/env python3
"""Record real FilesAndTrash options, thumbnail archive/inbox questions and
EditDeleteFilesPanel auto-resolution on authored selections from the basic fixture.
Writes are intercepted; panel UpdateOptions and native action construction run.
"""
import json
import os
import sys
import tempfile
import time
HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)


def record(session):
    c = session.controller
    def build():
        from hydrus.client import ClientConstants as CC, ClientLocation
        results = c.Read('media_results_from_ids', list(range(1, 41)))
        hashes = [m.GetHash() for m in results]
        page = c.gui._notebook.NewPageQuery(ClientLocation.LocationContext.STATICCreateSimple(CC.LOCAL_FILE_SERVICE_KEY), page_name='synthetic confirmation examples', initial_hashes=hashes)
        return page, results
    page, results = c.CallBlockingToQt(c.gui, build)
    for _ in range(400):
        if c.CallBlockingToQt(c.gui, lambda: page._initialised): break
        time.sleep(.025)
    else: raise RuntimeError('real thumbnail owner did not initialize')
    def drive():
        from qtpy import QtWidgets as QW
        from hydrus.core import HydrusConstants as HC
        from hydrus.client import ClientConstants as CC
        from hydrus.client.gui import ClientGUIDialogsQuick as Q
        from hydrus.client.gui.panels.options import FilesAndTrashPanel as O
        from hydrus.client.gui.panels import ClientGUIScrolledPanelsEdit as E
        from hydrus.client.media import ClientMediaSingle as M
        old = {k: HC.options[k] for k in ('confirm_archive', 'confirm_trash')}
        old_write, old_yes, old_text = c.Write, Q.GetYesNo, Q.EnterText
        keys = ('use_advanced_file_deletion_dialog','remember_last_advanced_file_deletion_special_action','remember_last_advanced_file_deletion_reason')
        old_flags = {k:c.new_options.GetBoolean(k) for k in keys}
        old_reasons = c.new_options.GetStringList('advanced_file_deletion_reasons')
        old_last = {k:c.new_options.GetNoneableString(k) for k in ('last_advanced_file_deletion_special_action','last_advanced_file_deletion_reason')}
        owner = page.GetMediaResultsPanel()
        out = {'initial': old.copy(), 'options': [], 'archive': [], 'deletion': []}
        panels = []
        questions, writes = [], []
        accepted = [False]
        def answer(parent, message, **kwargs):
            questions.append(message)
            return QW.QDialog.DialogCode.Accepted if accepted[0] else QW.QDialog.DialogCode.Rejected
        def write(name, *args, **kwargs):
            writes.append(name)
        c.Write, Q.GetYesNo = write, answer
        try:
            for value in (False, True):
                panel = O.FilesAndTrashPanel(c.gui); panels.append(panel)
                panel._confirm_archive.setChecked(value); panel._confirm_trash.setChecked(value)
                out['options'].append({'entered': value, 'staged': [HC.options['confirm_archive'],HC.options['confirm_trash']]})
                panel.UpdateOptions()
                reopened = O.FilesAndTrashPanel(c.gui); panels.append(reopened)
                out['options'][-1]['reopened'] = [reopened._confirm_archive.isChecked(), reopened._confirm_trash.isChecked()]
                for action, inbox in (('archive', True), ('inbox', False)):
                    choices = [m for m in owner.GetSortedMedia() if m.HasInbox() == inbox][:2]
                    assert len(choices) == 2
                    for count in (1, 2):
                        for yes in (False, True):
                            owner._HitMedia(None, False, False)
                            for i, media in enumerate(choices[:count]): owner._HitMedia(media, i > 0, False)
                            accepted[0] = yes; questions.clear(); writes.clear()
                            (owner._Archive if inbox else owner._Inbox)()
                            out['archive'].append({'confirm': value, 'action': action, 'count': count, 'accepted': yes, 'questions': list(questions), 'writes': list(writes)})
                by_domains = {}
                for result in results:
                    domains = [key for key in result.GetLocationsManager().GetCurrent() if c.services_manager.GetServiceType(key) == HC.LOCAL_FILE_DOMAIN]
                    if domains: by_domains.setdefault(len(domains), result)
                for domains, result in sorted(by_domains.items()):
                    deletion = E.EditDeleteFilesPanel(c.gui, [M.MediaSingle(result)], 'Deleted from Preview or Media Viewer.')
                    panels.append(deletion)
                    out['deletion'].append({'confirm': value, 'domains': domains, 'resolved': deletion.QuestionIsAlreadyResolved(), 'description': deletion._simple_description.text()})
            from hydrus.core import HydrusExceptions
            advanced = O.FilesAndTrashPanel(c.gui); panels.append(advanced)
            out['advanced_controls'] = []
            for enabled in (False, True):
                if advanced._use_advanced_file_deletion_dialog.isChecked() != enabled: advanced._use_advanced_file_deletion_dialog.click()
                out['advanced_controls'].append({'advanced':enabled,'enabled':[advanced._remember_last_advanced_file_deletion_special_action.isEnabled(),advanced._remember_last_advanced_file_deletion_reason.isEnabled(),advanced._advanced_file_deletion_reasons.isEnabled()]})
            queue = advanced._advanced_file_deletion_reasons
            queue.SetData(['alpha','beta','beta',''])
            events=[]
            next_text=[None]
            def enter(parent, message, default='', **kwargs):
                events.append({'message':message,'default':default,'answer':next_text[0]})
                if next_text[0] is None: raise HydrusExceptions.CancelledException()
                return next_text[0]
            Q.EnterText=enter
            for text in (None,'synthetic reason 日本',''):
                next_text[0]=text; queue._Add(); events[-1]['rows']=queue.GetData()
            queue._listbox.item(1).setSelected(True)
            next_text[0]='edited beta'; queue._Edit(); events[-1]['rows']=queue.GetData()
            queue._Up(); moved=queue.GetData()
            queue._Down(); restored=queue.GetData()
            questions.clear();accepted[0]=False;queue._Delete();cancel_rows=queue.GetData()
            accepted[0]=True;queue._Delete()
            out['reason_queue']={'events':events,'up':moved,'down':restored,'delete_question':list(questions),'cancel_rows':cancel_rows,'deleted_rows':queue.GetData(),'staged':c.new_options.GetStringList('advanced_file_deletion_reasons')}
            advanced.UpdateOptions()
            reopened=O.FilesAndTrashPanel(c.gui);panels.append(reopened)
            out['reason_queue']['reopened']=reopened._advanced_file_deletion_reasons.GetData()
            for key in keys: c.new_options.SetBoolean(key,True)
            c.new_options.SetStringList('advanced_file_deletion_reasons',['alpha','beta','beta',''])
            single=by_domains[1]
            double=by_domains[2]
            out['advanced']=[]
            def snapshot(dialog):
                return {'actions':[b.text() for b in dialog._action_radio._radio_buttons], 'action':next(i for i,b in enumerate(dialog._action_radio._radio_buttons) if b.isChecked()), 'reasons':[b.text() for b in dialog._reason_radio._radio_buttons], 'reason':next(i for i,b in enumerate(dialog._reason_radio._radio_buttons) if b.isChecked()),'custom':dialog._custom_reason.text(),'reason_enabled':dialog._reason_panel.isEnabled(),'custom_enabled':dialog._custom_reason.isEnabled()}
            for existing,last,remember in [(None,None,True),(None,'beta',True),(None,'outside list',True),(None,'outside list',False),('alpha','beta',True),('outside existing','beta',True)]:
                result=single.Duplicate();result.GetLocationsManager()._local_file_deletion_reason=existing
                c.new_options.SetBoolean('remember_last_advanced_file_deletion_reason',remember)
                c.new_options.SetNoneableString('last_advanced_file_deletion_reason',last)
                dialog=E.EditDeleteFilesPanel(c.gui,[M.MediaSingle(result)],'Deleted from Preview or Media Viewer.');panels.append(dialog)
                dialog.show();QW.QApplication.processEvents()
                item={'existing':existing,'last':last,'remember':remember,'hash':result.GetHash().hex(),'initial':snapshot(dialog)}
                custom=next(i for i,b in enumerate(dialog._reason_radio._radio_buttons) if b.text()=='custom')
                dialog._reason_radio.Select(custom);dialog._custom_reason.setText('synthetic custom 日本');dialog._UpdateControls()
                dialog.GetValue()
                item['accepted']=snapshot(dialog);item['saved_reason']=c.new_options.GetNoneableString('last_advanced_file_deletion_reason');item['saved_action']=c.new_options.GetNoneableString('last_advanced_file_deletion_special_action')
                dialog.hide();out['advanced'].append(item)
            dialog=E.EditDeleteFilesPanel(c.gui,[M.MediaSingle(single),M.MediaSingle(double)],'Deleted from Preview or Media Viewer.');panels.append(dialog)
            dialog.show();QW.QApplication.processEvents()
            out['action_cases']=[]
            for index,button in enumerate(dialog._action_radio._radio_buttons):
                dialog._action_radio.Select(index);dialog._UpdateControls();dialog.GetValue()
                out['action_cases'].append({'selected':button.text(),'state':snapshot(dialog),'saved_action':c.new_options.GetNoneableString('last_advanced_file_deletion_special_action')})
            mixed=[]
            for result,reason in ((single,'alpha'),(double,'different existing')):
                result=result.Duplicate();result.GetLocationsManager()._local_file_deletion_reason=reason;mixed.append(M.MediaSingle(result))
            mixed_dialog=E.EditDeleteFilesPanel(c.gui,mixed,'Deleted from Preview or Media Viewer.');panels.append(mixed_dialog)
            mixed_dialog.show();QW.QApplication.processEvents();out['mixed_existing']=snapshot(mixed_dialog)
            out['physical_remember']=[]
            for previous in (None,CC.LOCAL_FILE_SERVICE_KEY.hex(),single.GetLocationsManager().GetCurrent().intersection({service.GetServiceKey() for service in c.services_manager.GetServices((HC.LOCAL_FILE_DOMAIN,))}).pop().hex(),'clear_delete'):
                result=single.Duplicate();result.GetLocationsManager()._current={CC.TRASH_SERVICE_KEY,CC.HYDRUS_LOCAL_FILE_STORAGE_SERVICE_KEY}
                c.new_options.SetNoneableString('last_advanced_file_deletion_special_action',previous)
                physical=E.EditDeleteFilesPanel(c.gui,[M.MediaSingle(result)],'Deleted from Preview or Media Viewer.');panels.append(physical)
                physical._action_radio.Select(0);physical._UpdateControls();physical.GetValue()
                out['physical_remember'].append({'before':previous,'after':c.new_options.GetNoneableString('last_advanced_file_deletion_special_action')})
            mixed_dialog.hide()
            dialog.grab().save(os.path.join(HERE,'fixtures/files_trash_delete.png'));dialog.hide()
            panel.resize(1150,850); panel.show(); QW.QApplication.processEvents()
            panel.grab().save(os.path.join(HERE,'fixtures/files_trash_options.png'))
            return out
        finally:
            HC.options.update(old); c.Write, Q.GetYesNo, Q.EnterText = old_write, old_yes, old_text
            for key,value in old_flags.items(): c.new_options.SetBoolean(key,value)
            c.new_options.SetStringList('advanced_file_deletion_reasons',old_reasons)
            for key,value in old_last.items():c.new_options.SetNoneableString(key,value)
            for panel in panels: panel.deleteLater()
    return c.CallBlockingToQt(c.gui, drive)


def main():
    import hydrus_driver
    if len(sys.argv) > 1 and sys.argv[1] == '--child':
        output = sys.argv[2]
        import record_api
        result = hydrus_driver.run_client(record_api.unpack_fixture('basic'), record)
        with open(output,'w') as f: json.dump(result,f,ensure_ascii=False,indent=2)
        return
    with tempfile.TemporaryDirectory() as work:
        output = os.path.join(work,'result.json')
        hydrus_driver.run_in_subprocess(os.path.abspath(__file__),'--child',output)
        with open(output) as f: result=json.load(f)
        with open(os.path.join(HERE,'fixtures/files_trash.json'),'w') as f: json.dump(result,f,ensure_ascii=False,indent=2)

if __name__ == '__main__': main()

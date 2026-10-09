#!/usr/bin/env python3
"""Record the reference's undelete questions (ClientGUIMediaModalActions.UndeleteMedia).

On the basic fixture, files are really deleted from one or both of their local
file domains, then undeleted with confirm_trash off and on, the yes/no question
answered both ways and the 'Undelete for?' chooser answered with each choice
or cancelled. The undelete writes are intercepted and recorded as the service
name they target, so the fixture database keeps the deletions.
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
    from hydrus.core import HydrusConstants as HC
    from hydrus.client import ClientConstants as CC
    from hydrus.client.metadata import ClientContentUpdates as U

    local = {s.GetServiceKey(): s.GetName() for s in c.services_manager.GetServices((HC.LOCAL_FILE_DOMAIN,))}
    results = c.Read('media_results_from_ids', list(range(1, 41)))
    by_count = {}
    for result in results:
        domains = sorted(k for k in result.GetLocationsManager().GetCurrent() if k in local)
        by_count.setdefault(len(domains), []).append((result.GetHash(), domains))
    one = by_count[1][0]
    two = by_count[2][0]
    two_partial = by_count[2][1]

    def delete(hash, keys):
        for key in keys:
            update = U.ContentUpdate(HC.CONTENT_TYPE_FILES, HC.CONTENT_UPDATE_DELETE, {hash}, reason='synthetic')
            c.WriteSynchronous('content_updates', U.ContentUpdatePackage.STATICCreateFromContentUpdate(key, update))

    # one domain (to the trash), both domains (to the trash), one of two (stays in the other)
    delete(one[0], one[1])
    delete(two[0], two[1])
    delete(two_partial[0], two_partial[1][:1])
    time.sleep(0.5)
    cases = [
        ('deleted from its one domain', [one[0]]),
        ('deleted from both its domains', [two[0]]),
        ('deleted from one of its two domains', [two_partial[0]]),
        ('two files, each deleted from one domain', [one[0], two_partial[0]]),
    ]

    def drive():
        from qtpy import QtWidgets as QW
        from hydrus.client.gui import ClientGUIDialogsQuick as Q
        from hydrus.client.gui.media import ClientGUIMediaModalActions as A
        from hydrus.client.media import ClientMediaSingle as M
        from hydrus.core import HydrusExceptions
        old_confirm = HC.options['confirm_trash']
        old_write, old_yes, old_select = c.Write, Q.GetYesNo, Q.SelectFromListButtons
        names = dict(local)
        names[CC.COMBINED_LOCAL_FILE_DOMAINS_SERVICE_KEY] = 'all my files'
        log = []
        answer = {'yes': False, 'choice': None}

        def yes_no(parent, message, **kwargs):
            log.append({'question': message})
            return QW.QDialog.DialogCode.Accepted if answer['yes'] else QW.QDialog.DialogCode.Rejected

        def select(parent, title, choice_tuples, message='', **kwargs):
            log.append({'chooser': title, 'message': message, 'choices': [[t, tip] for (t, _, tip) in choice_tuples]})
            if answer['choice'] is None:
                raise HydrusExceptions.CancelledException()
            return choice_tuples[answer['choice']][1]

        def write(name, package, *args, **kwargs):
            for (key, updates) in package.IterateContentUpdates():
                for update in updates:
                    log.append({'undelete': names.get(key, key.hex()), 'files': sorted(h.hex() for h in update.GetHashes())})

        c.Write, Q.GetYesNo, Q.SelectFromListButtons = write, yes_no, select
        out = []
        try:
            for (name, hashes) in cases:
                medias = [M.MediaSingle(r) for r in c.Read('media_results', hashes)]
                deleted = sorted({local[k] for m in medias for k in m.GetLocationsManager().GetDeleted() if k in local})
                current = sorted({local[k] for m in medias for k in m.GetLocationsManager().GetCurrent() if k in local})
                case = {'case': name, 'files': [h.hex() for h in hashes], 'deleted_from': deleted, 'current_in': current, 'runs': []}
                for confirm in (False, True):
                    HC.options['confirm_trash'] = confirm
                    for (yes, choice) in ((False, None), (True, None), (True, 0), (True, 1), (True, 2)):
                        answer['yes'], answer['choice'] = yes, choice
                        log.clear()
                        A.UndeleteMedia(c.gui, medias)
                        case['runs'].append({'confirm': confirm, 'yes': yes, 'choice': choice, 'log': list(log)})
                out.append(case)
            return out
        finally:
            HC.options['confirm_trash'] = old_confirm
            c.Write, Q.GetYesNo, Q.SelectFromListButtons = old_write, old_yes, old_select

    return c.CallBlockingToQt(c.gui, drive)


def main():
    import hydrus_driver
    if len(sys.argv) > 1 and sys.argv[1] == '--child':
        output = sys.argv[2]
        import record_api
        result = hydrus_driver.run_client(record_api.unpack_fixture('basic'), record)
        with open(output, 'w') as f:
            json.dump(result, f, ensure_ascii=False, indent=2)
        return
    with tempfile.TemporaryDirectory() as work:
        output = os.path.join(work, 'result.json')
        hydrus_driver.run_in_subprocess(os.path.abspath(__file__), '--child', output)
        with open(output) as f:
            result = json.load(f)
        with open(os.path.join(HERE, 'fixtures/undelete_question.json'), 'w') as f:
            json.dump(result, f, ensure_ascii=False, indent=2)
            f.write('\n')


if __name__ == '__main__':
    main()

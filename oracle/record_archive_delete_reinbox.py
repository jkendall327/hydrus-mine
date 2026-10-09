#!/usr/bin/env python3
"""Record the archive/delete filter's commit under the delete lock options.

For each setting of "Do not permit archived files to be deleted from the
trash" and "After archive/delete filter, ensure deletees are inboxed before
delete" (a fresh boot of `basic` each, as the commit moves files), twelve of
the fixture's files are set archived or inboxed, two are kept and twelve
deleted by the real `ClientGUICanvas.CommitArchiveDelete` (what the filter's
finish runs, deleting from "my files"; twelve crosses its ten-file blocks).
The record says, for each file, whether it is in the inbox, in "my files" and
in the trash after the commit; then the real `DAEMONMaintainTrash` runs with
a size limit of 0 MB (empty the trash) and the record says which files are
still stored, i.e. which the lock kept.
"""
import json, os, sys, tempfile, types
HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)

NAMES = ['jpeg_00.jpg', 'jpeg_01.jpg', 'jpeg_02.jpg', 'jpeg_03.jpg', 'jpeg_04.jpg', 'jpeg_05.jpg',
         'jpeg_06.jpg', 'jpeg_07.jpg', 'jpeg_08.jpg', 'jpeg_09.jpg', 'jpeg_10.jpg', 'jpeg_11.jpg',
         'png_00.png', 'png_01.png']
# kept: the last two; archived before the filter: every other file
KEPT = NAMES[12:]
ARCHIVED = NAMES[0::2]
SETTINGS = [(False, False), (False, True), (True, False), (True, True)]


def record(session, lock, reinbox):
    from hydrus.core import HydrusConstants as HC
    from hydrus.client import ClientConstants as CC, ClientLocation, ClientDaemons
    from hydrus.client.gui.canvas import ClientGUICanvas
    from hydrus.client.metadata import ClientContentUpdates
    c = session.controller
    manifest = json.load(open(os.path.join(HERE, 'fixtures', 'legacy_db', 'basic.manifest.json')))
    hashes = {f['name']: bytes.fromhex(f['hash']) for f in manifest['files']}

    def write(service_key, action, hs):
        cu = ClientContentUpdates.ContentUpdate(HC.CONTENT_TYPE_FILES, action, hs)
        c.WriteSynchronous('content_updates', ClientContentUpdates.ContentUpdatePackage.STATICCreateFromContentUpdate(service_key, cu))

    # empty the trash the fixture came with, then the starting states
    trashed = c.Read('trash_hashes')
    if trashed:
        write(CC.HYDRUS_LOCAL_FILE_STORAGE_SERVICE_KEY, HC.CONTENT_UPDATE_DELETE, trashed)
    write(CC.HYDRUS_LOCAL_FILE_STORAGE_SERVICE_KEY, HC.CONTENT_UPDATE_INBOX, [hashes[n] for n in NAMES])
    write(CC.HYDRUS_LOCAL_FILE_STORAGE_SERVICE_KEY, HC.CONTENT_UPDATE_ARCHIVE, [hashes[n] for n in ARCHIVED])
    c.new_options.SetBoolean('delete_lock_for_archived_files', lock)
    c.new_options.SetBoolean('delete_lock_reinbox_deletees_after_archive_delete', reinbox)
    results = {n: c.Read('media_result', hashes[n]) for n in NAMES}
    kept = [results[n] for n in KEPT]
    deleted = [results[n] for n in NAMES if n not in KEPT]
    ClientGUICanvas.CommitArchiveDelete(ClientLocation.LocationContext.STATICCreateSimple(CC.LOCAL_FILE_SERVICE_KEY), kept, deleted)

    def states():
        out = {}
        for n in NAMES:
            lm = c.Read('media_result', hashes[n]).GetLocationsManager()
            current = lm.GetCurrent()
            out[n] = {'inbox': lm.inbox, 'my_files': CC.LOCAL_FILE_SERVICE_KEY in current,
                      'trash': CC.TRASH_SERVICE_KEY in current,
                      'stored': CC.HYDRUS_LOCAL_FILE_STORAGE_SERVICE_KEY in current}
        return out

    after_commit = states()
    HC.options['trash_max_age'] = None
    HC.options['trash_max_size'] = 0
    real_time = ClientDaemons.time

    class NoRest:
        def __getattr__(self, name):
            return getattr(real_time, name)

        def sleep(self, seconds):
            pass

    ClientDaemons.time = NoRest()
    ClientDaemons.DAEMONMaintainTrash()
    ClientDaemons.time = real_time
    return {'lock': lock, 'reinbox': reinbox, 'after_commit': after_commit, 'after_emptying_trash': states()}


def main():
    import hydrus_driver
    if len(sys.argv) > 1 and sys.argv[1] == '--child':
        index, output = int(sys.argv[2]), sys.argv[3]
        import record_api
        lock, reinbox = SETTINGS[index]
        result = hydrus_driver.run_client(record_api.unpack_fixture('basic'), lambda s: record(s, lock, reinbox))
        with open(output, 'w') as f:
            json.dump(result, f)
        return
    out = {'files': NAMES, 'kept': KEPT, 'archived_before': ARCHIVED, 'deleted_from': 'my files', 'settings': []}
    with tempfile.TemporaryDirectory() as tmp:
        for i in range(len(SETTINGS)):
            path = os.path.join(tmp, f'{i}.json')
            hydrus_driver.run_in_subprocess(os.path.abspath(__file__), '--child', str(i), path)
            with open(path) as f:
                out['settings'].append(json.load(f))
    with open(os.path.join(HERE, 'fixtures', 'archive_delete_reinbox.json'), 'w') as f:
        json.dump(out, f, indent=1)
        f.write('\n')
    for s in out['settings']:
        print(s['lock'], s['reinbox'], {n: (v['inbox'], v['trash'], v['stored']) for n, v in s['after_emptying_trash'].items()})


if __name__ == '__main__':
    main()

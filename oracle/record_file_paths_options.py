#!/usr/bin/env python3
"""Record two "files and trash" options on the reference code that reads them.

- ADVANCED: Do not do chmod when copying files: files of several permission
  modes are imported through the Client API (the reference copies them to a
  temporary file and then into its file storage), with the option off and
  on (set as Options does, then `ReinitGlobalSettings`); the mode each
  stored file ends with is recorded, with the process's umask.
- When physically deleting files or folders, send them to the OS's recycle
  bin: a file is trashed, deleted for good, and the real deferred physical
  delete (`ClientFilesManager.DoDeferredPhysicalDeletes`) run, with the
  option off and on; the record says whether the file and its thumbnail are
  still where they were, and whether they are in the freedesktop.org trash
  (`$XDG_DATA_HOME/Trash`, pointed into the recording's own directory).

The files are generated BMPs (`record_trash_maintenance.bmp`), seeds 100 up.
"""
import json, os, stat, sys, tempfile, threading
HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)

PORT = 45894
MODES = [0o400, 0o600, 0o604, 0o640, 0o644, 0o664, 0o700, 0o755]


def record(session, db_dir, xdg):
    from record_trash_maintenance import bmp
    from hydrus.core import HydrusConstants as HC, HydrusPaths
    from hydrus.client import ClientConstants as CC
    from hydrus.client.metadata import ClientContentUpdates
    c = session.controller
    umask = os.umask(0)
    os.umask(umask)
    out = {'umask': umask, 'chmod': [], 'recycle': []}
    seed = 100
    for off in (False, True):
        c.new_options.SetBoolean('do_not_do_chmod_mode', off)
        c.ReinitGlobalSettings()
        assert HydrusPaths.DO_NOT_DO_CHMOD_MODE == off
        for mode in MODES:
            seed += 1
            path = os.path.join(db_dir, f'chmod_{seed}.bmp')
            with open(path, 'wb') as f:
                f.write(bmp(seed, 20000))
            os.chmod(path, mode)
            h = session.api.post('/add_files/add_file', {'path': path})['hash']
            stored = c.client_files_manager.GetFilePath(bytes.fromhex(h), HC.IMAGE_BMP)
            out['chmod'].append({'do_not_chmod': off, 'seed': seed, 'source_mode': mode,
                                 'stored_mode': stat.S_IMODE(os.stat(stored).st_mode)})
    c.new_options.SetBoolean('do_not_do_chmod_mode', False)
    c.ReinitGlobalSettings()

    def write(service_key, hashes):
        cu = ClientContentUpdates.ContentUpdate(HC.CONTENT_TYPE_FILES, HC.CONTENT_UPDATE_DELETE, hashes)
        c.WriteSynchronous('content_updates', ClientContentUpdates.ContentUpdatePackage.STATICCreateFromContentUpdate(service_key, cu))

    c.new_options.SetBoolean('deferred_file_deletes_in_normal_time', True)
    c.new_options.SetInteger('ms_to_wait_between_physical_file_deletes', 0)
    trash = os.path.join(xdg, 'Trash')
    for on in (False, True):
        HC.options['delete_to_recycle_bin'] = on
        seed += 1
        path = os.path.join(db_dir, f'recycle_{seed}.bmp')
        with open(path, 'wb') as f:
            f.write(bmp(seed, 20000))
        h = bytes.fromhex(session.api.post('/add_files/add_file', {'path': path})['hash'])
        file_path = c.client_files_manager.GetFilePath(h, HC.IMAGE_BMP)
        thumb_path = c.client_files_manager.GetThumbnailPath(c.Read('media_result', h))
        assert os.path.exists(file_path) and os.path.exists(thumb_path)
        write(CC.COMBINED_LOCAL_FILE_DOMAINS_SERVICE_KEY, [h])
        write(CC.HYDRUS_LOCAL_FILE_STORAGE_SERVICE_KEY, [h])
        c.client_files_manager.DoDeferredPhysicalDeletes()

        def in_trash(p):
            name = os.path.basename(p)
            info = os.path.join(trash, 'info', name + '.trashinfo')
            entry = {'file': os.path.exists(os.path.join(trash, 'files', name))}
            if os.path.exists(info):
                with open(info) as f:
                    lines = [l for l in f.read().splitlines() if l.startswith('Path=')]
                entry['info_path_is_original'] = lines == ['Path=' + p]
            return entry

        out['recycle'].append({'recycle': on, 'seed': seed,
                               'file_still_there': os.path.exists(file_path),
                               'file_in_trash': in_trash(file_path),
                               'thumbnail_still_there': os.path.exists(thumb_path),
                               'thumbnail_in_trash': in_trash(thumb_path)})
    return out


def main():
    import hydrus_driver
    if len(sys.argv) > 1 and sys.argv[1] == '--child':
        output, xdg = sys.argv[2], sys.argv[3]
        os.environ['XDG_DATA_HOME'] = xdg
        import record_api
        db_dir = record_api.unpack_fixture('basic')
        result = hydrus_driver.run_client(db_dir, lambda s: record(s, db_dir, xdg), port=PORT)
        with open(output, 'w') as f:
            json.dump(result, f)
        return
    with tempfile.TemporaryDirectory() as tmp:
        path = os.path.join(tmp, 'result.json')
        xdg = os.path.join(tempfile.gettempdir(), 'hydrus_record_xdg')
        os.makedirs(xdg, exist_ok=True)
        hydrus_driver.run_in_subprocess(os.path.abspath(__file__), '--child', path, xdg)
        with open(path) as f:
            result = json.load(f)
    with open(os.path.join(HERE, 'fixtures', 'file_paths_options.json'), 'w') as f:
        json.dump(result, f, indent=1)
        f.write('\n')
    print(json.dumps(result))


if __name__ == '__main__':
    main()

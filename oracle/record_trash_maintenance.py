#!/usr/bin/env python3
"""Record which trashed files the reference's trash maintenance deletes for good.

Each scenario boots the `basic` fixture afresh (deleting is for good, so
scenarios cannot share a database), empties its trash, imports a few
generated BMPs of exact sizes (`bmp()`, written the same way by the native
test) through the Client API, sends files to the trash at scripted times
(time held still), sets the "files and trash" options' maximum age and size,
and runs the real `ClientDaemons.DAEMONMaintainTrash` at a scripted "now"
(its two-second rests skipped). It records each trashed file's size and
trash time, the groups of files it deleted for good in order (and whether
for size or for age), and what is left in the trash.

Scenarios hold the boundaries the port could get wrong: the age cutoff
whole-second rounding and its strict "older than", the size limit's strict
"bigger than" at exactly a megabyte, oldest-first order, groups of eight with
the size checked between them, size before age, and no limits at all.
"""
import json, os, struct, sys, tempfile, threading, types
HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)

PORT = 45893
MB = 1048576
HOUR = 3600
# a fixed "now", half a second into a second: 2026-01-01T00:00:00.500Z
NOW_MS = 1767225600500


def bmp(seed, size):
    """A 64x64 24-bit BMP of exactly `size` bytes: the pixel data starts as
    late as the size needs (the gap is zeros), so a file can be any size
    from 12,342 bytes up. Pixels follow `seed`, so files differ."""
    width = height = 64
    row = width * 3
    pixels = bytes(((seed * 31 + i * 7) ^ (i >> 8)) & 0xFF for i in range(row * height))
    offset = size - len(pixels)
    assert offset >= 54, size
    header = b'BM' + struct.pack('<IHHI', size, 0, 0, offset)
    dib = struct.pack('<IiiHHIIiiII', 40, width, height, 1, 24, 0, len(pixels), 2835, 2835, 0, 0)
    return header + dib + bytes(offset - 54) + pixels


# Each scenario: the limits, then the files trashed: (name, size or None for
# a `basic` file, ms before NOW_MS it was trashed). Generated files are named
# 'bmp<seed>'; `basic` files by their fixture name.
def scenarios():
    age_cutoff_ms = (NOW_MS // 1000 - 3 * HOUR) * 1000
    out = [
        {'name': 'defaults keep young files, drop those over 72 hours',
         'max_age_hours': 72, 'max_size_mb': 2048,
         'trash': [('jpeg_00.jpg', None, 73 * HOUR * 1000), ('jpeg_01.jpg', None, 72 * HOUR * 1000 + 1000),
                   ('jpeg_02.jpg', None, 72 * HOUR * 1000), ('jpeg_03.jpg', None, 71 * HOUR * 1000),
                   ('jpeg_04.jpg', None, 1000)]},
        {'name': 'age cutoff is a whole second, strictly older',
         'max_age_hours': 3, 'max_size_mb': None,
         'trash': [('jpeg_00.jpg', None, NOW_MS - (age_cutoff_ms - 1000)),
                   ('jpeg_01.jpg', None, NOW_MS - (age_cutoff_ms - 1)),
                   ('jpeg_02.jpg', None, NOW_MS - age_cutoff_ms),
                   ('jpeg_03.jpg', None, 3 * HOUR * 1000),
                   ('jpeg_04.jpg', None, 3 * HOUR * 1000 - 1),
                   ('jpeg_05.jpg', None, 2 * HOUR * 1000)]},
        {'name': 'no age limit keeps old files',
         'max_age_hours': None, 'max_size_mb': None,
         'trash': [('jpeg_00.jpg', None, 10000 * HOUR * 1000), ('bmp1', 2 * MB, 5000)]},
        {'name': 'a megabyte exactly is not over a megabyte',
         'max_age_hours': None, 'max_size_mb': 1,
         'trash': [('bmp1', MB, 5000)]},
        {'name': 'a byte over a megabyte is',
         'max_age_hours': None, 'max_size_mb': 1,
         'trash': [('bmp2', MB + 1, 5000)]},
        {'name': 'oldest first, eight at a time, size checked between',
         'max_age_hours': None, 'max_size_mb': 1,
         # twenty files of 100,000 bytes, trashed one second apart, not in
         # the order they were imported
         'trash': [(f'bmp{10 + i}', 100000, (1 + (i * 7) % 20) * 1000) for i in range(20)]},
        {'name': 'small files go with the big ones in a group',
         'max_age_hours': None, 'max_size_mb': 1,
         'trash': [('bmp3', 600000, 9000), ('jpeg_00.jpg', None, 8000), ('bmp4', 600000, 7000),
                   ('jpeg_01.jpg', None, 6000), ('png_00.png', None, 5000), ('bmp5', 300000, 4000),
                   ('jpeg_02.jpg', None, 3000), ('jpeg_03.jpg', None, 2000), ('jpeg_04.jpg', None, 1500),
                   ('bmp6', 300000, 1000)]},
        {'name': 'size first, then age',
         'max_age_hours': 1, 'max_size_mb': 1,
         'trash': [('bmp7', 700000, 30 * 60 * 1000), ('bmp8', 700000, 20 * 60 * 1000),
                   ('jpeg_00.jpg', None, 2 * HOUR * 1000), ('jpeg_01.jpg', None, 10 * 60 * 1000)]},
        {'name': 'zero megabytes empties the trash',
         'max_age_hours': None, 'max_size_mb': 0,
         'trash': [(name, None, (i + 1) * 1000) for i, name in enumerate(
             ['jpeg_00.jpg', 'jpeg_01.jpg', 'jpeg_02.jpg', 'jpeg_03.jpg', 'jpeg_04.jpg', 'jpeg_05.jpg',
              'jpeg_06.jpg', 'jpeg_07.jpg', 'jpeg_08.jpg', 'png_00.png', 'png_01.png'])]},
    ]
    return out


def run_scenario(scenario):
    import hydrus_driver, record_api
    manifest = json.load(open(os.path.join(HERE, 'fixtures', 'legacy_db', 'basic.manifest.json')))
    by_name = {f['name']: f['hash'] for f in manifest['files']}
    db_dir = record_api.unpack_fixture('basic')

    def hook(session):
        from hydrus.core import HydrusConstants as HC, HydrusTime
        from hydrus.client import ClientConstants as CC, ClientDaemons
        from hydrus.client.metadata import ClientContentUpdates
        c = session.controller
        real_time = HydrusTime.time
        held = [None]

        class HeldTime:
            # time.time() held where the script says; the rest is the real time module
            def __getattr__(self, name):
                return getattr(real_time, name)

            def time(self):
                return held[0] / 1000 if held[0] is not None else real_time.time()

        HydrusTime.time = HeldTime()

        def write(service_key, action, hashes):
            cu = ClientContentUpdates.ContentUpdate(HC.CONTENT_TYPE_FILES, action, hashes)
            c.WriteSynchronous('content_updates', ClientContentUpdates.ContentUpdatePackage.STATICCreateFromContentUpdate(service_key, cu))

        # empty the trash the fixture came with
        trashed = c.Read('trash_hashes')
        if trashed:
            write(CC.HYDRUS_LOCAL_FILE_STORAGE_SERVICE_KEY, HC.CONTENT_UPDATE_DELETE, trashed)
        names = {}
        for (name, size, _) in scenario['trash']:
            if size is None:
                names[name] = bytes.fromhex(by_name[name])
            else:
                path = os.path.join(db_dir, name + '.bmp')
                with open(path, 'wb') as f:
                    f.write(bmp(int(name[3:]), size))
                names[name] = bytes.fromhex(session.api.post('/add_files/add_file', {'path': path})['hash'])
        name_of = {h: n for n, h in names.items()}
        # trash them at their times, oldest first
        for (name, _, ago) in sorted(scenario['trash'], key=lambda t: -t[2]):
            held[0] = NOW_MS - ago
            write(CC.COMBINED_LOCAL_FILE_DOMAINS_SERVICE_KEY, HC.CONTENT_UPDATE_DELETE, [names[name]])
        held[0] = None

        def trash_rows():
            rows = {}
            for h in c.Read('trash_hashes'):
                rows[name_of.get(h, h.hex())] = True
            return rows

        info = {}
        for name, h in names.items():
            (media_result,) = c.Read('media_results', [h])
            lm = media_result.GetLocationsManager()
            info[name] = {'hash': h.hex(), 'size': media_result.GetSize(),
                          'trashed_ms': lm.GetTimesManager().GetImportedTimestampMS(CC.TRASH_SERVICE_KEY)}
        assert sorted(trash_rows()) == sorted(names), (trash_rows(), names)
        HC.options['trash_max_age'] = scenario['max_age_hours']
        HC.options['trash_max_size'] = scenario['max_size_mb']
        groups = []
        original_write = c.WriteSynchronous

        def logging_write(action, *args, **kwargs):
            if action == 'content_updates':
                package = args[0]
                for (key, updates) in package.IterateContentUpdates():
                    for u in updates:
                        groups.append([name_of.get(h, h.hex()) for h in u.GetHashes()])
            return original_write(action, *args, **kwargs)

        c.WriteSynchronous = logging_write
        real_daemon_time = ClientDaemons.time

        class NoRest:
            def __getattr__(self, name):
                return getattr(real_daemon_time, name)

            def sleep(self, seconds):
                pass

        ClientDaemons.time = NoRest()
        held[0] = NOW_MS
        try:
            ClientDaemons.DAEMONMaintainTrash()
        finally:
            held[0] = None
            c.WriteSynchronous = original_write
        left = sorted(trash_rows())
        return {'files': info, 'groups': groups, 'left': left}

    return hydrus_driver.run_client(db_dir, hook, port=PORT)


def main():
    import hydrus_driver
    if len(sys.argv) > 1 and sys.argv[1] == '--child':
        index, output = int(sys.argv[2]), sys.argv[3]
        result = run_scenario(scenarios()[index])
        with open(output, 'w') as f:
            json.dump(result, f)
        return
    out = {'now_ms': NOW_MS, 'scenarios': []}
    with tempfile.TemporaryDirectory() as tmp:
        for i, scenario in enumerate(scenarios()):
            path = os.path.join(tmp, f'{i}.json')
            hydrus_driver.run_in_subprocess(os.path.abspath(__file__), '--child', str(i), path)
            with open(path) as f:
                result = json.load(f)
            out['scenarios'].append({'name': scenario['name'], 'max_age_hours': scenario['max_age_hours'],
                                     'max_size_mb': scenario['max_size_mb'],
                                     'trash': [{'name': n, 'ago_ms': ago, **result['files'][n]} for (n, _, ago) in scenario['trash']],
                                     'groups': result['groups'], 'left': result['left']})
            print(scenario['name'], result['groups'], result['left'])
    with open(os.path.join(HERE, 'fixtures', 'trash_maintenance.json'), 'w') as f:
        json.dump(out, f, indent=1)
        f.write('\n')
    print('recorded trash maintenance')


if __name__ == '__main__':
    main()

#!/usr/bin/env python3
"""Record actual FilenameTaggingOptions.GetTags path rebasing/separator behavior.

A real Hydrus controller and tag-display manager consume the options. The
unchanged method is run with Python's standard posixpath and ntpath backends
scoped to its module, so the Windows cases are Python path-backend recordings,
not claims of having run Qt on Windows. No files or remote URLs are opened.
"""
import json
import os
import sys
import tempfile
from pathlib import Path
HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
OUT = HERE / 'fixtures' / 'filename_simple_paths.json'

def record(session):
    def work():
        import ntpath
        import posixpath
        from types import SimpleNamespace
        from hydrus.client import ClientConstants as CC
        from hydrus.client.importing.options import FilenameTaggingOptions as F
        reference = json.loads((HERE / 'fixtures' / 'filename_simple.json').read_text())
        options = F.FilenameTaggingOptions()
        options.SimpleSetTuple(set(), (False, ''), {-1: (True, 'series'), -2: (True, 'creator')})
        original_os = F.os
        cases = []
        try:
            for platform, path_module in [('posix', posixpath), ('windows', ntpath)]:
                F.os = SimpleNamespace(path=path_module, sep=path_module.sep)
                prefix = '/tmp/filename-bridge' if platform == 'posix' else r'C:\Temp\filename-bridge'
                for index, recorded in enumerate(reference['paths']):
                    parts = recorded.lstrip('/').split('/')
                    for label, components in [('rebased_with_srv', parts), ('rebased_without_srv', parts[1:])]:
                        path = path_module.join(prefix, *components)
                        cases.append({'platform': platform, 'name': label, 'recorded_index': index,
                                      'path': path, 'tags': sorted(options.GetTags(CC.DEFAULT_LOCAL_TAG_SERVICE_KEY, path))})
                    if platform == 'windows':
                        path = path_module.join(prefix, '/'.join(parts[1:]))
                        cases.append({'platform': platform, 'name': 'mixed_separator_tail', 'recorded_index': index,
                                      'path': path, 'tags': sorted(options.GetTags(CC.DEFAULT_LOCAL_TAG_SERVICE_KEY, path))})
        finally:
            F.os = original_os
        return {'consumer': 'FilenameTaggingOptions.GetTags with actual controller tag-display filtering',
                'path_backends': ['posixpath', 'ntpath'], 'directories': [[-1, 'series'], [-2, 'creator']], 'cases': cases}
    return session.controller.CallBlockingToQt(session.controller.gui, work)

def main():
    import hydrus_driver
    import record_api
    if len(sys.argv)>1 and sys.argv[1]=='--child':
        destination = sys.argv[2]
        result = hydrus_driver.run_client(record_api.unpack_fixture('basic'), record)
        Path(destination).write_text(json.dumps(result))
        return
    with tempfile.TemporaryDirectory() as directory:
        destination = Path(directory) / 'out.json'
        hydrus_driver.run_in_subprocess(str(Path(__file__).resolve()), '--child', str(destination))
        result = json.loads(destination.read_text())
    OUT.write_text(json.dumps(result, indent=2) + '\n')
    print('wrote', OUT)
if __name__=='__main__':
    main()

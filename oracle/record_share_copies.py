#!/usr/bin/env python3
"""Record the thumbnail menu's share > copy rows on three files: copy hashes
(sha256, md5, sha1, sha512, blurhash, pixel hashes), copy file ids and copy
paths, as the clipboard text the reference publishes for each (the real
`CopyHashesToClipboard`, `CopyFileIdsToClipboard` and
`CopyFilePathsToClipboard` on the real media), and the share menu's rows
for that selection.

Usage: QT_QPA_PLATFORM=offscreen ~/pyenv/bin/python oracle/record_share_copies.py
       (writes fixtures/share_copies.json)
"""
import json
import sys
import tempfile
from pathlib import Path
HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))


def record(session):
    from hydrus.client.gui.media import ClientGUIMediaModalActions as Modal, ClientGUIMediaSimpleActions as Simple
    from hydrus.client.media import ClientMediaSingle
    controller, gui = session.controller, session.controller.gui
    manifest = json.loads((HERE / 'fixtures/legacy_db/basic.manifest.json').read_text())
    hashes = [bytes.fromhex(f['hash']) for f in manifest['files'][:6]]

    def drive():
        medias = [ClientMediaSingle.MediaSingle(m) for m in controller.Read('media_results', hashes)]
        published = []
        original = controller.pub

        def pub(topic, *args, **kwargs):
            if topic == 'clipboard':
                published.append(list(args))
            else:
                original(topic, *args, **kwargs)
        controller.pub = pub
        rows = []
        try:
            for selection in (medias[:2], medias[2:3], medias):
                out = {'hashes': [m.GetHash().hex() for m in selection],
                    'ids': [m.GetMediaResult().GetHashId() for m in selection],
                    'mimes': [m.GetMime() for m in selection],
                    'blurhashes': [m.GetFileInfoManager().blurhash for m in selection],
                    'pixel_hashes': [(m.GetFileInfoManager().pixel_hash.hex() if m.GetFileInfoManager().pixel_hash is not None else None) for m in selection],
                    'copies': {}}
                for hash_type in ('sha256', 'md5', 'sha1', 'sha512', 'blurhash', 'pixel_hash'):
                    published.clear()
                    Modal.CopyHashesToClipboard(gui, hash_type, selection)
                    out['copies'][hash_type] = [list(p) for p in published]
                published.clear()
                Simple.CopyFileIdsToClipboard(selection)
                out['copies']['file ids'] = [list(p) for p in published]
                published.clear()
                Simple.CopyFilePathsToClipboard(selection)
                out['copies']['paths'] = [list(p) for p in published]
                rows.append(out)
        finally:
            controller.pub = original
        return rows
    return {'selections': controller.CallBlockingToQt(gui, drive)}


def main():
    import hydrus_driver, record_api
    if len(sys.argv) > 1 and sys.argv[1] == '--child':
        Path(sys.argv[2]).write_text(json.dumps(hydrus_driver.run_client(record_api.unpack_fixture('basic'), record)))
        return
    with tempfile.TemporaryDirectory() as work:
        out = Path(work) / 'share.json'
        hydrus_driver.run_in_subprocess(str(Path(__file__).resolve()), '--child', str(out))
        result = json.loads(out.read_text())
    (HERE / 'fixtures' / 'share_copies.json').write_text(json.dumps(result, indent=2) + '\n')


if __name__ == '__main__':
    main()

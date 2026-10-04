#!/usr/bin/env python3
"""Record the hash-prefix checkbox and actual single/multiple hash clipboard output.

The real FilesAndTrashPanel saves the setting and CopyHashesToClipboard reads it
for all six menu hash types. Clipboard publications and missing-digest warnings
are captured on basic fixture media; no generated hashes replace real database data.
"""
import json
import os
import sys
import tempfile

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)


def record(session):
    manifest = json.load(open(os.path.join(HERE, 'fixtures', 'legacy_db', 'basic.manifest.json')))
    hashes = [bytes.fromhex(file['hash']) for file in manifest['files'] if file['import_status'] == 1]
    controller = session.controller
    results = controller.Read('media_results', hashes)
    def qt():
        from hydrus.client.media import ClientMediaSingle
        from hydrus.client.gui.media import ClientGUIMediaModalActions
        from hydrus.client.gui import ClientGUIDialogsMessage
        from hydrus.client.gui.panels.options import FilesAndTrashPanel
        panel = FilesAndTrashPanel.FilesAndTrashPanel(controller.gui)
        default = panel._prefix_hash_when_copying.isChecked()
        media = [ClientMediaSingle.MediaSingle(result) for result in results]
        original_pub = controller.pub
        original_warning = ClientGUIDialogsMessage.ShowWarning
        original_prefix = controller.new_options.GetBoolean('prefix_hash_when_copying')
        cases = []
        try:
            for prefix in [False, True]:
                panel._prefix_hash_when_copying.setChecked(prefix)
                panel.UpdateOptions()
                for count in [1, len(media)]:
                    for kind in ['sha256', 'md5', 'sha1', 'sha512', 'blurhash', 'pixel_hash']:
                        clipboard = []
                        warnings = []
                        def pub(topic, *args, **kwargs):
                            if topic == 'clipboard':
                                clipboard.append(args[1])
                            elif topic != 'message':
                                return original_pub(topic, *args, **kwargs)
                        controller.pub = pub
                        ClientGUIDialogsMessage.ShowWarning = lambda win, text: warnings.append(text)
                        ClientGUIMediaModalActions.CopyHashesToClipboard(controller.gui, kind, media[:count])
                        cases.append({'prefix': prefix, 'kind': kind, 'hashes': [item.GetHash().hex() for item in media[:count]], 'clipboard': clipboard, 'warnings': warnings})
        finally:
            controller.pub = original_pub
            ClientGUIDialogsMessage.ShowWarning = original_warning
            controller.new_options.SetBoolean('prefix_hash_when_copying', original_prefix)
            panel.deleteLater()
        return {'default': default, 'cases': cases}
    return controller.CallBlockingToQt(controller.gui, qt)


def main():
    import hydrus_driver
    import record_api
    if len(sys.argv) > 1:
        output = sys.argv[2]
        result = hydrus_driver.run_client(record_api.unpack_fixture('basic'), record)
        with open(output, 'w') as stream:
            json.dump(result, stream)
        return
    with tempfile.TemporaryDirectory() as directory:
        path = os.path.join(directory, 'result.json')
        hydrus_driver.run_in_subprocess(os.path.abspath(__file__), '--child', path)
        with open(path) as stream:
            result = json.load(stream)
    output = os.path.join(HERE, 'fixtures', 'hash_clipboard_options.json')
    with open(output, 'w') as stream:
        json.dump(result, stream, indent=2)
        stream.write('\n')
    print('wrote ' + output)


if __name__ == '__main__':
    main()
